//! Explicitly bounded Codex generations using one fixed local retrieval treatment.
use super::{
    lab_mcp::{self, SessionConfig},
    live_budget::LiveBudget,
    mcp_trace::{self, PayloadMode, TraceSummary, TransportConfig},
};
use crate::codex::{CodexRun, CodexRunner};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    io::{Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    time::Instant,
};

#[derive(Debug, Clone)]
pub struct LiveOptions {
    pub question: String,
    pub session: SessionConfig,
    pub output: PathBuf,
    pub ledger: PathBuf,
    pub max_generations: usize,
    pub max_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LiveRecord {
    pub schema_version: u32,
    pub execution_kind: String,
    pub run_id: String,
    pub question: String,
    pub config_id: String,
    pub config: super::types::RunConfig,
    pub filters: crate::retrieval::QueryFilters,
    pub status: String,
    pub budget_id: String,
    pub generation_session_index: usize,
    pub ai_judge_calls: usize,
    pub answer_quality_status: String,
    pub source_guard_passed: Option<bool>,
    pub runtime_configuration: Option<Value>,
    pub run: Option<CodexRun>,
    pub error: Option<String>,
    pub elapsed_ms: u64,
    #[serde(default)]
    pub payload_mode: PayloadMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transport_treatment_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audit_trace: Option<TraceSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_integrity_status: Option<String>,
}

/// Execute a bounded session only after its actual runtime matches the comparison.
pub async fn ask_pinned(
    options: LiveOptions,
    model: &str,
    expected: Option<&Value>,
) -> Result<LiveRecord, String> {
    ask_with_runtime_pin(
        options,
        CodexRunner::new_selected,
        Some((model.into(), expected.cloned())),
    )
    .await
}

pub async fn cli(args: &[String]) -> Result<(), String> {
    if args.first().map(String::as_str) == Some("mcp") {
        if args.len() != 3 || args[1] != "--session" || args[2].len() > 16_384 {
            return Err("Usage: lab mcp --session <fixed session JSON>".into());
        }
        let settings: SessionConfig =
            serde_json::from_str(&args[2]).map_err(|_| "Invalid selected MCP session settings")?;
        return lab_mcp::serve(settings, std::io::stdin().lock(), std::io::stdout().lock());
    }
    if args.first().map(String::as_str) != Some("ask") {
        return Err("Usage: lab ask --live ... | lab mcp --session <JSON>".into());
    }
    if args.iter().filter(|arg| arg.as_str() == "--live").count() != 1 {
        return Err("Selected Codex generation requires explicit --live".into());
    }
    let values: Vec<_> = args[1..]
        .iter()
        .filter(|arg| arg.as_str() != "--live")
        .cloned()
        .collect();
    if !values.len().is_multiple_of(2) {
        return Err("Every lab ask option requires a value".into());
    }
    let own = [
        "--question",
        "--output",
        "--ledger",
        "--max-generations",
        "--max-seconds",
        "--model-cache",
        "--sqlite-dir",
        "--query-filters",
        "--payload-mode",
    ];
    let common = [
        "--methods",
        "--profiles",
        "--top-k",
        "--candidate-k",
        "--context-limit",
        "--selection",
        "--apply-filters",
        "--mmr",
        "--rrf-k",
        "--vector-weight",
        "--mmr-lambda",
    ];
    let mut flags = BTreeMap::new();
    let mut plan_args = vec!["plan".to_string()];
    for pair in values.chunks_exact(2) {
        if pair[1].is_empty()
            || (!own.contains(&pair[0].as_str()) && !common.contains(&pair[0].as_str()))
            || flags.insert(pair[0].clone(), pair[1].clone()).is_some()
        {
            return Err("Unknown, empty or duplicate lab ask option".into());
        }
        if common.contains(&pair[0].as_str()) {
            plan_args.extend_from_slice(pair);
        }
    }
    let get = |key: &str| {
        flags
            .get(key)
            .map(String::as_str)
            .ok_or_else(|| format!("Missing {key}"))
    };
    let plan: super::types::ExperimentPlan = serde_json::from_value(super::cli::cli(&plan_args)?)
        .map_err(|_| "Invalid selected retrieval plan")?;
    if plan.configurations.len() != 1 {
        return Err("lab ask requires exactly one --methods and one --profiles treatment".into());
    }
    let output = absolute(Path::new(get("--output")?))?;
    let filters = flags
        .get("--query-filters")
        .map(|value| serde_json::from_str(value).map_err(|_| "Invalid typed query filters"))
        .transpose()?
        .unwrap_or_default();
    let payload_mode = match flags
        .get("--payload-mode")
        .map(String::as_str)
        .unwrap_or("compact")
    {
        "full" => PayloadMode::Full,
        "compact" => PayloadMode::Compact,
        _ => return Err("--payload-mode must be full or compact".into()),
    };
    let transport = TransportConfig {
        payload_mode,
        session_id: format!(
            "lab-mcp-{}-{}",
            crate::experiment::budget::now_ms()?,
            std::process::id()
        ),
        trace_path: output.with_extension("mcp-trace.jsonl"),
    };
    let options = LiveOptions {
        question: crate::validate_query(get("--question")?)?,
        session: SessionConfig {
            schema_version: 2,
            transport: Some(transport),
            config: plan.configurations[0].config.clone(),
            filters,
            model_cache: absolute(Path::new(
                flags
                    .get("--model-cache")
                    .map(String::as_str)
                    .unwrap_or(".model-cache"),
            ))?,
            sqlite_dir: absolute(
                &flags
                    .get("--sqlite-dir")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| output.parent().unwrap().join("retrieval-cache")),
            )?,
        },
        output,
        ledger: absolute(Path::new(get("--ledger")?))?,
        max_generations: get("--max-generations")?
            .parse()
            .map_err(|_| "Invalid --max-generations")?,
        max_seconds: get("--max-seconds")?
            .parse()
            .map_err(|_| "Invalid --max-seconds")?,
    };
    let record = ask_with_runtime(options, CodexRunner::new_selected).await?;
    println!(
        "{}",
        serde_json::json!({"schema_version":1,"run_id":record.run_id,"status":record.status,"config_id":record.config_id,"generation_session_index":record.generation_session_index,"ai_judge_calls":0,"source_guard_passed":record.source_guard_passed,"answer_quality_status":"not_assessed"})
    );
    if record.status != "completed" {
        return Err(
            "Selected Codex execution failed; inspect the saved result and budget ledger".into(),
        );
    }
    Ok(())
}

fn absolute(path: &Path) -> Result<PathBuf, String> {
    if path.is_absolute() {
        Ok(path.into())
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .map_err(|_| "Cannot resolve selected retrieval path".into())
    }
}

fn persist(file: &mut File, record: &LiveRecord) -> Result<(), String> {
    let mut bytes = serde_json::to_vec(record).map_err(|_| "Cannot encode selected live result")?;
    bytes.push(b'\n');
    file.seek(SeekFrom::Start(0))
        .and_then(|_| file.set_len(0))
        .and_then(|_| file.write_all(&bytes))
        .and_then(|_| file.sync_all())
        .map_err(|_| "Cannot persist selected live result".into())
}

async fn ask_with_runtime<F, R>(options: LiveOptions, runtime: F) -> Result<LiveRecord, String>
where
    F: FnOnce(std::time::Duration) -> R,
    R: std::future::Future<Output = Result<CodexRunner, String>>,
{
    ask_with_runtime_pin(options, runtime, None).await
}

async fn ask_with_runtime_pin<F, R>(
    options: LiveOptions,
    runtime: F,
    pin: Option<(String, Option<Value>)>,
) -> Result<LiveRecord, String>
where
    F: FnOnce(std::time::Duration) -> R,
    R: std::future::Future<Output = Result<CodexRunner, String>>,
{
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let started = Instant::now();
    options.session.validate()?;
    let question = crate::validate_query(&options.question)?;
    if options.output == options.ledger
        || options.output.exists()
        || !options
            .output
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .is_dir()
    {
        return Err(
            "Choose a new live output file in an existing directory, separate from its ledger"
                .into(),
        );
    }
    if let Some(transport) = &options.session.transport
        && (transport.trace_path == options.output
            || transport.trace_path == options.ledger
            || transport.trace_path.exists()
            || transport.trace_path.parent() != options.output.parent())
    {
        return Err(
            "Choose a new parent-owned trace beside the live output, separate from its ledger"
                .into(),
        );
    }
    let config_id = super::catalog::config_id(&options.session.config)?;
    let run_id = format!(
        "lab-live-{}-{}-{}",
        crate::experiment::budget::now_ms()?,
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    let mut budget = LiveBudget::open(
        &options.ledger,
        options.max_generations,
        options.max_seconds,
    )?;
    let mut output_options = OpenOptions::new();
    output_options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        output_options.mode(0o600);
    }
    let mut output = output_options
        .open(&options.output)
        .map_err(|_| "Cannot create new selected live result")?;
    if let Some(transport) = &options.session.transport
        && output_options.open(&transport.trace_path).is_err()
    {
        drop(output);
        let _ = std::fs::remove_file(&options.output);
        return Err("Cannot create new parent-owned MCP audit trace".into());
    }
    let timeout = match budget.reserve(&run_id, &config_id) {
        Ok(timeout) => timeout,
        Err(error) => {
            drop(output);
            let _ = std::fs::remove_file(&options.output);
            if let Some(transport) = &options.session.transport {
                let _ = std::fs::remove_file(&transport.trace_path);
            }
            return Err(error);
        }
    };
    let mut record = LiveRecord {
        schema_version: if options.session.transport.is_some() { 2 } else { 1 },
        execution_kind: if options.session.transport.is_some() { "codex_selected_mcp_v2" } else { "codex_selected_mcp_v1" }.into(),
        run_id,
        question: question.clone(),
        config_id: config_id.clone(),
        config: options.session.config.clone(),
        filters: options.session.filters.clone(),
        status: "started".into(),
        budget_id: budget.id().into(),
        generation_session_index: budget.used(),
        ai_judge_calls: 0,
        answer_quality_status: "not_assessed".into(),
        source_guard_passed: None,
        runtime_configuration: None,
        run: None,
        error: None,
        elapsed_ms: 0,
        payload_mode: options.session.transport.as_ref().map(|t| t.payload_mode).unwrap_or_default(),
        transport_treatment_id: options.session.transport.as_ref().map(|t| {
            serde_json::to_vec(&serde_json::json!({"schema_version":1,"config_id":config_id,"filters":options.session.filters,"payload_mode":t.payload_mode}))
                .map(|bytes| crate::manifest::hash(&bytes)).map_err(|_| "Cannot encode MCP transport treatment")
        }).transpose()?,
        audit_trace: options.session.transport.as_ref().map(mcp_trace::summary).transpose()?,
        trace_integrity_status: options.session.transport.as_ref().map(|_| "not_checked".into()),
    };
    persist(&mut output, &record)?;
    let execution = tokio::time::timeout(timeout, async {
        let runner = runtime(budget.remaining()?).await?;
        record.runtime_configuration = Some(runner.configuration());
        if let Some((model, expected)) = &pin
            && (record.runtime_configuration.as_ref().unwrap()["model"] != *model
                || expected.as_ref().is_some_and(|configuration| {
                    record.runtime_configuration.as_ref() != Some(configuration)
                }))
        {
            return Err(
                "runtime: Agentic comparison runtime/model changed before generation".into(),
            );
        }
        let launch = crate::codex::SelectedMcpLaunch {
            arguments: vec![
                "lab".into(),
                "mcp".into(),
                "--session".into(),
                serde_json::to_string(&options.session)
                    .map_err(|_| "Cannot encode fixed retrieval session")?,
            ],
            instructions: format!(
                "Fixed retrieval experiment {config_id}. The selected method, profile and typed filters are fixed for this session. Every search_document call MUST use top_k={}. Before searching, derive a checklist only from the user's question (for example day, setting, time and location). Check the retrieved sources against every part of that checklist. If information is missing, search again with a different targeted query, up to four allowed search attempts. Stop when the question is supported, or when the available sources cannot establish the missing information; clearly state remaining unknowns. Never infer current places from a schedule. Scores are method-specific ranking signals, never confidence. Retrieved evidence includes the actual execution configuration and source dates. Do not claim these hard checks judge answer correctness.",
                options.session.config.retrieval.top_k
            ),
        };
        let run = runner
            .run_with_selected_retrieval(
                &question,
                options.session.config.retrieval.top_k,
                &launch,
                budget.remaining()?,
            )
            .await?;
        let validated = lab_mcp::validate_run(&run, &options.session);
        record.source_guard_passed = Some(validated.is_ok());
        record.run = Some(run);
        validated
    });
    let result = tokio::select! {
        result = execution => result.unwrap_or_else(|_| Err("runtime: Selected live execution timed out".into())),
        cancelled = tokio::signal::ctrl_c() => match cancelled {
            Ok(()) => Err("cancelled".into()),
            Err(_) => Err("runtime: Cannot observe live cancellation".into()),
        },
    };
    match result {
        Ok(()) => {
            record.status = "completed".into();
        }
        Err(error) => {
            record.status = "failed".into();
            record.error = Some(crate::codex::public_error_code(&error).into());
        }
    }
    if let Some(transport) = &options.session.transport {
        match mcp_trace::summary(transport) {
            Ok(summary) => {
                record.audit_trace = Some(summary);
                record.trace_integrity_status = Some(
                    match record.source_guard_passed {
                        Some(true) => "validated",
                        Some(false) => "invalid",
                        None => "partial_not_validated",
                    }
                    .into(),
                );
            }
            Err(_) => {
                record.audit_trace = None;
                record.status = "failed".into();
                record.source_guard_passed = Some(false);
                record.trace_integrity_status = Some("invalid".into());
                record.error = Some("output".into());
            }
        }
    }
    record.elapsed_ms = started.elapsed().as_millis() as u64;
    persist(&mut output, &record)?;
    Ok(record)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        codex::{Citation, GroundedAnswer},
        retrieval::{RetrievalConfig, SearchMethod},
        retrieval_lab::lab_mcp::SearchSession,
        storage::StorageProfile,
    };
    fn temporary() -> PathBuf {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        std::env::temp_dir().join(format!(
            "coa-selected-live-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ))
    }
    fn options(base: &std::path::Path) -> LiveOptions {
        LiveOptions {
            question: "dinsdag binnen".into(),
            session: SessionConfig {
                schema_version: 1,
                transport: None,
                config: super::super::types::RunConfig {
                    profile: StorageProfile::DocumentsActivities,
                    retrieval: RetrievalConfig {
                        method: SearchMethod::Bm25,
                        ..Default::default()
                    },
                },
                filters: Default::default(),
                model_cache: base.join("NO_MODEL"),
                sqlite_dir: base.join("NO_SQLITE"),
            },
            output: base.join("answer.jsonl"),
            ledger: base.join("budget.json"),
            max_generations: 2,
            max_seconds: 1200,
        }
    }
    fn fake_events(session: &SessionConfig) -> String {
        let response = SearchSession::new(session.clone())
            .unwrap()
            .search("dinsdag binnen", 3)
            .unwrap();
        let section = &response["hits"][0]["section"];
        let answer = GroundedAnswer {
            text: "Praktische informatie uit de gevonden bron.".into(),
            citations: vec![Citation {
                source_id: section["source_id"].as_str().unwrap().into(),
                source_date: section["source_date"].as_str().unwrap().into(),
                section_id: section["id"].as_str().unwrap().into(),
                quote: section["text"].as_str().unwrap().into(),
            }],
            unknowns: vec![],
        };
        [serde_json::json!({"type":"item.completed","item":{"type":"mcp_tool_call","server":"coa","tool":"search_document","arguments":{"query":"dinsdag binnen","top_k":3},"status":"completed","result":{"content":[{"type":"text","text":response.to_string()}]}}}),serde_json::json!({"type":"item.completed","item":{"type":"agent_message","text":serde_json::to_string(&answer).unwrap()}}),serde_json::json!({"type":"turn.completed"})].iter().map(Value::to_string).collect::<Vec<_>>().join("\n")
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_changed_model_is_rejected_before_a_generation_starts() {
        let base = temporary();
        std::fs::create_dir(&base).unwrap();
        let options = options(&base);
        let events = fake_events(&options.session);
        let marker = base.join("generation-started");
        let script = format!(
            "#!/bin/sh\ncat >/dev/null\ntouch '{}'\ncat <<'COA_EVENTS'\n{events}\nCOA_EVENTS\n",
            marker.display()
        );
        let runner = crate::codex::fixture_runner(&base.join("runtime"), &script);
        let record = ask_with_runtime_pin(
            options,
            |_| async { Ok(runner) },
            Some(("different-model".into(), None)),
        )
        .await
        .unwrap();
        assert_eq!(
            record.status, "failed",
            "runtime model mismatch must fail before generation"
        );
        assert!(
            !marker.exists(),
            "the changed runtime must never receive the question"
        );
        assert_eq!(record.generation_session_index, 1);
        std::fs::remove_dir_all(base).unwrap();
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn selected_generation_preserves_actual_tool_outcomes_and_rejects_overwrite_before_runtime()
     {
        let base = temporary();
        std::fs::create_dir(&base).unwrap();
        let options = options(&base);
        let events = fake_events(&options.session);
        let script =
            format!("#!/bin/sh\ncat >/dev/null\ncat <<'COA_EVENTS'\n{events}\nCOA_EVENTS\n");
        let runner = crate::codex::fixture_runner(&base.join("runtime"), &script);
        let record = ask_with_runtime(options.clone(), |_| async { Ok(runner) })
            .await
            .unwrap();
        assert_eq!(record.status, "completed");
        assert_eq!(record.source_guard_passed, Some(true));
        assert_eq!(record.ai_judge_calls, 0);
        assert_eq!(record.answer_quality_status, "not_assessed");
        assert_eq!(record.run.as_ref().unwrap().tool_calls.len(), 1);
        assert_eq!(record.run.as_ref().unwrap().manifest["score_kind"], "bm25");
        assert!(record.run.as_ref().unwrap().usage.is_none());
        assert!(!options.session.model_cache.exists());
        let saved = std::fs::read(&options.output).unwrap();
        assert!(
            ask_with_runtime(options.clone(), |_| async {
                panic!("Existing output must fail before runtime")
            })
            .await
            .is_err()
        );
        assert_eq!(std::fs::read(&options.output).unwrap(), saved);
        let budget: Value =
            serde_json::from_slice(&std::fs::read(&options.ledger).unwrap()).unwrap();
        assert_eq!(budget["attempts"].as_array().unwrap().len(), 1);
        std::fs::remove_dir_all(base).unwrap();
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn compact_and_full_live_sessions_bind_real_local_trace_without_provider_calls() {
        for mode in [PayloadMode::Full, PayloadMode::Compact] {
            let base = temporary();
            std::fs::create_dir(&base).unwrap();
            let mut options = options(&base);
            options.session.schema_version = 2;
            options.session.transport = Some(TransportConfig {
                payload_mode: mode,
                session_id: "fixed-live-fixture".into(),
                trace_path: base.join("answer.mcp-trace.jsonl"),
            });
            let session = options.session.clone();
            let runtime_dir = base.join("runtime");
            let record = ask_with_runtime(options.clone(), |_| {
                let events = fake_events(&session);
                let script = format!(
                    "#!/bin/sh\ncat >/dev/null\ncat <<'COA_EVENTS'\n{events}\nCOA_EVENTS\n"
                );
                let runner = crate::codex::fixture_runner(&runtime_dir, &script);
                async { Ok(runner) }
            })
            .await
            .unwrap();
            assert_eq!(record.schema_version, 2);
            assert_eq!(record.status, "completed");
            assert_eq!(record.payload_mode, mode);
            assert_eq!(record.source_guard_passed, Some(true));
            assert_eq!(record.trace_integrity_status.as_deref(), Some("validated"));
            let audit = record.audit_trace.as_ref().unwrap();
            assert_eq!(audit.successful_searches, 1);
            assert_eq!(audit.trace_bytes, std::fs::read(&audit.path).unwrap().len());
            assert!(audit.payload_bytes > 0);
            let visible = record.run.as_ref().unwrap().tool_calls[0]
                .result
                .as_ref()
                .unwrap();
            assert_eq!(visible.get("outcome").is_some(), mode == PayloadMode::Full);
            assert_eq!(audit.payload_bytes, visible.to_string().len());
            assert_eq!(record.ai_judge_calls, 0);
            assert!(!options.session.model_cache.exists());
            std::fs::remove_dir_all(base).unwrap();
        }
    }
    #[test]
    fn historical_live_schema_one_loads_as_full_without_fabricated_audit() {
        let record = serde_json::json!({"schema_version":1,"execution_kind":"codex_selected_mcp_v1","run_id":"historical","question":"Wanneer?","config_id":"legacy","config":options(std::path::Path::new("/tmp")).session.config,"filters":{},"status":"failed","budget_id":"legacy","generation_session_index":1,"ai_judge_calls":0,"answer_quality_status":"not_assessed","source_guard_passed":null,"runtime_configuration":null,"run":null,"error":"timeout","elapsed_ms":1});
        let loaded: LiveRecord = serde_json::from_value(record).unwrap();
        assert_eq!(loaded.payload_mode, PayloadMode::Full);
        assert!(loaded.audit_trace.is_none());
        assert!(loaded.transport_treatment_id.is_none());
    }
    #[tokio::test]
    async fn failed_runtime_is_saved_and_reserved_without_inventing_an_answer_or_usage() {
        let base = temporary();
        std::fs::create_dir(&base).unwrap();
        let options = options(&base);
        let record = ask_with_runtime(options.clone(), |_| async {
            Err("runtime: FAKE-PRIVATE-TOKEN /Users/PRIVATE".into())
        })
        .await
        .unwrap();
        assert_eq!(record.status, "failed");
        assert!(record.run.is_none());
        assert_eq!(record.source_guard_passed, None);
        assert_eq!(record.generation_session_index, 1);
        assert_eq!(record.ai_judge_calls, 0);
        let saved = String::from_utf8(std::fs::read(&options.output).unwrap()).unwrap();
        assert!(!saved.contains("FAKE-PRIVATE-TOKEN"));
        assert!(!saved.contains("/Users/PRIVATE"));
        std::fs::remove_dir_all(base).unwrap();
    }
    #[tokio::test]
    async fn explicit_live_opt_in_and_single_treatment_are_required_before_any_side_effect() {
        assert!(cli(&["ask".into()]).await.unwrap_err().contains("--live"));
        assert!(
            cli(&[
                "ask".into(),
                "--live".into(),
                "--methods".into(),
                "bm25,dense".into()
            ])
            .await
            .is_err()
        );
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn runtime_preparation_cannot_extend_the_global_deadline() {
        let base = temporary();
        std::fs::create_dir(&base).unwrap();
        let mut options = options(&base);
        options.max_seconds = 1;
        let events = fake_events(&options.session);
        let marker = base.join("GENERATION_STARTED");
        let script = format!(
            "#!/bin/sh\nprintf 'started' > '{}'\ncat >/dev/null\ncat <<'COA_EVENTS'\n{events}\nCOA_EVENTS\n",
            marker.display()
        );
        let runner = crate::codex::fixture_runner(&base.join("runtime"), &script);
        let record = ask_with_runtime(options, |_| async {
            tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
            Ok(runner)
        })
        .await
        .unwrap();
        assert_eq!(record.status, "failed");
        assert!(
            !marker.exists(),
            "No generation starts after the global deadline"
        );
        std::fs::remove_dir_all(base).unwrap();
    }
}
