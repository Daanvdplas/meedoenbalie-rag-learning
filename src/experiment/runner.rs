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
