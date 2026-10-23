use super::types::*;
use std::{
    io::{BufRead, Write},
    path::Path,
};

pub fn read_records(path: &Path) -> Result<Vec<ExperimentRecord>, String> {
    let file = std::fs::File::open(path).map_err(|_| "Cannot read experiment artifact")?;
    if file
        .metadata()
        .map_err(|_| "Cannot inspect experiment artifact")?
        .len()
        > 64 * 1024 * 1024
    {
        return Err("Experiment artifact exceeds 64 MiB".into());
    }
    let mut records = Vec::new();
    let mut ids = std::collections::HashSet::new();
    for line in std::io::BufReader::new(file).lines() {
        let line = line.map_err(|_| "Cannot read experiment row")?;
        if line.trim().is_empty() {
            continue;
        }
        if line.len() > 2 * 1024 * 1024 || records.len() >= 400 {
            return Err("Experiment artifact exceeds row limits".into());
        }
        let record: ExperimentRecord =
            serde_json::from_str(&line).map_err(|_| "Invalid experiment record schema")?;
        validate_record(&record)?;
        if !ids.insert(record.run_id.clone()) {
            return Err("Duplicate experiment run ID".into());
        }
        records.push(record);
    }
    if records.is_empty() {
        return Err("Experiment artifact contains no records".into());
    }
    Ok(records)
}

fn hex(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

pub fn validate_record(record: &ExperimentRecord) -> Result<(), String> {
    let manifest = &record.manifest;
    if record.schema_version != SCHEMA_VERSION
        || manifest.schema_version != SCHEMA_VERSION
        || record.run_id.is_empty()
        || record.run_id.len() > 160
        || record.case_id.is_empty()
        || record.family.is_empty()
        || record.split.is_empty()
        || record.kind.is_empty()
        || crate::validate_query(&record.question).is_err()
    {
        return Err("Invalid experiment record identity".into());
    }
    manifest.limits.validate()?;
    for digest in [
        &manifest.questions_sha256,
        &manifest.expectations_sha256,
        &manifest.corpus_sha256,
        &manifest.generation_prompt_sha256,
        &manifest.rubric_sha256,
        &manifest.binary_sha256,
        &manifest.runtime_sha256,
        &manifest.code_mode_host_sha256,
    ] {
        if !hex(digest) {
            return Err("Invalid experiment fingerprint".into());
        }
    }
    if manifest.as_of != crate::AS_OF
        || manifest.corpus_sha256
            != crate::manifest::hash(crate::DOCUMENTS.join("\n---document---\n").as_bytes())
    {
        return Err("Experiment corpus differs from this executable".into());
    }
    let sources = crate::source::parse_sources(crate::DOCUMENTS)?;
    if manifest.sources.len() != sources.len()
        || sources.iter().zip(crate::DOCUMENTS).any(|(source, raw)| {
            !manifest.sources.iter().any(|s| {
                s.id == source.id
                    && s.title == source.title
                    && s.date == source.date
                    && s.sha256 == crate::manifest::hash(raw.as_bytes())
            })
        })
    {
        return Err("Invalid experiment source identity".into());
    }
    if record.execution_kind == ExecutionKind::Live
        && (manifest.generator_route != "OpenAI via locally authenticated Codex CLI"
            || !record.trace_complete && record.execution_status == ExecutionStatus::Completed)
    {
        return Err("Live record lacks genuine execution identity".into());
    }
    if record.execution_kind == ExecutionKind::Live {
        let budget = manifest
            .budget
            .as_ref()
            .ok_or("Live record lacks a global budget reservation")?;
        if budget.schema_version != 1
            || budget.id.is_empty()
            || budget.max_generation_sessions != 40
            || budget.max_grading_sessions != 56
            || budget.max_seconds != 14400
            || (record.execution_status == ExecutionStatus::Completed
                && budget.generation_sessions_used == 0)
            || budget.generation_sessions_used > 40
            || budget.grading_sessions_used > 56
        {
            return Err("Invalid global experiment budget provenance".into());
        }
        let prompt = if record.arm == Arm::AgenticDense {
            crate::codex::generation_prompt(&record.question, manifest.limits.top_k)
        } else {
            super::arms::context_prompt(&record.question, &record.offered_sections)?
        };
        if crate::manifest::hash(prompt.as_bytes()) != manifest.generation_prompt_sha256 {
            return Err("Generation prompt fingerprint mismatch".into());
        }
        if matches!(record.arm, Arm::AgenticDense | Arm::DenseOnce)
            && (manifest
                .embedding_revision
                .as_ref()
                .is_none_or(|r| r.len() != 40 || !r.bytes().all(|b| b.is_ascii_hexdigit()))
                || manifest.embedding_files.len() != 5
                || manifest.embedding_files.iter().any(|f| {
                    !hex(&f.sha256)
                        || !matches!(
                            f.name.as_str(),
                            "model_optimized.onnx"
                                | "tokenizer.json"
                                | "config.json"
                                | "tokenizer_config.json"
                                | "special_tokens_map.json"
                        )
                }))
        {
            return Err("Invalid embedding weights identity".into());
        }
    }
    for section in &record.offered_sections {
        let original = sources
            .iter()
            .flat_map(|s| &s.sections)
            .find(|s| s.source_id == section.source_id && s.id == section.id);
        if original.is_none_or(|s| {
            s.source_date != section.source_date
                || s.source_title != section.source_title
                || s.heading != section.heading
                || s.text != section.text
        }) {
            return Err("Offered context differs from the pinned corpus".into());
        }
    }
    if record.offered_sections.len() > 30
        || record.tool_calls.len() > manifest.limits.max_search_calls
        || record.retrieval_steps.len() > 4
    {
        return Err("Experiment execution limit exceeded".into());
    }
    if record.arm != Arm::AgenticDense && !record.tool_calls.is_empty() {
        return Err("No-tool evaluation arm contains tool calls".into());
    }
    for step in &record.retrieval_steps {
        if !matches!(step.origin.as_str(), "mcp" | "preselection")
            || crate::validate_query(&step.query).is_err()
            || !(1..=6).contains(&step.top_k)
            || step.hits.len() > step.top_k
            || step.hits.iter().any(|hit| {
                !hit.score.is_finite()
                    || !matches!(hit.score_kind.as_str(), "cosine" | "bm25")
                    || !record.offered_sections.iter().any(|s| {
                        s.source_id == hit.section.source_id
                            && s.id == hit.section.id
                            && s.text == hit.section.text
                    })
            })
        {
            return Err("Invalid recorded retrieval step".into());
        }
    }
    match record.execution_status {
        ExecutionStatus::Completed => {
            let answer = record
                .answer
                .as_ref()
                .ok_or("Completed execution is missing its answer")?;
            if record.error.is_some() {
                return Err("Completed execution contains an error".into());
            }
            super::arms::validate_answer(answer, &record.offered_sections)?;
            if record.execution_kind == ExecutionKind::Live {
                if record.arm == Arm::AgenticDense && record.tool_calls.is_empty() {
                    return Err("Agent execution contains no real MCP call".into());
                }
                if record.arm == Arm::FullContext && record.offered_sections.len() != 30 {
                    return Err("Full-context execution did not offer all sections".into());
                }
            }
        }
        _ => {
            if record.answer.is_some()
                || record.error.as_deref().is_none_or(|s| {
                    !matches!(
                        s,
                        "runtime_failed"
                            | "timeout"
                            | "cancelled"
                            | "budget_exhausted"
                            | "invalid_output"
                            | "invalid_evidence"
                            | "not_run"
                    )
                })
            {
                return Err("Invalid failed/skipped execution record".into());
            }
        }
    }
    // Strictly typed grading/metric validation is added with their respective modules.
    if record.metrics.is_some() || record.judgement.is_some() {
        return Err("Unsupported assessment payload version".into());
    }
    Ok(())
}

pub fn write_record(file: &mut std::fs::File, record: &ExperimentRecord) -> Result<(), String> {
    validate_record(record)?;
    serde_json::to_writer(&mut *file, record).map_err(|_| "Cannot encode experiment result")?;
    file.write_all(b"\n")
        .and_then(|_| file.flush())
        .map_err(|_| "Cannot persist experiment result".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn record() -> ExperimentRecord {
        let manifest = json!({"schema_version":1,"experiment_id":"test","created_at_unix_ms":1,"questions_version":"v1","questions_sha256":"a".repeat(64),"expectations_version":"v1","expectations_sha256":"b".repeat(64),"corpus_sha256":crate::manifest::hash(crate::DOCUMENTS.join("\n---document---\n").as_bytes()),"sources":crate::source::parse_sources(crate::DOCUMENTS).unwrap().iter().zip(crate::DOCUMENTS).map(|(s,raw)|json!({"id":s.id,"title":s.title,"date":s.date,"sha256":crate::manifest::hash(raw.as_bytes())})).collect::<Vec<_>>(),"as_of":crate::AS_OF,"generation_prompt_version":"v1","generation_prompt_sha256":"c".repeat(64),"rubric_version":"v1","rubric_sha256":"d".repeat(64),"model":"test-model","client_version":"codex-cli test","binary_sha256":"e".repeat(64),"runtime_sha256":"f".repeat(64),"code_mode_host_sha256":"a".repeat(64),"generator_route":"fixture","grader_route":"fixture","embedding_model":crate::MODEL,"embedding_files":[],"reproducibility_limitations":["fixture"],"limits":{"max_cases":2,"max_generations":4,"max_judgements":4,"max_seconds":600,"per_call_seconds":120,"concurrency":1,"max_search_calls":4,"top_k":3}});
        let mut value = json!({"schema_version":1,"run_id":"test","case_id":"q1","family":"days","split":"pilot","kind":"fact","arm":"full_context","execution_kind":"fixture","execution_status":"completed","question":"Wanneer tuinieren?","manifest":null,"answer":{"text":"Onbekend.","citations":[],"unknowns":["Tijd onbekend"]},"tool_calls":[],"offered_sections":[],"elapsed_ms":1,"usage":null,"error":null,"metrics":null,"judgement":null});
        value["manifest"] = manifest;
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn artifact_validation_rejects_fake_live_identity_inconsistent_status_and_source_drift() {
        let mut item = record();
        assert!(validate_record(&item).is_ok());
        item.execution_kind = ExecutionKind::Live;
        assert!(validate_record(&item).is_err());
        item = record();
        item.execution_status = ExecutionStatus::Failed;
        assert!(validate_record(&item).is_err());
        item = record();
        item.manifest.corpus_sha256 = "z".repeat(64);
        assert!(validate_record(&item).is_err());
    }

    #[test]
    fn missing_or_malformed_artifact_is_never_an_empty_success() {
        assert!(read_records(Path::new("/missing/PRIVATE-CANARY.jsonl")).is_err());
        let path =
            std::env::temp_dir().join(format!("coa-invalid-artifact-{}.jsonl", std::process::id()));
        std::fs::write(&path, "\n").unwrap();
        assert!(read_records(&path).is_err());
        std::fs::write(&path, "{\"schema_version\":1}\n").unwrap();
        assert!(read_records(&path).is_err());
        std::fs::remove_file(path).unwrap();
    }
}

#[derive(Debug)]
pub struct RunOptions {
    pub output: std::path::PathBuf,
    pub ledger: std::path::PathBuf,
    pub arms: Vec<Arm>,
    pub cases: Vec<String>,
    pub limits: Limits,
}

pub async fn run(
    options: RunOptions,
    questions: super::dataset::QuestionSet,
    labels: super::dataset::LabelSet,
) -> Result<(), String> {
    use std::{
        fs::OpenOptions,
        time::{Duration, Instant},
    };
    options.limits.validate()?;
    if options.arms.is_empty()
        || options.cases.is_empty()
        || options.cases.len() > options.limits.max_cases
        || options.cases.len() * options.arms.len() > options.limits.max_generations
    {
        return Err("Selected cases/arms exceed declared generation limits".into());
    }
    let mut seen = std::collections::HashSet::new();
    if options.arms.iter().any(|a| !seen.insert(a.name())) {
        return Err("Duplicate arm selection".into());
    }
    let mut seen = std::collections::HashSet::new();
    let cases = options
        .cases
        .iter()
        .map(|id| {
            if !seen.insert(id.clone()) {
                return Err("Duplicate case selection".to_string());
            }
            questions
                .cases
                .iter()
                .find(|case| &case.id == id)
                .cloned()
                .ok_or("Unknown experiment case".into())
        })
        .collect::<Result<Vec<_>, String>>()?;
    let mut ledger = super::budget::Ledger::open(&options.ledger)?;
    let runtime = crate::codex::CodexRunner::new()?;
    let sources = crate::source::parse_sources(crate::DOCUMENTS)?;
    let mut index = if options.arms.contains(&Arm::DenseOnce) {
        Some(crate::Index::load().map_err(|_| "Cannot load local embedding index")?)
    } else {
        None
    };
    let (revision, embedding_files) = if options
        .arms
        .iter()
        .any(|a| matches!(a, Arm::AgenticDense | Arm::DenseOnce))
    {
        embedding_identity()?
    } else {
        (None, vec![])
    };
    let mut creation = OpenOptions::new();
    creation.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        creation.mode(0o600);
    }
    let mut output = creation.open(&options.output).map_err(
        |_| "Cannot create new experiment artifact (existing files are never overwritten)",
    )?;
    let config = runtime.configuration();
    let text = |key: &str| config[key].as_str().unwrap_or_default().to_owned();
    let fingerprint = |key: &str| {
        config["binaries"][key]
            .as_str()
            .unwrap_or_default()
            .to_owned()
    };
    let experiment_id = format!(
        "experiment-{}-{}",
        super::budget::now_ms()?,
        std::process::id()
    );
    let started = Instant::now();
    let mut cancelled = false;
    for case in &cases {
        for arm in &options.arms {
            let mut record=ExperimentRecord {
            schema_version:1,run_id:format!("{experiment_id}-{}-{}",case.id,arm.name()),case_id:case.id.clone(),family:case.family.clone(),split:case.split.clone(),kind:case.kind.clone(),arm:*arm,execution_kind:ExecutionKind::Live,execution_status:ExecutionStatus::Skipped,question:case.question.clone(),
            manifest:Manifest {schema_version:1,experiment_id:experiment_id.clone(),created_at_unix_ms:super::budget::now_ms()?,questions_version:questions.dataset_version.clone(),questions_sha256:super::dataset::questions_hash(&questions)?,expectations_version:labels.dataset_version.clone(),expectations_sha256:crate::manifest::hash(&serde_json::to_vec(&labels).map_err(|_|"Cannot hash criteria")?),corpus_sha256:questions.corpus_sha256.clone(),sources:sources.iter().zip(crate::DOCUMENTS).map(|(s,raw)|SourceIdentity{id:s.id.clone(),title:s.title.clone(),date:s.date.clone(),sha256:crate::manifest::hash(raw.as_bytes())}).collect(),as_of:crate::AS_OF.into(),generation_prompt_version:if *arm==Arm::AgenticDense {text("prompt_version")} else {super::arms::CONTEXT_PROMPT_VERSION.into()},generation_prompt_sha256:String::new(),rubric_version:"semantic-rubric-v1-unvalidated".into(),rubric_sha256:crate::manifest::hash(b"semantic-rubric-v1-unvalidated"),model:text("model"),client_version:text("client_version"),binary_sha256:fingerprint("mcp"),runtime_sha256:fingerprint("codex"),code_mode_host_sha256:fingerprint("code_mode_host"),generator_route:"OpenAI via locally authenticated Codex CLI".into(),grader_route:"not graded".into(),embedding_model:crate::MODEL.into(),embedding_files:embedding_files.clone(),embedding_revision:revision.clone(),reproducibility_limitations:vec!["Model alias/provider routing may change; binary and input hashes do not guarantee bitwise model reproduction.".into(),"Session budgets count Codex sessions, not hidden provider requests.".into(),"Semantic grading is provisional; no human calibration.".into()],limits:options.limits.clone(),budget:Some(ledger.state.clone())},answer:None,tool_calls:vec![],retrieval_steps:vec![],trace_complete:false,offered_sections:vec![],elapsed_ms:0,usage:None,error:Some("not_run".into()),metrics:None,judgement:None,
        };
            if *arm != Arm::AgenticDense {
                let hits = match arm {
                    Arm::FullContext => vec![],
                    Arm::Bm25 => super::arms::bm25(&sources, &case.question, options.limits.top_k)?,
                    Arm::DenseOnce => {
                        index
                            .as_mut()
                            .ok_or("Dense index unavailable")?
                            .search_with_top_k(&case.question, options.limits.top_k)
                            .map_err(|_| "Dense retrieval failed")?
                            .hits
                    }
                    _ => unreachable!(),
                };
                if *arm == Arm::FullContext {
                    record.offered_sections =
                        sources.iter().flat_map(|s| s.sections.clone()).collect();
                } else {
                    record.offered_sections = hits.iter().map(|h| h.section.clone()).collect();
                    record.retrieval_steps.push(RetrievalStep {
                        origin: "preselection".into(),
                        query: case.question.clone(),
                        top_k: options.limits.top_k,
                        hits: hits
                            .into_iter()
                            .map(|hit| RankedSection {
                                section: hit.section,
                                score: hit.similarity as f64,
                                score_kind: if *arm == Arm::Bm25 { "bm25" } else { "cosine" }
                                    .into(),
                            })
                            .collect(),
                    });
                }
            }
            let prompt = if *arm == Arm::AgenticDense {
                crate::codex::generation_prompt(&case.question, options.limits.top_k)
            } else {
                super::arms::context_prompt(&case.question, &record.offered_sections)?
            };
            // Only user input and supplied public corpus sections cross the generator boundary.
            // Protected criteria/canary are held exclusively in this orchestration process.
            if prompt.contains(&labels.evaluation_only_canary) {
                return Err("Protected criteria leaked into generation request".into());
            }
            record.manifest.generation_prompt_sha256 = crate::manifest::hash(prompt.as_bytes());
            let invocation_remaining =
                Duration::from_secs(options.limits.max_seconds).checked_sub(started.elapsed());
            let remaining = ledger
                .remaining()
                .ok()
                .zip(invocation_remaining)
                .map(|(a, b)| a.min(b));
            let timeout =
                remaining.map(|r| r.min(Duration::from_secs(options.limits.per_call_seconds)));
            if cancelled {
                record.execution_status = ExecutionStatus::Cancelled;
                record.error = Some("cancelled".into());
            } else if timeout.is_none_or(|t| t.is_zero()) || ledger.reserve_generation().is_err() {
                record.error = Some("budget_exhausted".into());
            } else {
                record.manifest.budget = Some(ledger.state.clone());
                let timeout = timeout.unwrap();
                let call_started = Instant::now();
                let generation = async {
                    if *arm == Arm::AgenticDense {
                        let run = runtime.run(&case.question, options.limits.top_k).await?;
                        canonicalize_agent(&mut record, run)?;
                    } else {
                        let result = runtime
                            .run_isolated_json(&prompt, &crate::codex::output_schema(), timeout)
                            .await?;
                        let answer = serde_json::from_value(result.value)
                            .map_err(|_| "output: Invalid grounded answer")?;
                        super::arms::validate_answer(&answer, &record.offered_sections)?;
                        record.answer = Some(answer);
                        record.usage = result.usage;
                        record.trace_complete = true;
                    }
                    Ok::<(), String>(())
                };
                let result = tokio::select! {
                    result=tokio::time::timeout(timeout,generation)=>match result {Ok(v)=>v,Err(_)=>Err("timeout".into())},
                    _=tokio::signal::ctrl_c()=>{cancelled=true;Err("cancelled".into())}
                };
                record.elapsed_ms = call_started.elapsed().as_millis() as u64;
                match result {
                    Ok(()) => {
                        record.execution_status = ExecutionStatus::Completed;
                        record.error = None;
                    }
                    Err(error) => {
                        record.answer = None;
                        record.execution_status = if error.contains("timeout") {
                            ExecutionStatus::TimedOut
                        } else if error == "cancelled" {
                            ExecutionStatus::Cancelled
                        } else {
                            ExecutionStatus::Failed
                        };
                        record.error = Some(
                            if error.contains("timeout") {
                                "timeout"
                            } else if error == "cancelled" {
                                "cancelled"
                            } else if error.starts_with("output:") {
                                "invalid_output"
                            } else {
                                "runtime_failed"
                            }
                            .into(),
                        );
                    }
                }
            }
            write_record(&mut output, &record)?;
            println!("{} {} {:?}", case.id, arm.name(), record.execution_status);
        }
    }
    Ok(())
}

fn canonicalize_agent(
    record: &mut ExperimentRecord,
    run: crate::codex::CodexRun,
) -> Result<(), String> {
    let sources = crate::source::parse_sources(crate::DOCUMENTS)?;
    let mut calls = run.tool_calls;
    for call in &mut calls {
        if call.server != "coa" || call.tool != "search_document" {
            return Err("output: Unexpected retrieval tool".into());
        }
        let mut hits = vec![];
        if let Some(result) = &call.result {
            let values = result["hits"]
                .as_array()
                .ok_or("output: Missing retrieved passages")?;
            for value in values {
                let offered: crate::Section = serde_json::from_value(value["section"].clone())
                    .map_err(|_| "output: Invalid retrieved section")?;
                let section = sources
                    .iter()
                    .flat_map(|s| &s.sections)
                    .find(|s| {
                        s.source_id == offered.source_id
                            && s.id == offered.id
                            && s.text == offered.text
                            && s.source_date == offered.source_date
                            && s.heading == offered.heading
                            && s.source_title == offered.source_title
                    })
                    .ok_or("output: Retrieved evidence differs from pinned corpus")?
                    .clone();
                let score = value["similarity"]
                    .as_f64()
                    .filter(|v| v.is_finite())
                    .ok_or("output: Missing retrieval score")?;
                hits.push(RankedSection {
                    section: section.clone(),
                    score,
                    score_kind: "cosine".into(),
                });
                if !record
                    .offered_sections
                    .iter()
                    .any(|s| s.source_id == section.source_id && s.id == section.id)
                {
                    record.offered_sections.push(section);
                }
            }
        }
        record.retrieval_steps.push(RetrievalStep {
            origin: "mcp".into(),
            query: call.query.clone(),
            top_k: call.top_k,
            hits: hits.clone(),
        });
        // Replace arbitrary runtime JSON with an explicit shareable allowlist.
        if call.result.is_some() {
            call.result = Some(
                serde_json::json!({"hits":hits.iter().map(|h|serde_json::json!({"section":h.section,"similarity":h.score})).collect::<Vec<_>>()}),
            );
        }
        if call.error.is_some() {
            call.error = Some("Search tool failed".into());
        }
    }
    super::arms::validate_answer(&run.answer, &record.offered_sections)?;
    record.answer = Some(run.answer);
    record.usage = run.usage;
    record.tool_calls = calls;
    record.trace_complete = true;
    Ok(())
}

fn embedding_identity() -> Result<(Option<String>, Vec<ModelFile>), String> {
    let base =
        Path::new(".model-cache/models--Qdrant--paraphrase-multilingual-MiniLM-L12-v2-onnx-Q");
    let revision = std::fs::read_to_string(base.join("refs/main")).map_err(
        |_| "Embedding revision unavailable; run from the existing model-cache directory",
    )?;
    let revision = revision.trim();
    if revision.len() != 40 || !revision.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("Invalid embedding revision".into());
    }
    let snapshot = base.join("snapshots").join(revision);
    let names = [
        "model_optimized.onnx",
        "config.json",
        "tokenizer.json",
        "tokenizer_config.json",
        "special_tokens_map.json",
    ];
    let mut files = vec![];
    for name in names {
        let bytes = std::fs::read(snapshot.join(name))
            .map_err(|_| "Embedding model identity file unavailable")?;
        files.push(ModelFile {
            name: name.into(),
            sha256: crate::manifest::hash(&bytes),
        });
    }
    Ok((Some(revision.into()), files))
}
