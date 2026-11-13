//! The web demo's Rig agent, with a bounded, stateless Responses API transport.
//! CLI and experiment entry points continue to use the separate Codex runner.
mod rig_runtime;
mod transport;
use crate::{
    DemoSearchMethod,
    codex::{AnswerLanguage, ToolCall},
    corpus::CorpusId,
    experiment::types::TokenUsage,
};
use reqwest::{
    Client, Url,
    header::{AUTHORIZATION, HeaderMap, HeaderValue},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{io::Read, path::Path, sync::Arc, time::Duration};

const ENDPOINT: &str = "https://api.openai.com/v1/responses";
const MODEL: &str = "gpt-6.1-sol";
const TIMEOUT: Duration = Duration::from_secs(120);
const MAX_RESPONSE_BYTES: usize = 512 * 1024;
const MAX_HISTORY_BYTES: usize = 1024 * 1024;
const MAX_OUTPUT_TOKENS: usize = 4096;
const MOCK_KEY: &str = "coa-local-test-key";
pub type Search = Arc<dyn Fn(&str, usize, DemoSearchMethod) -> Result<Value, String> + Send + Sync>;

#[derive(Clone)]
pub struct ResponsesRunner {
    client: Client,
    endpoint: Url,
    provider: rig::providers::openai::OpenAIConfig,
    search: Search,
    timeout: Duration,
    offline: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchArguments {
    query: String,
    top_k: usize,
}
impl ResponsesRunner {
    /// Endpoints are restricted to OpenAI or an explicit loopback development server.
    /// The web factory below never sends a real credential to a test endpoint.
    pub fn new(api_key: String, endpoint: &str, search: Search) -> Result<Self, String> {
        let endpoint = Url::parse(endpoint).map_err(|_| "runtime: Ongeldig API-adres.")?;
        let offline = endpoint.scheme() == "http"
            && matches!(endpoint.host_str(), Some("127.0.0.1" | "[::1]" | "::1"));
        if (endpoint.as_str() != ENDPOINT && !offline)
            || endpoint.path() != "/v1/responses"
            || endpoint.query().is_some()
            || endpoint.fragment().is_some()
            || !endpoint.username().is_empty()
            || endpoint.password().is_some()
        {
            return Err("runtime: API-adres is niet toegestaan.".into());
        }
        let key = validate_key(api_key)?;
        let provider = rig::providers::openai::OpenAIConfig::new(key.clone())
            .with_base_url(endpoint.as_str().trim_end_matches("/responses"));
        if offline && (!cfg!(debug_assertions) || key != MOCK_KEY) {
            return Err("runtime: Een lokale test accepteert uitsluitend de vaste testsleutel in een debugbuild.".into());
        }
        let mut bearer = HeaderValue::from_str(&format!("Bearer {key}"))
            .map_err(|_| "runtime: Ongeldige API-sleutel.")?;
        bearer.set_sensitive(true);
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, bearer);
        let client = Client::builder()
            .default_headers(headers)
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(10))
            .timeout(TIMEOUT)
            .build()
            .map_err(|_| "runtime: Antwoordservice kan niet worden gestart.")?;
        Ok(Self {
            client,
            endpoint,
            provider,
            search,
            timeout: TIMEOUT,
            offline,
        })
    }
    pub fn from_env(search: Search) -> Result<Self, String> {
        if let Ok(url) = std::env::var("COA_RESPONSES_TEST_URL") {
            if !cfg!(debug_assertions) || !url.starts_with("http://") {
                return Err(
                    "runtime: Testconfiguratie is alleen lokaal in een debugbuild toegestaan."
                        .into(),
                );
            }
            // Never read a user's real API key in offline test mode.
            let mut runner = Self::new(MOCK_KEY.into(), &url, search)?;
            if !runner.offline {
                return Err("runtime: Testadres moet loopback zijn.".into());
            }
            if let Ok(value) = std::env::var("COA_RESPONSES_TEST_TIMEOUT_MS") {
                let milliseconds = value
                    .parse::<u64>()
                    .ok()
                    .filter(|n| (1..=120_000).contains(n))
                    .ok_or("runtime: Ongeldige testdeadline.")?;
                runner.timeout = Duration::from_millis(milliseconds);
            }
            return Ok(runner);
        }
        let key = match std::env::var("OPENAI_API_KEY")
            .ok()
            .filter(|key| !key.trim().is_empty())
        {
            Some(key) => key,
            None => {
                let path = std::env::var_os("COA_OPENAI_API_KEY_FILE")
                    .ok_or("runtime: De antwoordservice is nog niet ingesteld. Een OpenAI API-sleutel ontbreekt.")?;
                read_key(Path::new(&path))?
            }
        };
        Self::new(key, ENDPOINT, search)
    }
    pub fn configuration(&self) -> Value {
        json!({"provider":"openai","transport":"responses_api","runtime":"rig-0.46.0","model":MODEL,"reasoning_effort":"low","store":false,"prompt_version":"responses-activity-v1","max_searches":4,"max_provider_requests":5,"max_output_tokens_per_request":MAX_OUTPUT_TOKENS,"timeout_ms":self.timeout.as_millis(),"offline_mock":self.offline})
    }
    pub async fn run(
        &self,
        query: &str,
        top_k: usize,
        corpus: CorpusId,
        language: AnswerLanguage,
        method: DemoSearchMethod,
    ) -> Result<Value, String> {
        let query = crate::validate_query(query).map_err(|_| "input: Ongeldige vraag.")?;
        crate::ranking::validate_top_k(top_k)
            .map_err(|_| "input: Ongeldig aantal bronpassages.")?;
        tokio::time::timeout(
            self.timeout,
            self.run_inner(&query, top_k, corpus, language, method),
        )
        .await
        .map_err(|_| "runtime: De antwoordservice heeft te lang gewacht.".to_owned())?
    }
    async fn run_inner(
        &self,
        query: &str,
        top_k: usize,
        corpus: CorpusId,
        language: AnswerLanguage,
        method: DemoSearchMethod,
    ) -> Result<Value, String> {
        rig_runtime::run(self, query, top_k, corpus, language, method).await
    }
}

fn validate_key(key: String) -> Result<String, String> {
    let key = key.trim();
    if !(16..=4096).contains(&key.len()) || !key.bytes().all(|b| b.is_ascii_graphic()) {
        return Err("runtime: Ongeldige API-sleutel.".into());
    }
    Ok(key.to_owned())
}
fn read_key(path: &Path) -> Result<String, String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(path)
            .map_err(|_| "runtime: Het API-sleutelbestand is niet beschikbaar.")?;
        let meta = file
            .metadata()
            .map_err(|_| "runtime: Het API-sleutelbestand is niet beschikbaar.")?;
        if !meta.is_file()
            || meta.nlink() != 1
            || meta.uid() != unsafe { libc::geteuid() }
            || meta.mode() & 0o077 != 0
        {
            return Err("runtime: Het API-sleutelbestand moet een eigen regulier bestand met rechten 0600 zijn.".into());
        }
        let mut key = String::new();
        (&mut file)
            .take(4097)
            .read_to_string(&mut key)
            .map_err(|_| "runtime: Het API-sleutelbestand kan niet worden gelezen.")?;
        validate_key(key)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Err("runtime: Gebruik OPENAI_API_KEY op dit systeem.".into())
    }
}
fn add_usage(total: &mut Option<TokenUsage>, value: &Value) {
    let current = (|| {
        Some(TokenUsage {
            input_tokens: value["input_tokens"].as_u64()?,
            output_tokens: value["output_tokens"].as_u64()?,
            cached_input_tokens: value["input_tokens_details"]["cached_tokens"].as_u64(),
            reasoning_output_tokens: value["output_tokens_details"]["reasoning_tokens"].as_u64(),
        })
    })();
    *total = total
        .as_ref()
        .zip(current.as_ref())
        .and_then(|(sum, current)| {
            Some(TokenUsage {
                input_tokens: sum.input_tokens.checked_add(current.input_tokens)?,
                output_tokens: sum.output_tokens.checked_add(current.output_tokens)?,
                cached_input_tokens: sum
                    .cached_input_tokens
                    .zip(current.cached_input_tokens)
                    .and_then(|(a, b)| a.checked_add(b)),
                reasoning_output_tokens: sum
                    .reasoning_output_tokens
                    .zip(current.reasoning_output_tokens)
                    .and_then(|(a, b)| a.checked_add(b)),
            })
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Json, Router, extract::State, routing::post};
    use serde_json::{Value, json};
    use std::{
        collections::VecDeque,
        sync::{
            Arc, Mutex,
            atomic::{AtomicUsize, Ordering},
        },
        time::Duration,
    };

    fn evidence(method: crate::DemoSearchMethod) -> Value {
        json!({"sources":[{"id":"ACT-TUIN-001","date":"2026-10-20"}],"as_of":"2026-11-05","manifest":{"corpus":"synthetic-ten"},"retrieval_method":method,"hits":[{"section":{"source_id":"ACT-TUIN-001","source_date":"2026-10-20","id":"S2","text":"Iedere dinsdag van 10.00 tot 12.00 uur."},"similarity":0.8}]})
    }
    fn search() -> Search {
        Arc::new(|_, _, method| Ok(evidence(method)))
    }
    fn envelope(output: Value) -> Value {
        json!({"id":"resp_synthetic","object":"response","status":"completed","model":"gpt-6.1-sol","output":output,"usage":{"input_tokens":100,"input_tokens_details":{"cached_tokens":20},"output_tokens":25,"output_tokens_details":{"reasoning_tokens":5}}})
    }
    fn call(id: &str, query: &str) -> Value {
        json!({"type":"function_call","id":format!("fc_{id}"),"call_id":id,"name":"search_document","arguments":json!({"query":query,"top_k":3}).to_string(),"status":"completed"})
    }
    fn final_answer() -> Value {
        json!({"type":"message","role":"assistant","status":"completed","content":[{"type":"output_text","text":json!({"text":"Dinsdag van 10 tot 12 uur.","citations":[{"source_id":"ACT-TUIN-001","source_date":"2026-10-20","section_id":"S2","quote":"Iedere dinsdag van 10.00 tot 12.00 uur."}],"unknowns":[]}).to_string(),"annotations":[]}]})
    }
    async fn mock(
        responses: Vec<Value>,
    ) -> (String, Arc<Mutex<Vec<Value>>>, tokio::task::JoinHandle<()>) {
        type Script = (Arc<Mutex<VecDeque<Value>>>, Arc<Mutex<Vec<Value>>>);
        async fn respond(
            State((script, seen)): State<Script>,
            Json(body): Json<Value>,
        ) -> Json<Value> {
            seen.lock().unwrap().push(body);
            Json(
                script
                    .lock()
                    .unwrap()
                    .pop_front()
                    .expect("no unexpected provider requests"),
            )
        }
        let seen = Arc::new(Mutex::new(Vec::new()));
        let script = Arc::new(Mutex::new(responses.into()));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/v1/responses", listener.local_addr().unwrap());
        let app = Router::new()
            .route("/v1/responses", post(respond))
            .with_state((script, seen.clone()));
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        (endpoint, seen, task)
    }
    async fn run(runner: &ResponsesRunner) -> Result<Value, String> {
        runner
            .run(
                "Wanneer tuinieren?",
                3,
                crate::corpus::CorpusId::ExpandedTen,
                crate::codex::AnswerLanguage::En,
                crate::DemoSearchMethod::Bm25,
            )
            .await
    }

    #[tokio::test]
    async fn duplicate_tool_identity_is_rejected_before_a_second_search() {
        let (url, seen, task) = mock(vec![
            envelope(json!([call("same", "tuinieren")])),
            envelope(json!([call("same", "tuinieren tijd")])),
        ])
        .await;
        let count = Arc::new(AtomicUsize::new(0));
        let calls = count.clone();
        let runner = ResponsesRunner::new(
            MOCK_KEY.into(),
            &url,
            Arc::new(move |_, _, method| {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(evidence(method))
            }),
        )
        .unwrap();
        assert!(run(&runner).await.unwrap_err().starts_with("tool:"));
        assert_eq!(count.load(Ordering::SeqCst), 1);
        assert_eq!(seen.lock().unwrap().len(), 2);
        task.abort();
    }
    #[tokio::test]
    async fn parallel_tool_response_is_rejected_before_any_search() {
        let (url, seen, task) = mock(vec![envelope(json!([
            call("one", "tuinieren"),
            call("two", "tuinieren tijd")
        ]))])
        .await;
        let count = Arc::new(AtomicUsize::new(0));
        let calls = count.clone();
        let runner = ResponsesRunner::new(
            MOCK_KEY.into(),
            &url,
            Arc::new(move |_, _, method| {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(evidence(method))
            }),
        )
        .unwrap();
        assert!(run(&runner).await.unwrap_err().starts_with("tool:"));
        assert_eq!(count.load(Ordering::SeqCst), 0);
        assert_eq!(seen.lock().unwrap().len(), 1);
        task.abort();
    }
    #[tokio::test]
    async fn oversized_evidence_never_starts_another_provider_request() {
        let (url, seen, task) = mock(vec![envelope(json!([call("one", "tuinieren")]))]).await;
        let runner = ResponsesRunner::new(
            MOCK_KEY.into(),
            &url,
            Arc::new(|_, _, method| {
                let mut result = evidence(method);
                result["synthetic_padding"] = json!("x".repeat(MAX_HISTORY_BYTES));
                Ok(result)
            }),
        )
        .unwrap();
        assert!(run(&runner).await.unwrap_err().starts_with("output:"));
        assert_eq!(seen.lock().unwrap().len(), 1);
        task.abort();
    }
    #[tokio::test]
    async fn null_assistant_phase_is_a_valid_final_answer() {
        let mut answer = final_answer();
        answer["phase"] = Value::Null;
        let (url, _, task) = mock(vec![
            envelope(json!([call("one", "tuinieren")])),
            envelope(json!([answer])),
        ])
        .await;
        let runner = ResponsesRunner::new(MOCK_KEY.into(), &url, search()).unwrap();
        assert!(
            run(&runner).await.is_ok(),
            "a null phase must behave like an omitted phase"
        );
        task.abort();
    }
    #[tokio::test]
    async fn encrypted_reasoning_and_commentary_survive_two_tool_rounds_without_becoming_public() {
        let reasoning = |id: &str| json!({"type":"reasoning","id":id,"summary":[{"type":"summary_text","text":"private-synthetic-summary"}],"encrypted_content":format!("private-encrypted-{id}")});
        let commentary = |id: &str| json!({"type":"message","id":id,"role":"assistant","phase":"commentary","status":"completed","content":[{"type":"output_text","text":"I will inspect the source.","annotations":[]}]});
        let first = vec![
            reasoning("one"),
            commentary("one"),
            call("one", "tuinieren tijd"),
        ];
        let second = vec![
            reasoning("two"),
            commentary("two"),
            call("two", "tuinieren wanneer"),
        ];
        let mut final_item = final_answer();
        final_item["phase"] = json!("final_answer");
        let (url, seen, task) = mock(vec![
            envelope(json!(first)),
            envelope(json!(second)),
            envelope(json!([reasoning("three"), commentary("three"), final_item])),
        ])
        .await;
        let runner = ResponsesRunner::new(MOCK_KEY.into(), &url, search()).unwrap();
        let result = run(&runner)
            .await
            .expect("commentary is not the structured final answer");
        let requests = seen.lock().unwrap();
        assert_eq!(requests.len(), 3);
        let one = requests[1]["input"].as_array().unwrap();
        assert_eq!(&one[1..4], first.as_slice());
        let two = requests[2]["input"].as_array().unwrap();
        assert_eq!(&two[..one.len()], one.as_slice());
        assert_eq!(&two[one.len()..one.len() + 3], second.as_slice());
        assert_eq!(two[0]["content"][0]["text"], "Wanneer tuinieren?");
        assert!(
            !result.to_string().contains("private-encrypted")
                && !result.to_string().contains("private-synthetic-summary")
        );
        task.abort();
    }
    #[test]
    fn real_credentials_cannot_be_configured_for_a_loopback_mock() {
        assert!(
            ResponsesRunner::new(
                "a-real-looking-private-key".into(),
                "http://127.0.0.1:8891/v1/responses",
                search()
            )
            .is_err()
        );
    }
    #[tokio::test]
    async fn oversized_chunked_and_incomplete_json_bodies_are_rejected_safely() {
        use axum::body::{Body, Bytes};
        use axum::response::Response;
        for large in [true, false] {
            let app = Router::new().route(
                "/v1/responses",
                post(move || async move {
                    let payload = if large {
                        format!(
                            "{{\"private-provider-body\":\"{}\"}}",
                            "x".repeat(MAX_RESPONSE_BYTES)
                        )
                    } else {
                        "{\"private-provider-body\":".to_owned()
                    };
                    let chunks: Vec<Result<Bytes, std::io::Error>> = payload
                        .as_bytes()
                        .chunks(4096)
                        .map(|chunk| Ok(Bytes::copy_from_slice(chunk)))
                        .collect();
                    let stream = futures_util::stream::iter(chunks);
                    Response::builder()
                        .header("content-type", "application/json")
                        .body(Body::from_stream(stream))
                        .unwrap()
                }),
            );
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let url = format!("http://{}/v1/responses", listener.local_addr().unwrap());
            let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
            let runner = ResponsesRunner::new(MOCK_KEY.into(), &url, search()).unwrap();
            let error = run(&runner).await.unwrap_err();
            assert!(error.starts_with("output:"));
            assert!(!error.contains("private-provider-body"));
            task.abort();
        }
    }
    #[tokio::test]
    async fn stateless_tool_loop_keeps_real_evidence_and_sums_all_provider_usage() {
        let reasoning = json!({"type":"reasoning","id":"rs_synthetic","summary":[],"encrypted_content":"synthetic-encrypted-private"});
        let (url, seen, task) = mock(vec![
            envelope(json!([reasoning, call("one", "tuinieren tijd")])),
            envelope(json!([final_answer()])),
        ])
        .await;
        let runner = ResponsesRunner::new(MOCK_KEY.into(), &url, search()).unwrap();
        let result = run(&runner).await.unwrap();
        assert_eq!(runner.configuration()["runtime"], "rig-0.46.0");
        assert_eq!(result["client_version"], "rig-0.46.0");
        assert_eq!(result["provider"], "openai");
        assert_eq!(result["transport"], "responses_api");
        assert_eq!(result["provider_requests"], 2);
        assert_eq!(result["usage"]["input_tokens"], 200);
        assert_eq!(result["usage"]["output_tokens"], 50);
        assert_eq!(result["usage"]["reasoning_output_tokens"], 10);
        assert_eq!(result["tool_calls"][0]["query"], "tuinieren tijd");
        assert_eq!(
            result["tool_calls"][0]["result"],
            evidence(crate::DemoSearchMethod::Bm25)
        );
        let public = result.to_string();
        assert!(
            !public.contains("synthetic-api-key")
                && !public.contains("synthetic-encrypted-private")
        );
        let requests = seen.lock().unwrap();
        assert_eq!(requests.len(), 2);
        for request in requests.iter() {
            assert_eq!(request["model"], "gpt-6.1-sol");
            assert_eq!(request["store"], false);
            assert_eq!(request["reasoning"]["effort"], "low");
            assert_eq!(request["parallel_tool_calls"], false);
            assert_eq!(request["text"]["format"]["strict"], true);
            assert_eq!(request["tools"].as_array().unwrap().len(), 1);
            assert!(request.get("previous_response_id").is_none());
            assert!(
                request["instructions"]
                    .as_str()
                    .unwrap()
                    .contains("untrusted DATA")
            );
            assert!(
                request["instructions"]
                    .as_str()
                    .unwrap()
                    .contains("in English")
            );
        }
        assert_eq!(requests[0]["tool_choice"], "required");
        assert!(
            requests[1]["input"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["encrypted_content"] == "synthetic-encrypted-private")
        );
        assert!(
            requests[1]["input"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["type"] == "function_call_output" && item["call_id"] == "one")
        );
        task.abort();
    }
    #[tokio::test]
    async fn unknown_or_invalid_tool_calls_never_execute_local_code() {
        for bad in [
            json!({"name":"shell","arguments":"{}"}),
            json!({"name":"search_document","arguments":"{\"query\":\"ok\",\"top_k\":3,\"path\":\"/etc/shadow\"}"}),
            json!({"name":"search_document","arguments":"{\"query\":\"\",\"top_k\":3}"}),
            json!({"name":"search_document","arguments":"{\"query\":"}),
        ] {
            let mut item = call("one", "ok");
            item["name"] = bad["name"].clone();
            item["arguments"] = bad["arguments"].clone();
            let (url, seen, task) = mock(vec![envelope(json!([item]))]).await;
            let calls = Arc::new(AtomicUsize::new(0));
            let count = calls.clone();
            let runner = ResponsesRunner::new(
                MOCK_KEY.into(),
                &url,
                Arc::new(move |_, _, method| {
                    count.fetch_add(1, Ordering::SeqCst);
                    Ok(evidence(method))
                }),
            )
            .unwrap();
            assert!(run(&runner).await.unwrap_err().starts_with("tool:"));
            assert_eq!(calls.load(Ordering::SeqCst), 0);
            assert_eq!(
                seen.lock().unwrap().len(),
                1,
                "invalid tools must not cause a retry"
            );
            task.abort();
        }
    }
    #[tokio::test]
    async fn four_searches_are_the_limit_and_fifth_request_must_finish() {
        for last in [final_answer(), call("five", "do not execute")] {
            let mut script: Vec<Value> = (0..4)
                .map(|i| envelope(json!([call(&format!("call{i}"), "tuinieren")])))
                .collect();
            script.push(envelope(json!([last.clone()])));
            let (url, seen, task) = mock(script).await;
            let count = Arc::new(AtomicUsize::new(0));
            let n = count.clone();
            let runner = ResponsesRunner::new(
                MOCK_KEY.into(),
                &url,
                Arc::new(move |_, _, method| {
                    n.fetch_add(1, Ordering::SeqCst);
                    Ok(evidence(method))
                }),
            )
            .unwrap();
            let result = run(&runner).await;
            assert_eq!(result.is_ok(), last["type"] == "message");
            assert_eq!(count.load(Ordering::SeqCst), 4);
            assert_eq!(seen.lock().unwrap()[4]["tool_choice"], "none");
            task.abort();
        }
    }
    #[tokio::test]
    async fn invented_citations_and_refusals_or_incomplete_responses_fail_closed() {
        let mut wrong = final_answer();
        wrong["content"][0]["text"] = json!({"text":"Monday","citations":[{"source_id":"ACT-TUIN-001","source_date":"2026-10-20","section_id":"S2","quote":"Not in source"}],"unknowns":[]}).to_string().into();
        let mut incomplete = envelope(json!([final_answer()]));
        incomplete["status"] = json!("incomplete");
        for response in [
            envelope(json!([wrong])),
            incomplete,
            envelope(
                json!([{"type":"message","role":"assistant","content":[{"type":"refusal","refusal":"synthetic-private-detail"}]}]),
            ),
        ] {
            let (url, _, task) =
                mock(vec![envelope(json!([call("one", "tuinieren")])), response]).await;
            let runner = ResponsesRunner::new(MOCK_KEY.into(), &url, search()).unwrap();
            let error = run(&runner).await.unwrap_err();
            assert!(!error.contains("synthetic-private-detail"));
            task.abort();
        }
    }
    #[tokio::test]
    async fn provider_error_details_and_redirects_cannot_escape_or_forward_the_key() {
        use axum::{http::StatusCode, response::IntoResponse};
        for status in [
            StatusCode::UNAUTHORIZED,
            StatusCode::TOO_MANY_REQUESTS,
            StatusCode::INTERNAL_SERVER_ERROR,
            StatusCode::TEMPORARY_REDIRECT,
        ] {
            let app = Router::new().route(
                "/v1/responses",
                post(move || async move {
                    (
                        status,
                        [("location", "http://127.0.0.1:1/stolen")],
                        "synthetic-api-key private provider diagnostics",
                    )
                        .into_response()
                }),
            );
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let url = format!("http://{}/v1/responses", listener.local_addr().unwrap());
            let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
            let runner = ResponsesRunner::new(MOCK_KEY.into(), &url, search()).unwrap();
            let error = run(&runner).await.unwrap_err();
            assert!(error.starts_with("runtime:"));
            assert!(
                !error.contains("private")
                    && !error.contains("synthetic-api-key")
                    && !error.contains("127.0.0.1")
            );
            task.abort();
        }
    }
    #[tokio::test]
    async fn deadline_covers_the_whole_tool_loop() {
        let count = Arc::new(AtomicUsize::new(0));
        let second_started = Arc::new(tokio::sync::Notify::new());
        let seen = count.clone();
        let started = second_started.clone();
        let app = Router::new().route(
            "/v1/responses",
            post(move || {
                let seen = seen.clone();
                let started = started.clone();
                async move {
                    if seen.fetch_add(1, Ordering::SeqCst) == 0 {
                        Json(envelope(json!([call("one", "tuinieren")])))
                    } else {
                        started.notify_one();
                        // A per-request HTTP timeout is 120 seconds. Only the
                        // runner's one shared deadline can finish this run.
                        std::future::pending::<()>().await;
                        unreachable!()
                    }
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/v1/responses", listener.local_addr().unwrap());
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let mut runner = ResponsesRunner::new(MOCK_KEY.into(), &url, search()).unwrap();
        runner.timeout = Duration::from_secs(1);
        let execution = tokio::spawn(async move { run(&runner).await });
        tokio::time::timeout(Duration::from_secs(1), second_started.notified())
            .await
            .expect("the second request starts before the shared deadline");
        let result = tokio::time::timeout(Duration::from_secs(2), execution)
            .await
            .expect("the shared deadline cancels the blocked second request")
            .unwrap();
        assert_eq!(
            result.unwrap_err(),
            "runtime: De antwoordservice heeft te lang gewacht."
        );
        assert_eq!(count.load(Ordering::SeqCst), 2);
        task.abort();
    }
}
