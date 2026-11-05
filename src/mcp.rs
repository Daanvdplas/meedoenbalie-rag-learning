use serde_json::{Value, json};
use std::io::{BufRead, Read, Write};

pub const MAX_SEARCH_CALLS: usize = 4;
const MAX_LINE_BYTES: u64 = 16_384;
const SEARCH_LIMIT_MESSAGE: &str = "Search call limit reached (four per Codex run)";

#[derive(Default)]
pub struct SearchPolicy {
    fixed_top_k: Option<usize>,
    description: Option<String>,
}

impl SearchPolicy {
    pub fn for_corpus(corpus: crate::corpus::CorpusId) -> Self {
        if corpus.is_baseline() {
            return Self::default();
        }
        Self {
            fixed_top_k: None,
            description: Some(format!(
                "Search {} fictional Dutch voluntary-activity documents. Returned exact sections preserve source_id, source_date and source_title. Section IDs repeat across documents. Search again with a focused query when information is missing, at most four calls. Never infer current availability or transport from unrelated evidence.",
                corpus.documents().len()
            )),
        }
    }

    pub fn fixed(top_k: usize, description: String) -> Result<Self, String> {
        if !(1..=6).contains(&top_k) || description.is_empty() || description.len() > 4000 {
            return Err("Invalid fixed retrieval tool policy".into());
        }
        Ok(Self {
            fixed_top_k: Some(top_k),
            description: Some(description),
        })
    }
}

pub fn handle_request<F>(request: Value, search: &mut F) -> Option<Value>
where
    F: FnMut(&str, usize) -> Result<Value, String>,
{
    handle_request_with_policy(request, search, &SearchPolicy::default())
}

fn rpc_error(id: Value, code: i32, message: &str) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
}

pub fn handle_request_with_policy<F>(
    request: Value,
    search: &mut F,
    policy: &SearchPolicy,
) -> Option<Value>
where
    F: FnMut(&str, usize) -> Result<Value, String>,
{
    let id = request.get("id")?.clone();
    if request["jsonrpc"] != "2.0" || !(id.is_string() || id.is_number() || id.is_null()) {
        return Some(rpc_error(id, -32600, "Invalid JSON-RPC request"));
    }
    let result = match request["method"].as_str() {
        Some("initialize") => {
            let version = request["params"]["protocolVersion"]
                .as_str()
                .unwrap_or("2025-06-18");
            let version = match version {
                "2024-11-05" | "2025-03-26" | "2025-06-18" | "2025-11-25" => version,
                _ => "2025-06-18",
            };
            json!({"protocolVersion":version,"capabilities":{"tools":{}},"serverInfo":{"name":"meedoen-retrieval","version":env!("CARGO_PKG_VERSION")}})
        }
        Some("ping") => json!({}),
        Some("tools/list") => {
            let mut listing = json!({"tools":[{
            "name":"search_document",
            "description":"Search five fictional Dutch voluntary-activity documents: gardening, bicycle repair, language cafe, cooking and walking. Each returned section has its own source_id, source_date and source_title. Section IDs repeat across documents; always preserve document ownership. Returns actual nearest source evidence, not proof every question is answerable. Use another focused query if evidence for part of the question is missing. Never infer current availability or transport from unrelated neighbours.",
            "inputSchema":{"type":"object","properties":{"query":{"type":"string","minLength":1,"maxLength":1000},"top_k":{"type":"integer","minimum":1,"maximum":6}},"required":["query","top_k"],"additionalProperties":false},
            "annotations":{"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}
            }]});
            if let Some(top_k) = policy.fixed_top_k {
                listing["tools"][0]["inputSchema"]["properties"]["top_k"]["enum"] = json!([top_k]);
            }
            if let Some(description) = &policy.description {
                listing["tools"][0]["description"] = json!(description);
            }
            listing
        }
        Some("tools/call") => {
            let params = &request["params"];
            let args = &params["arguments"];
            let valid = args.as_object().is_some_and(|args| {
                args.len() == 2 && args.contains_key("query") && args.contains_key("top_k")
            });
            let query = args["query"]
                .as_str()
                .and_then(|q| crate::validate_query(q).ok());
            let top_k = args["top_k"].as_u64().filter(|k| (1..=6).contains(k));
            if params["name"] != "search_document"
                || !valid
                || query.is_none()
                || top_k.is_none()
                || policy
                    .fixed_top_k
                    .is_some_and(|fixed| top_k != Some(fixed as u64))
            {
                let message = match policy.fixed_top_k {
                    Some(top_k) => format!("Expected search_document with query (1–1000 characters), fixed top_k={top_k}, and no extra arguments"),
                    None => "Expected search_document with query (1–1000 characters) and top_k (1–6), and no extra arguments".into(),
                };
                return Some(rpc_error(id, -32602, &message));
            }
            match search(&query.unwrap(), top_k.unwrap() as usize) {
                Ok(value) => {
                    json!({"content":[{"type":"text","text":value.to_string()}],"isError":false})
                }
                Err(error) => {
                    let message = if error == SEARCH_LIMIT_MESSAGE {
                        SEARCH_LIMIT_MESSAGE
                    } else {
                        "Local retrieval failed; check the model cache and restart the app"
                    };
                    json!({"content":[{"type":"text","text":message}],"isError":true})
                }
            }
        }
        _ => return Some(rpc_error(id, -32601, "Method not found")),
    };
    Some(json!({"jsonrpc":"2.0","id":id,"result":result}))
}

/// MCP stdout contains protocol responses only. The caller owns the fixed index.
pub fn serve<F>(reader: impl BufRead, writer: impl Write, search: F) -> Result<(), String>
where
    F: FnMut(&str, usize) -> Result<Value, String>,
{
    serve_with_policy(reader, writer, search, SearchPolicy::default())
}

pub fn serve_with_policy<F>(
    mut reader: impl BufRead,
    mut writer: impl Write,
    mut search: F,
    policy: SearchPolicy,
) -> Result<(), String>
where
    F: FnMut(&str, usize) -> Result<Value, String>,
{
    let mut calls = 0;
    let mut attempts = 0;
    let mut bounded = |query: &str, top_k: usize| {
        calls += 1;
        if calls > MAX_SEARCH_CALLS {
            return Err(SEARCH_LIMIT_MESSAGE.into());
        }
        search(query, top_k)
    };
    loop {
        let mut line = Vec::new();
        let read = reader
            .by_ref()
            .take(MAX_LINE_BYTES + 1)
            .read_until(b'\n', &mut line)
            .map_err(|e| e.to_string())?;
        if read == 0 {
            return Ok(());
        }
        if line.len() as u64 > MAX_LINE_BYTES {
            return Err("MCP request exceeds 16 KiB".into());
        }
        let response = match serde_json::from_slice(&line) {
            Ok(request) => {
                let request: Value = request;
                if policy.fixed_top_k.is_some() && request["method"] == "tools/call" {
                    attempts += 1;
                    if attempts > MAX_SEARCH_CALLS {
                        if let Some(id) = request.get("id") {
                            let response = json!({"jsonrpc":"2.0","id":id,"result":{"content":[{"type":"text","text":SEARCH_LIMIT_MESSAGE}],"isError":true}});
                            serde_json::to_writer(&mut writer, &response)
                                .map_err(|e| e.to_string())?;
                            writer
                                .write_all(b"\n")
                                .and_then(|_| writer.flush())
                                .map_err(|e| e.to_string())?;
                        }
                        continue;
                    }
                }
                handle_request_with_policy(request, &mut bounded, &policy)
            }
            Err(_) => Some(rpc_error(Value::Null, -32700, "Invalid JSON")),
        };
        if let Some(response) = response {
            serde_json::to_writer(&mut writer, &response).map_err(|e| e.to_string())?;
            writer
                .write_all(b"\n")
                .and_then(|_| writer.flush())
                .map_err(|e| e.to_string())?;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn fixed_policy_counts_invalid_attempts_towards_session_limit() {
        let requests: Vec<_> = (0..5).map(|id| json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":"search_document","arguments":{"query":"Wanneer?","top_k":if id<4 {6} else {3}}}}).to_string()).collect();
        let mut output = Vec::new();
        let mut invoked = 0;
        serve_with_policy(
            std::io::Cursor::new(requests.join("\n") + "\n"),
            &mut output,
            |_, _| {
                invoked += 1;
                Ok(json!({"hits":[]}))
            },
            SearchPolicy::fixed(3, "fixed".into()).unwrap(),
        )
        .unwrap();
        assert_eq!(
            invoked, 0,
            "Invalid attempts must consume the fixed session allowance"
        );
    }

    #[test]
    fn fixed_search_policy_is_advertised_and_rejects_treatment_changes_before_search() {
        let policy = SearchPolicy::fixed(
            3,
            "Keyword search in the selected SQLite activity structure".into(),
        )
        .unwrap();
        let mut never = |_: &str, _: usize| panic!("A changed top-k must never reach retrieval");
        let list = handle_request_with_policy(
            json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}),
            &mut never,
            &policy,
        )
        .unwrap();
        assert_eq!(
            list["result"]["tools"][0]["inputSchema"]["properties"]["top_k"]["enum"],
            json!([3])
        );
        let response = handle_request_with_policy(json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"search_document","arguments":{"query":"Waar?","top_k":6}}}), &mut never, &policy).unwrap();
        assert_eq!(response["error"]["code"], -32602);
        assert!(SearchPolicy::fixed(7, "invalid".into()).is_err());
    }

    #[test]
    fn initializes_and_advertises_only_read_only_search() {
        let mut never = |_: &str, _: usize| panic!("discovery cannot search");
        let init = handle_request(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}), &mut never).unwrap();
        assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
        let list = handle_request(
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
            &mut never,
        )
        .unwrap();
        let tools = list["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["name"], "search_document");
        assert_eq!(tools[0]["annotations"]["readOnlyHint"], true);
        assert!(!list.to_string().contains("expectations"));
    }

    #[test]
    fn routes_validated_query_and_top_k_to_retrieval() {
        let mut observed = None;
        let mut search = |query: &str, top_k: usize| {
            observed = Some((query.to_owned(), top_k));
            Ok(json!({"hits":[{"section":{"id":"S2","text":"Dinsdag"}}]}))
        };
        let result = handle_request(json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"search_document","arguments":{"query":"  Wanneer?  ","top_k":2}}}), &mut search).unwrap();
        assert_eq!(result["result"]["isError"], false);
        assert_eq!(observed, Some(("Wanneer?".into(), 2)));
        assert!(
            result["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("S2")
        );
    }

    #[test]
    fn rejects_invalid_arguments_before_search() {
        let mut never = |_: &str, _: usize| panic!("invalid input must not search");
        for args in [
            json!({"query":"","top_k":3}),
            json!({"query":"x","top_k":7}),
            json!({"query":"x","top_k":0}),
            json!({"query":"x","top_k":1,"path":"/tmp/gold"}),
            json!({"query":"x","top_k":1.5}),
        ] {
            let response=handle_request(json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"search_document","arguments":args}}), &mut never).unwrap();
            assert_eq!(response["error"]["code"], -32602);
        }
    }

    #[test]
    fn retrieval_errors_do_not_expose_host_diagnostics_to_codex() {
        let mut fail =
            |_: &str, _: usize| Err("FAKE-PRIVATE-TOKEN /Users/FAKE-PRIVATE-HOME/cache".into());
        let result = handle_request(json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"search_document","arguments":{"query":"Wanneer?","top_k":3}}}), &mut fail).unwrap();
        assert_eq!(result["result"]["isError"], true);
        assert!(!result.to_string().contains("FAKE-PRIVATE"));
        assert_eq!(
            result["result"]["content"][0]["text"],
            "Local retrieval failed; check the model cache and restart the app"
        );
    }

    #[test]
    fn call_limit_remains_distinct_from_a_private_retrieval_error() {
        let request = json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"search_document","arguments":{"query":"Wanneer?","top_k":3}}}).to_string();
        let input = format!("{}\n", vec![request; MAX_SEARCH_CALLS + 1].join("\n"));
        let mut output = Vec::new();
        serve(std::io::Cursor::new(input), &mut output, |_, _| {
            Ok(json!({"hits":[]}))
        })
        .unwrap();
        let replies: Vec<Value> = std::str::from_utf8(&output)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert!(
            replies[..MAX_SEARCH_CALLS]
                .iter()
                .all(|reply| reply["result"]["isError"] == false)
        );
        assert_eq!(replies[MAX_SEARCH_CALLS]["result"]["isError"], true);
        assert_eq!(
            replies[MAX_SEARCH_CALLS]["result"]["content"][0]["text"],
            SEARCH_LIMIT_MESSAGE
        );
    }

    #[test]
    fn search_failure_is_an_explicit_tool_error() {
        let mut fail = |_: &str, _: usize| Err("Model unavailable".into());
        let result = handle_request(json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"search_document","arguments":{"query":"Wanneer?","top_k":3}}}), &mut fail).unwrap();
        assert_eq!(result["result"]["isError"], true);
        assert!(result.to_string().contains("Local retrieval failed"));
    }
}
