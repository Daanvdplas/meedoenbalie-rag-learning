//! Input-only plan and bounded batch execution for actual multi-search sessions.
use super::{
    catalog,
    dataset::{self, DatasetIdentity},
    live::LiveRecord,
    types::RunConfig,
};
use crate::{
    retrieval::{RetrievalConfig, SearchMethod},
    storage::StorageProfile,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

const SELECTION: &str = include_str!("../../fixtures/experiments/presentation-selection.json");
pub const EXECUTION_KIND: &str = "agentic_search_comparison";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SelectedCase {
    dataset_id: String,
    case_id: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Selection {
    schema_version: u32,
    selection_id: String,
    cases: Vec<SelectedCase>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlannedRun {
    pub run_key: String,
    pub dataset_id: String,
    pub dataset: DatasetIdentity,
    pub query_inputs_sha256: Option<String>,
    pub case_id: String,
    pub question: String,
    pub config_id: String,
    pub config: RunConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentPlan {
    pub schema_version: u32,
    pub selection_id: String,
    pub selection_sha256: String,
    pub protocol_id: String,
    pub requested_model: String,
    pub case_count: usize,
    pub planned_sessions: usize,
    pub max_search_calls_per_session: usize,
    pub max_session_seconds: u64,
    pub ordering: String,
    pub runs: Vec<PlannedRun>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentRunRecord {
    pub schema_version: u32,
    pub execution_kind: String,
    pub batch_id: String,
    pub dataset_id: String,
    pub dataset: DatasetIdentity,
    pub query_inputs_sha256: Option<String>,
    pub case_id: String,
    pub question: String,
    pub selection_sha256: String,
    pub protocol_id: String,
    pub requested_model: String,
    pub live_output_path: PathBuf,
    pub live_output_sha256: String,
    pub live: LiveRecord,
}

#[derive(Debug, Clone)]
pub struct StoredAgentRecord {
    pub record: AgentRunRecord,
    pub record_sha256: String,
}

#[derive(Debug, Clone)]
pub struct AgentRunOptions {
    pub plan: AgentPlan,
    pub output: PathBuf,
    pub ledger: PathBuf,
    pub model_cache: PathBuf,
    pub max_generations: usize,
    pub max_seconds: u64,
}

#[derive(Debug, Serialize)]
pub struct AgentRunSummary {
    pub schema_version: u32,
    pub batch_id: String,
    pub protocol_id: String,
    pub planned_sessions: usize,
    pub saved_sessions: usize,
    pub completed: usize,
    pub failed: usize,
    pub stopped_after_failure: bool,
    pub ai_judge_calls: usize,
    pub output_sha256: String,
}

pub async fn run(options: AgentRunOptions) -> Result<AgentRunSummary, String> {
    run_with_executor(options, |options, model, expected| async move {
        super::live::ask_pinned(options, &model, expected.as_ref()).await
    })
    .await
}

async fn run_with_executor<F, Fut>(
    _options: AgentRunOptions,
    _execute: F,
) -> Result<AgentRunSummary, String>
where
    F: FnMut(super::live::LiveOptions, String, Option<serde_json::Value>) -> Fut,
    Fut: std::future::Future<Output = Result<LiveRecord, String>>,
{
    Err("Agentic batch execution is not implemented yet".into())
}

pub fn fixed_config(method: SearchMethod) -> RunConfig {
    RunConfig {
        profile: StorageProfile::DocumentsSections,
        retrieval: RetrievalConfig {
            method,
            ..Default::default()
        },
    }
}

pub fn selection_sha256() -> String {
    crate::manifest::hash(SELECTION.as_bytes())
}

pub fn protocol_id(model: &str) -> Result<String, String> {
    let datasets: Vec<_> = ["legacy", "independent-20261103"]
        .into_iter()
        .map(dataset::load)
        .collect::<Result<_, _>>()?;
    let bytes = serde_json::to_vec(&serde_json::json!({
        "schema_version":1, "selection_sha256":selection_sha256(), "requested_model":model,
        "configurations":catalog::METHODS.map(fixed_config),
        "datasets":datasets.iter().map(|value| &value.catalog.identity).collect::<Vec<_>>(),
        "max_search_calls":crate::mcp::MAX_SEARCH_CALLS, "session_seconds":120,
        "payload_mode":"compact", "search_instruction_version":"question-checklist-v1"
    }))
    .map_err(|_| "Cannot encode agentic comparison protocol")?;
    Ok(crate::manifest::hash(&bytes))
}

pub fn plan(
    case_ids: &[String],
    methods: &[SearchMethod],
    model: &str,
) -> Result<AgentPlan, String> {
    let selection: Selection =
        serde_json::from_str(SELECTION).map_err(|_| "Invalid input-only presentation selection")?;
    if selection.schema_version != 1
        || selection.cases.len() != 10
        || methods.is_empty()
        || methods
            .iter()
            .enumerate()
            .any(|(i, method)| methods[..i].contains(method))
        || case_ids.iter().enumerate().any(|(i, id)| {
            case_ids[..i].contains(id) || !selection.cases.iter().any(|case| &case.case_id == id)
        })
        || model.is_empty()
        || model.len() > 100
    {
        return Err("Invalid agentic question/method/model selection".into());
    }
    let mut runs = Vec::new();
    let mut cases = BTreeSet::new();
    for (index, selected) in selection.cases.iter().enumerate() {
        let dataset = dataset::load(&selected.dataset_id)?;
        let case = dataset
            .catalog
            .cases
            .iter()
            .find(|case| case.id == selected.case_id)
            .ok_or("Presentation selection has unknown input-only question")?;
        if !case_ids.is_empty() && !case_ids.contains(&case.id) {
            continue;
        }
        cases.insert(case.id.clone());
        for offset in 0..catalog::METHODS.len() {
            let method = catalog::METHODS[(index + offset) % catalog::METHODS.len()];
            if !methods.contains(&method) {
                continue;
            }
            let config = fixed_config(method);
            let config_id = catalog::config_id(&config)?;
            runs.push(PlannedRun {
                run_key: format!(
                    "{}__{}__{}",
                    selected.dataset_id,
                    case.id,
                    serde_json::to_value(method)
                        .map_err(|e| e.to_string())?
                        .as_str()
                        .unwrap()
                ),
                dataset_id: selected.dataset_id.clone(),
                dataset: dataset.catalog.identity.clone(),
                query_inputs_sha256: dataset.query_inputs_sha256.clone(),
                case_id: case.id.clone(),
                question: case.question.clone(),
                config_id,
                config,
            });
        }
    }
    Ok(AgentPlan {
        schema_version: 1,
        selection_id: selection.selection_id,
        selection_sha256: selection_sha256(),
        protocol_id: protocol_id(model)?,
        requested_model: model.into(),
        case_count: cases.len(),
        planned_sessions: runs.len(),
        max_search_calls_per_session: crate::mcp::MAX_SEARCH_CALLS,
        max_session_seconds: 120,
        ordering:
            "presentation case order; rotate the fixed four-method order by the original case index"
                .into(),
        runs,
    })
}

fn flags(args: &[String], allowed: &[&str]) -> Result<BTreeMap<String, String>, String> {
    if !args.len().is_multiple_of(2) {
        return Err("Each agentic option requires a value".into());
    }
    let mut result = BTreeMap::new();
    for pair in args.chunks_exact(2) {
        if !allowed.contains(&pair[0].as_str())
            || pair[1].is_empty()
            || result.insert(pair[0].clone(), pair[1].clone()).is_some()
        {
            return Err("Unknown, empty or duplicate agentic option".into());
        }
    }
    Ok(result)
}

fn selected_plan(flags: &BTreeMap<String, String>) -> Result<AgentPlan, String> {
    let cases = flags
        .get("--cases")
        .map(|text| {
            text.split(',')
                .map(|value| value.trim().to_owned())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let methods = flags
        .get("--methods")
        .map(|text| {
            text.split(',')
                .map(|value| {
                    serde_json::from_value(serde_json::Value::String(value.trim().into()))
                        .map_err(|_| "Unknown agentic method")
                })
                .collect::<Result<Vec<SearchMethod>, _>>()
        })
        .transpose()?
        .unwrap_or_else(|| catalog::METHODS.to_vec());
    plan(&cases, &methods, &crate::codex::configured_model()?)
}

pub fn plan_cli(args: &[String]) -> Result<serde_json::Value, String> {
    serde_json::to_value(selected_plan(&flags(args, &["--cases", "--methods"])?)?)
        .map_err(|e| e.to_string())
}

pub fn validate_record_identity(record: &AgentRunRecord) -> Result<(), String> {
    let selected = plan(
        &[record.case_id.clone()],
        &[record.live.config.retrieval.method],
        &record.requested_model,
    )?;
    let expected = selected
        .runs
        .iter()
        .find(|run| run.dataset_id == record.dataset_id && run.case_id == record.case_id)
        .ok_or("Agentic run has a different selected dataset or case")?;
    let live = &record.live;
    if record.schema_version != 1
        || record.execution_kind != EXECUTION_KIND
        || record.batch_id.is_empty()
        || record.batch_id.len() > 160
        || record.selection_sha256 != selected.selection_sha256
        || record.protocol_id != selected.protocol_id
        || record.dataset != expected.dataset
        || record.query_inputs_sha256 != expected.query_inputs_sha256
        || record.question != expected.question
        || live.question != record.question
        || live.config != expected.config
        || live.config_id != expected.config_id
        || live.schema_version != 2
        || live.execution_kind != "codex_selected_mcp_v2"
        || live.payload_mode != super::mcp_trace::PayloadMode::Compact
        || live.filters != Default::default()
        || live.run_id.is_empty()
        || live.budget_id.is_empty()
        || !(1..=40).contains(&live.generation_session_index)
        || live.ai_judge_calls != 0
        || live.answer_quality_status != "not_assessed"
        || !matches!(live.status.as_str(), "completed" | "failed")
        || !record.live_output_path.is_absolute()
        || record.live_output_sha256.len() != 64
    {
        return Err("Agentic run identity, treatment or protected input binding changed".into());
    }
    let startup_failure = live.status == "failed"
        && live.run.is_none()
        && live.source_guard_passed.is_none()
        && live.error.as_deref() == Some("runtime")
        && live
            .audit_trace
            .as_ref()
            .is_some_and(|trace| trace.successful_searches == 0 && trace.trace_bytes == 0);
    if let Some(configuration) = &live.runtime_configuration
        && configuration["model"] != record.requested_model
        && !startup_failure
    {
        return Err("Agentic runtime model differs from the fixed comparison".into());
    }
    if let Some(run) = &live.run
        && (run.question != record.question
            || run.model != record.requested_model
            || live
                .runtime_configuration
                .as_ref()
                .is_none_or(|configuration| configuration["client_version"] != run.client_version))
    {
        return Err("Agentic runtime completion differs from the submitted input or model".into());
    }
    if (live.status == "completed"
        && (live.error.is_some()
            || live.run.is_none()
            || live.runtime_configuration.is_none()
            || live.source_guard_passed != Some(true)
            || live.trace_integrity_status.as_deref() != Some("validated")))
        || (live.status == "failed" && live.error.as_deref().is_none_or(str::is_empty))
    {
        return Err("Agentic completion/failure status is inconsistent".into());
    }
    let expected_transport_id=crate::manifest::hash(&serde_json::to_vec(&serde_json::json!({"schema_version":1,"config_id":live.config_id,"filters":live.filters,"payload_mode":live.payload_mode})).map_err(|_|"Cannot encode fixed transport identity")?);
    if live.transport_treatment_id.as_deref() != Some(expected_transport_id.as_str()) {
        return Err("Agentic MCP treatment identity changed".into());
    }
    if let Some(trace) = &live.audit_trace {
        if trace.path != record.live_output_path.with_extension("mcp-trace.jsonl") {
            return Err("Agentic audit trace does not belong to its saved live result".into());
        }
    } else if live.status == "completed" {
        return Err("Completed agentic run has no audit trace".into());
    }
    let actual = read_bounded(&record.live_output_path)?;
    if crate::manifest::hash(&actual) != record.live_output_sha256 {
        return Err("Saved live result bytes changed".into());
    }
    let saved: LiveRecord = serde_json::from_slice(&actual)
        .map_err(|_| "Saved live result is not one valid live record")?;
    if serde_json::to_value(saved).map_err(|e| e.to_string())?
        != serde_json::to_value(live).map_err(|e| e.to_string())?
    {
        return Err("Agentic wrapper does not preserve the exact live result".into());
    }
    Ok(())
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, String> {
    let metadata = std::fs::metadata(path).map_err(|_| "Cannot read agentic result artifact")?;
    if !metadata.is_file() || metadata.len() > super::records::MAX_ARTIFACT_BYTES as u64 {
        return Err("Agentic result artifact exceeds its file/size boundary".into());
    }
    std::fs::read(path).map_err(|_| "Cannot read agentic result bytes".into())
}

pub fn read_records(path: &Path) -> Result<Vec<StoredAgentRecord>, String> {
    let bytes = read_bounded(path)?;
    let mut records = Vec::new();
    let mut ids = BTreeSet::new();
    let mut treatments = BTreeSet::new();
    let lines = bytes.split(|byte| *byte == b'\n').collect::<Vec<_>>();
    for (index, line) in lines.iter().enumerate() {
        if line.is_empty() && index + 1 == lines.len() {
            continue;
        }
        if line.is_empty() {
            return Err("Empty line inside agentic result artifact".into());
        }
        let record: AgentRunRecord = serde_json::from_slice(line)
            .map_err(|_| format!("Invalid agentic result at line {}", index + 1))?;
        validate_record_identity(&record)?;
        if !ids.insert(record.live.run_id.clone())
            || !treatments.insert((
                record.dataset_id.clone(),
                record.case_id.clone(),
                record.live.config_id.clone(),
            ))
        {
            return Err("Duplicate live session or question/treatment in agentic results".into());
        }
        if records.first().is_some_and(|first: &StoredAgentRecord| {
            first.record.protocol_id != record.protocol_id
                || first.record.batch_id != record.batch_id
        }) {
            return Err("Mixed agentic batch/protocol identities in one result file".into());
        }
        records.push(StoredAgentRecord {
            record,
            record_sha256: crate::manifest::hash(line),
        });
    }
    if records.is_empty() {
        return Err("Agentic result artifact is empty".into());
    }
    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::super::mcp_trace::{PayloadMode, TraceSummary};
    use super::*;

    fn directory() -> PathBuf {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "coa-agent-batch-test-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        path
    }

    fn failed_record(base: &Path) -> AgentRunRecord {
        let plan = plan(&["T02".into()], &[SearchMethod::Bm25], "test-model").unwrap();
        let selected = &plan.runs[0];
        let output = base.join("live.jsonl");
        let trace = output.with_extension("mcp-trace.jsonl");
        std::fs::write(&trace, b"").unwrap();
        let config_id = catalog::config_id(&selected.config).unwrap();
        let live=LiveRecord {
            schema_version:2,execution_kind:"codex_selected_mcp_v2".into(),run_id:"agent-fixture".into(),
            question:selected.question.clone(),config_id:config_id.clone(),config:selected.config.clone(),filters:Default::default(),status:"failed".into(),
            budget_id:"fixture-budget".into(),generation_session_index:1,ai_judge_calls:0,answer_quality_status:"not_assessed".into(),
            source_guard_passed:None,runtime_configuration:None,run:None,error:Some("runtime".into()),elapsed_ms:0,payload_mode:PayloadMode::Compact,
            transport_treatment_id:Some(crate::manifest::hash(&serde_json::to_vec(&serde_json::json!({"schema_version":1,"config_id":config_id,"filters":crate::retrieval::QueryFilters::default(),"payload_mode":"compact"})).unwrap())),
            audit_trace:Some(TraceSummary {session_id:"fixture-trace".into(),path:trace,sha256:crate::manifest::hash(b""),trace_bytes:0,payload_bytes:0,successful_searches:0}),
            trace_integrity_status:Some("partial_not_validated".into()),
        };
        let mut bytes = serde_json::to_vec(&live).unwrap();
        bytes.push(b'\n');
        std::fs::write(&output, &bytes).unwrap();
        AgentRunRecord {
            schema_version: 1,
            execution_kind: EXECUTION_KIND.into(),
            batch_id: "fixture-batch".into(),
            dataset_id: selected.dataset_id.clone(),
            dataset: selected.dataset.clone(),
            query_inputs_sha256: selected.query_inputs_sha256.clone(),
            case_id: selected.case_id.clone(),
            question: selected.question.clone(),
            selection_sha256: plan.selection_sha256,
            protocol_id: plan.protocol_id,
            requested_model: "test-model".into(),
            live_output_path: output,
            live_output_sha256: crate::manifest::hash(&bytes),
            live,
        }
    }

    fn options(base: &Path) -> AgentRunOptions {
        AgentRunOptions {
            plan: plan(
                &["T02".into(), "P06".into()],
                &[SearchMethod::Bm25],
                "test-model",
            )
            .unwrap(),
            output: base.join("batch.jsonl"),
            ledger: base.join("budget.json"),
            model_cache: base.join("NO_MODEL"),
            max_generations: 40,
            max_seconds: 4800,
        }
    }

    fn fake_failed_attempt(options: &super::super::live::LiveOptions) -> LiveRecord {
        let mut live = failed_record(options.output.parent().unwrap()).live;
        let mut budget = super::super::live_budget::LiveBudget::open(
            &options.ledger,
            options.max_generations,
            options.max_seconds,
        )
        .unwrap();
        budget
            .reserve(
                &live.run_id,
                &catalog::config_id(&options.session.config).unwrap(),
            )
            .unwrap();
        live.budget_id = budget.id().into();
        live.generation_session_index = budget.used();
        drop(budget);
        live.question = options.question.clone();
        live.config = options.session.config.clone();
        live.config_id = catalog::config_id(&live.config).unwrap();
        live.transport_treatment_id=Some(crate::manifest::hash(&serde_json::to_vec(&serde_json::json!({"schema_version":1,"config_id":live.config_id,"filters":live.filters,"payload_mode":live.payload_mode})).unwrap()));
        let transport = options.session.transport.as_ref().unwrap();
        std::fs::write(&transport.trace_path, b"").unwrap();
        live.audit_trace = Some(super::super::mcp_trace::summary(transport).unwrap());
        let mut bytes = serde_json::to_vec(&live).unwrap();
        bytes.push(b'\n');
        std::fs::write(&options.output, bytes).unwrap();
        live
    }

    #[tokio::test]
    async fn the_batch_saves_a_failed_attempt_and_stops_without_hidden_retries() {
        let base = directory();
        let options = options(&base);
        let output = options.output.clone();
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = calls.clone();
        let summary = run_with_executor(options, move |options, _, _| {
            counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            async move { Ok(fake_failed_attempt(&options)) }
        })
        .await
        .expect("a failed live attempt must remain inspectable in the batch");
        assert_eq!(calls.load(std::sync::atomic::Ordering::Relaxed), 1);
        assert_eq!(summary.planned_sessions, 2);
        assert_eq!(summary.saved_sessions, 1);
        assert_eq!(summary.failed, 1);
        assert!(summary.stopped_after_failure);
        let records = read_records(&output).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].record.live.status, "failed");
        assert!(!base.join("NO_MODEL").exists());
        std::fs::remove_dir_all(base).unwrap();
    }

    #[tokio::test]
    async fn the_same_ledger_cannot_be_resumed_with_a_different_comparison_model() {
        let base = directory();
        let first = options(&base);
        run_with_executor(first.clone(), |options, _, _| async move {
            Ok(fake_failed_attempt(&options))
        })
        .await
        .unwrap();
        let mut second = first;
        second.output = base.join("other.jsonl");
        second.plan = plan(&["P06".into()], &[SearchMethod::Bm25], "different-model").unwrap();
        assert!(
            run_with_executor(second.clone(), |_, _, _| async {
                panic!("changed model must be rejected before another execution")
            })
            .await
            .is_err()
        );
        assert!(!second.output.exists());
        std::fs::remove_dir_all(base).unwrap();
    }

    #[tokio::test]
    async fn an_existing_batch_is_rejected_before_any_budget_or_runtime_side_effect() {
        let base = directory();
        let options = options(&base);
        std::fs::write(&options.output, b"original").unwrap();
        assert!(
            run_with_executor(options.clone(), |_, _, _| async {
                panic!("existing batch cannot start execution")
            })
            .await
            .is_err()
        );
        assert_eq!(std::fs::read(&options.output).unwrap(), b"original");
        assert!(!options.ledger.exists());
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn preserved_failed_attempts_are_bound_to_the_original_case_and_live_bytes() {
        let base = directory();
        let record = failed_record(&base);
        validate_record_identity(&record)
            .expect("a failed attempt is still valid evidence of actual work");
        let mut changed = record.clone();
        changed.case_id = "P06".into();
        assert!(validate_record_identity(&changed).is_err());
        let mut changed = record.clone();
        changed.live.question = "another question".into();
        assert!(validate_record_identity(&changed).is_err());
        let mut changed = record.clone();
        changed.dataset_id = "independent-20261103".into();
        assert!(validate_record_identity(&changed).is_err());
        let mut changed = record.clone();
        changed.live.config.retrieval.top_k = 6;
        assert!(validate_record_identity(&changed).is_err());
        std::fs::write(&record.live_output_path, b"tampered live bytes").unwrap();
        assert!(validate_record_identity(&record).is_err());
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn the_reader_hashes_exact_original_lines_and_rejects_duplicate_sessions() {
        let base = directory();
        let record = failed_record(&base);
        let path = base.join("batch.jsonl");
        let line = serde_json::to_vec(&record).unwrap();
        let mut bytes = line.clone();
        bytes.push(b'\n');
        std::fs::write(&path, &bytes).unwrap();
        let stored =
            read_records(&path).expect("agent batch records must be readable without a runtime");
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].record_sha256, crate::manifest::hash(&line));
        bytes.extend_from_slice(&line);
        bytes.push(b'\n');
        std::fs::write(&path, &bytes).unwrap();
        assert!(read_records(&path).is_err());
        std::fs::remove_dir_all(base).unwrap();
    }
}
