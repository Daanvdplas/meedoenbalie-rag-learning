//! Parent-owned audit sidecar. Only the explicitly projected JSON reaches MCP stdout.
use crate::codex::CodexRun;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs::{File, OpenOptions},
    io::Write,
    path::PathBuf,
};

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PayloadMode {
    #[default]
    Full,
    Compact,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransportConfig {
    pub payload_mode: PayloadMode,
    pub session_id: String,
    pub trace_path: PathBuf,
}
impl TransportConfig {
    pub fn validate(&self) -> Result<(), String> {
        if !self.trace_path.is_absolute()
            || self.session_id.is_empty()
            || self.session_id.len() > 128
            || !self
                .session_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
        {
            return Err("Invalid fixed MCP trace identity or location".into());
        }
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AuditRecord {
    schema_version: u32,
    session_id: String,
    call_index: usize,
    request_id: String,
    trace_id: String,
    payload_mode: PayloadMode,
    payload_bytes: usize,
    full_response_sha256: String,
    full_response: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TraceSummary {
    pub session_id: String,
    pub path: PathBuf,
    pub sha256: String,
    pub trace_bytes: usize,
    /// UTF-8 JSON text placed inside successful MCP content blocks, excluding the RPC envelope.
    pub payload_bytes: usize,
    pub successful_searches: usize,
}

pub struct TraceWriter {
    config: TransportConfig,
    file: File,
    calls: usize,
}
impl TraceWriter {
    pub fn open(config: TransportConfig) -> Result<Self, String> {
        config.validate()?;
        // The CLI parent creates this file. Never create/truncate a model-selected location.
        let meta = std::fs::symlink_metadata(&config.trace_path)
            .map_err(|_| "Missing parent-owned MCP trace")?;
        if !meta.is_file() || meta.len() != 0 {
            return Err("MCP trace must be a new empty regular file".into());
        }
        let file = OpenOptions::new()
            .append(true)
            .open(&config.trace_path)
            .map_err(|_| "Cannot open parent-owned MCP trace")?;
        Ok(Self {
            config,
            file,
            calls: 0,
        })
    }
    pub fn record(&mut self, full_response: Value) -> Result<Value, String> {
        let index = self.calls + 1;
        let record = audit_record(full_response, &self.config, index)?;
        let payload = project(&record)?;
        let mut bytes = serde_json::to_vec(&record).map_err(|_| "Cannot encode MCP audit trace")?;
        bytes.push(b'\n');
        self.file
            .write_all(&bytes)
            .and_then(|_| self.file.sync_data())
            .map_err(|_| "Cannot flush MCP audit trace")?;
        self.calls = index;
        Ok(payload)
    }
}

fn audit_record(
    full_response: Value,
    config: &TransportConfig,
    call_index: usize,
) -> Result<AuditRecord, String> {
    let bytes = serde_json::to_vec(&full_response).map_err(|_| "Invalid full MCP evidence")?;
    let mut record = AuditRecord {
        schema_version: 1,
        session_id: config.session_id.clone(),
        call_index,
        request_id: format!("{}-request-{call_index}", config.session_id),
        trace_id: format!("{}-trace-{call_index}", config.session_id),
        payload_mode: config.payload_mode,
        payload_bytes: 0,
        full_response_sha256: crate::manifest::hash(&bytes),
        full_response,
    };
    record.payload_bytes = serde_json::to_vec(&project(&record)?)
        .map_err(|_| "Invalid MCP payload")?
        .len();
    Ok(record)
}

fn project(record: &AuditRecord) -> Result<Value, String> {
    let full = &record.full_response;
    if record.payload_mode == PayloadMode::Full {
        return Ok(full.clone());
    }
    let hits: Vec<_> = full["hits"]
        .as_array()
        .ok_or("Missing full MCP hits")?
        .iter()
        .map(|hit| json!({"section":hit["section"]}))
        .collect();
    Ok(json!({
        "schema_version":2,"payload_mode":"compact",
        "question":full["question"],"top_k":full["top_k"],
        "sources":full["sources"],"as_of":full["as_of"],"manifest":full["manifest"],"hits":hits,
        "trace":{"session_id":record.session_id,"call_index":record.call_index,"request_id":record.request_id,
            "trace_id":record.trace_id,"full_response_sha256":record.full_response_sha256}
    }))
}

/// Pure projection for offline byte comparisons. It does not run retrieval or a model.
pub fn project_for_comparison(
    full: Value,
    mode: PayloadMode,
    session_id: &str,
    call_index: usize,
) -> Result<Value, String> {
    let config = TransportConfig {
        payload_mode: mode,
        session_id: session_id.into(),
        trace_path: std::env::temp_dir().join("unused-audit.jsonl"),
    };
    config.validate()?;
    project(&audit_record(full, &config, call_index)?)
}

fn read_records(config: &TransportConfig) -> Result<(Vec<AuditRecord>, TraceSummary), String> {
    config.validate()?;
    let metadata = std::fs::symlink_metadata(&config.trace_path)
        .map_err(|_| "output: Missing MCP audit trace")?;
    if !metadata.is_file() || metadata.len() > super::records::MAX_ARTIFACT_BYTES as u64 {
        return Err("output: Invalid MCP audit trace file".into());
    }
    let bytes =
        std::fs::read(&config.trace_path).map_err(|_| "output: Cannot read MCP audit trace")?;
    if !bytes.is_empty() && !bytes.ends_with(b"\n") {
        return Err("output: Truncated MCP audit trace".into());
    }
    let mut records = Vec::new();
    let mut payload_bytes = 0;
    for line in bytes
        .strip_suffix(b"\n")
        .unwrap_or(&bytes)
        .split(|b| *b == b'\n')
    {
        if bytes.is_empty() {
            break;
        }
        if line.is_empty() {
            return Err("output: Empty MCP audit record".into());
        }
        let record: AuditRecord =
            serde_json::from_slice(line).map_err(|_| "output: Invalid MCP audit trace schema")?;
        let expected = audit_record(record.full_response.clone(), config, records.len() + 1)?;
        if record.schema_version != 1
            || record.session_id != config.session_id
            || record.call_index != expected.call_index
            || record.request_id != expected.request_id
            || record.trace_id != expected.trace_id
            || record.payload_mode != config.payload_mode
            || record.full_response_sha256 != expected.full_response_sha256
            || record.payload_bytes != expected.payload_bytes
        {
            return Err("output: MCP audit identity, order or digest changed".into());
        }
        payload_bytes += record.payload_bytes;
        records.push(record);
    }
    let summary = TraceSummary {
        session_id: config.session_id.clone(),
        path: config.trace_path.clone(),
        sha256: crate::manifest::hash(&bytes),
        trace_bytes: bytes.len(),
        payload_bytes,
        successful_searches: records.len(),
    };
    Ok((records, summary))
}

pub fn summary(config: &TransportConfig) -> Result<TraceSummary, String> {
    read_records(config).map(|(_, summary)| summary)
}

/// Compare actual model-visible responses to the trace, then restore full evidence for existing guards.
pub fn bind_run(
    run: &CodexRun,
    config: &TransportConfig,
) -> Result<(CodexRun, TraceSummary), String> {
    let (records, summary) = read_records(config)?;
    let mut full_run = run.clone();
    let mut index = 0;
    for call in &mut full_run.tool_calls {
        if call.status != "completed" {
            continue;
        }
        let record = records.get(index).ok_or("output: Missing MCP audit call")?;
        if call.result.as_ref() != Some(&project(record)?)
            || record.full_response["question"] != call.query
            || record.full_response["top_k"] != call.top_k
        {
            return Err("output: Model-visible MCP evidence differs from its audit trace".into());
        }
        call.result = Some(record.full_response.clone());
        index += 1;
    }
    if index != records.len() {
        return Err("output: Unmatched MCP audit calls".into());
    }
    Ok((full_run, summary))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        codex::{GroundedAnswer, ToolCall},
        retrieval_lab::lab_mcp::{SearchSession, tests::settings},
    };
    fn fixture(mode: PayloadMode) -> (PathBuf, TransportConfig, CodexRun) {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "coa-trace-test-{}-{}.jsonl",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::write(&path, b"").unwrap();
        let config = TransportConfig {
            payload_mode: mode,
            session_id: "fixture-session".into(),
            trace_path: path.clone(),
        };
        let mut writer = TraceWriter::open(config.clone()).unwrap();
        let full = SearchSession::new(settings())
            .unwrap()
            .search("dinsdag binnen", 3)
            .unwrap();
        let response = writer.record(full.clone()).unwrap();
        let run = CodexRun {
            id: "test".into(),
            status: "completed".into(),
            question: "dinsdag binnen".into(),
            answer: GroundedAnswer {
                text: "Onbekend".into(),
                citations: vec![],
                unknowns: vec!["Beschikbaarheid onbekend".into()],
            },
            tool_calls: vec![ToolCall {
                server: "coa".into(),
                tool: "search_document".into(),
                query: "dinsdag binnen".into(),
                top_k: 3,
                status: "completed".into(),
                result: Some(response),
                error: None,
            }],
            sources: full["sources"].as_array().unwrap().clone(),
            as_of: crate::AS_OF.into(),
            manifest: full["manifest"].clone(),
            client_version: "fixture".into(),
            model: "fixture".into(),
            elapsed_ms: 1,
            usage: None,
        };
        (path, config, run)
    }
    #[test]
    fn full_and_compact_bind_exact_evidence_and_original_source_guards() {
        for mode in [PayloadMode::Full, PayloadMode::Compact] {
            let (path, config, run) = fixture(mode);
            let (full, summary) = bind_run(&run, &config).unwrap();
            assert!(summary.payload_bytes > 0);
            assert!(
                full.tool_calls[0].result.as_ref().unwrap()["outcome"]["candidates"].is_array()
            );
            let mut session = settings();
            session.schema_version = 2;
            session.transport = Some(config.clone());
            super::super::lab_mcp::validate_run(&run, &session).unwrap();
            let mut forged = run.clone();
            forged.tool_calls[0].result.as_mut().unwrap()["hits"][0]["section"]["text"] =
                json!("fabricated");
            assert!(super::super::lab_mcp::validate_run(&forged, &session).is_err());
            std::fs::remove_file(path).unwrap();
        }
    }
    #[test]
    fn binding_rejects_missing_duplicate_truncated_foreign_and_tampered_trace() {
        let (path, config, run) = fixture(PayloadMode::Compact);
        let original = std::fs::read(&path).unwrap();
        let mut value: Value = serde_json::from_slice(&original).unwrap();
        value["full_response"]["outcome"]["estimated_context_tokens"] = json!(0);
        let mut tampered = serde_json::to_vec(&value).unwrap();
        tampered.push(b'\n');
        let duplicate = [original.as_slice(), original.as_slice()].concat();
        let blank_line = [original.as_slice(), b"\n"].concat();
        value = serde_json::from_slice(&original).unwrap();
        value["session_id"] = json!("foreign");
        let mut foreign = serde_json::to_vec(&value).unwrap();
        foreign.push(b'\n');
        for bad in [
            vec![],
            duplicate,
            blank_line,
            original[..original.len() - 1].to_vec(),
            foreign,
            tampered,
        ] {
            std::fs::write(&path, bad).unwrap();
            assert!(bind_run(&run, &config).is_err());
        }
        std::fs::remove_file(path).unwrap();
        assert!(bind_run(&run, &config).is_err());
    }
    #[test]
    fn reordered_model_calls_and_audit_lines_are_rejected() {
        let (path, config, mut run) = fixture(PayloadMode::Compact);
        let full = SearchSession::new(settings())
            .unwrap()
            .search("fiets dinsdag", 3)
            .unwrap();
        let second = audit_record(full, &config, 2).unwrap();
        let original = std::fs::read(&path).unwrap();
        let mut second_line = serde_json::to_vec(&second).unwrap();
        second_line.push(b'\n');
        std::fs::write(
            &path,
            [original.as_slice(), second_line.as_slice()].concat(),
        )
        .unwrap();
        let mut call = run.tool_calls[0].clone();
        call.query = "fiets dinsdag".into();
        call.result = Some(project(&second).unwrap());
        run.tool_calls.push(call);
        bind_run(&run, &config).unwrap();
        run.tool_calls.swap(0, 1);
        assert!(bind_run(&run, &config).is_err());
        run.tool_calls.swap(0, 1);
        std::fs::write(
            &path,
            [second_line.as_slice(), original.as_slice()].concat(),
        )
        .unwrap();
        assert!(bind_run(&run, &config).is_err());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    #[ignore = "requires local immutable historical live artifacts and explicit output path"]
    fn replay_historical_model_payload_bytes_without_models() {
        let input =
            PathBuf::from(std::env::var("COA_MCP_REPLAY_INPUT").expect("Set COA_MCP_REPLAY_INPUT"));
        let output = PathBuf::from(
            std::env::var("COA_MCP_REPLAY_OUTPUT").expect("Set COA_MCP_REPLAY_OUTPUT"),
        );
        let mut paths: Vec<_> = std::fs::read_dir(input)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "jsonl"))
            .collect();
        paths.sort();
        let mut rows = Vec::new();
        let mut full_total = 0usize;
        let mut compact_total = 0usize;
        for path in paths {
            let text = std::fs::read_to_string(&path).unwrap();
            let record: super::super::live::LiveRecord =
                serde_json::from_str(text.trim_end()).unwrap();
            let run = record.run.as_ref().unwrap();
            let mut session = settings();
            session.config = record.config;
            session.filters = record.filters;
            super::super::lab_mcp::validate_run(run, &session).unwrap();
            for (index, call) in run.tool_calls.iter().enumerate() {
                let Some(full) = call.result.as_ref() else {
                    continue;
                };
                let compact = project_for_comparison(
                    full.clone(),
                    PayloadMode::Compact,
                    "lab-mcp-offline-replay-20261103",
                    index + 1,
                )
                .unwrap();
                let full_bytes = full.to_string().len();
                let compact_bytes = compact.to_string().len();
                let offered: Vec<_> = full["hits"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|hit| json!({"section":hit["section"]}))
                    .collect();
                assert_eq!(compact["hits"], json!(offered));
                assert_eq!(compact["manifest"], full["manifest"]);
                assert_eq!(compact["sources"], full["sources"]);
                assert_eq!(compact["as_of"], full["as_of"]);
                assert_eq!(compact["question"], call.query);
                assert_eq!(compact["top_k"], call.top_k);
                assert!(compact_bytes < full_bytes);
                full_total += full_bytes;
                compact_total += compact_bytes;
                rows.push(json!({"artifact":path.file_name().unwrap().to_string_lossy(),"call_index":index+1,"method":session.config.retrieval.method,"profile":session.config.profile,"full_payload_bytes":full_bytes,"compact_payload_bytes":compact_bytes,"bytes_saved":full_bytes-compact_bytes,"exact_offered_sections_preserved":true,"canonical_guard_passed":true}));
            }
        }
        assert_eq!(rows.len(), 13);
        let proof = json!({"schema_version":1,"experiment":"offline_payload_projection_v1","provider_calls":0,"ai_judge_calls":0,"quality_comparison":"not_measured","unit":"UTF-8 JSON text bytes inside successful MCP text blocks, excluding RPC envelopes","limitation":"Historical JSON replay measures payload size only. It does not measure provider tokens, answer quality, runtime latency or money. Full method/config/corpus manifests and source catalogue are deliberately retained.","full_payload_bytes":full_total,"compact_payload_bytes":compact_total,"bytes_saved":full_total-compact_total,"reduction_fraction":1.0-compact_total as f64/full_total as f64,"calls":rows});
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(output)
            .unwrap();
        file.write_all(&serde_json::to_vec_pretty(&proof).unwrap())
            .unwrap();
        file.sync_all().unwrap();
    }

    #[test]
    fn new_trace_rejects_existing_content_and_symlink() {
        let (path, config, _) = fixture(PayloadMode::Full);
        assert!(TraceWriter::open(config.clone()).is_err());
        #[cfg(unix)]
        {
            let link = path.with_extension("link");
            std::os::unix::fs::symlink(&path, &link).unwrap();
            assert!(
                TraceWriter::open(TransportConfig {
                    trace_path: link.clone(),
                    ..config
                })
                .is_err()
            );
            std::fs::remove_file(link).unwrap();
        }
        std::fs::remove_file(path).unwrap();
    }
}
