use std::{collections::BTreeSet, io::Write, path::Path};

use super::types::{RetrievalRecord, StoredRecord};

pub const MAX_ARTIFACT_BYTES: usize = 128 * 1024 * 1024;

pub fn validate_record(record: &RetrievalRecord) -> Result<(), String> {
    use super::types::{ExecutionStatus, RECORD_SCHEMA_VERSION};
    if record.schema_version != RECORD_SCHEMA_VERSION
        || record.run_id.is_empty()
        || record.batch_id.is_empty()
        || record.case_id.is_empty()
        || record.config_id != super::catalog::config_id(&record.config)?
        || crate::validate_query(&record.question)? != record.question
    {
        return Err("Invalid retrieval record schema, identity or question".into());
    }
    for value in [
        record.preparation.model_load_ms,
        record.preparation.document_embedding_ms,
        record.preparation.storage_sync_ms,
        record.preparation.storage_load_ms,
        record.preparation.profile_preparation_ms,
        record.timings.query_embedding_ms,
        record.timings.retrieval_ms,
    ]
    .into_iter()
    .flatten()
    {
        if !value.is_finite() || value < 0.0 {
            return Err("Retrieval stage timings must be finite nonnegative measurements".into());
        }
    }
    if let Some(shared) = &record.shared_preparation
        && (shared.preparation_id.is_empty()
            || !shared.source_parse_ms.is_finite()
            || shared.source_parse_ms < 0.0
            || [
                shared.model_identity_ms,
                shared.model_load_ms,
                shared.document_inference_ms,
                shared.query_inference_ms,
            ]
            .into_iter()
            .flatten()
            .any(|value| !value.is_finite() || value < 0.0))
    {
        return Err("Invalid shared preparation provenance or timing".into());
    }
    match record.execution_status {
        ExecutionStatus::Completed => {
            let outcome = record
                .outcome
                .as_ref()
                .ok_or("Completed retrieval has no outcome")?;
            if record.error.is_some()
                || record.preparation.actual_profile != Some(record.config.profile)
                || record.preparation.corpus_identity.as_ref() != Some(&outcome.corpus_identity)
                || record.preparation.preparation_id.is_none()
                || record.preparation.layout.is_none()
                || record.timings.retrieval_ms.is_none()
                || outcome.query != record.question
                || outcome.config != record.config.retrieval
            {
                return Err("Completed retrieval does not preserve its actual configuration and preparation".into());
            }
            for score in outcome
                .candidates
                .iter()
                .map(|candidate| &candidate.scores)
                .chain(outcome.hits.iter().filter_map(|hit| hit.scores.as_ref()))
            {
                if !score.rank_score.is_finite()
                    || score
                        .dense_similarity
                        .is_some_and(|value| !value.is_finite())
                    || score.bm25_score.is_some_and(|value| !value.is_finite())
                {
                    return Err("Retrieval contains nonfinite ranking scores".into());
                }
            }
        }
        ExecutionStatus::Failed | ExecutionStatus::Unsupported => {
            if record.outcome.is_some()
                || record
                    .error
                    .as_deref()
                    .is_none_or(|value| value.trim().is_empty())
            {
                return Err(
                    "Failed/unsupported retrieval needs an error and no invented outcome".into(),
                );
            }
        }
    }
    Ok(())
}

pub fn write_jsonl_new<T: serde::Serialize>(path: &Path, values: &[T]) -> Result<(), String> {
    if values.is_empty() {
        return Err("Cannot create an empty result artifact".into());
    }
    let lines: Vec<_> = values
        .iter()
        .map(serde_json::to_vec)
        .collect::<Result<_, _>>()
        .map_err(|error| error.to_string())?;
    if lines.iter().map(|line| line.len() + 1).sum::<usize>() > MAX_ARTIFACT_BYTES {
        return Err("Result artifact exceeds the 128 MiB limit".into());
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| {
            format!(
                "Cannot create new result artifact {}: {error}",
                path.display()
            )
        })?;
    for line in lines {
        file.write_all(&line)
            .and_then(|_| file.write_all(b"\n"))
            .map_err(|error| error.to_string())?;
    }
    file.sync_all().map_err(|error| error.to_string())
}

pub fn write_records(path: &Path, records: &[RetrievalRecord]) -> Result<(), String> {
    let mut ids = BTreeSet::new();
    for record in records {
        validate_record(record)?;
        if !ids.insert(&record.run_id) {
            return Err("Duplicate retrieval run identity".into());
        }
    }
    write_jsonl_new(path, records)
}

pub fn read_records(path: &Path) -> Result<Vec<StoredRecord>, String> {
    let metadata = std::fs::metadata(path).map_err(|error| error.to_string())?;
    if metadata.len() > MAX_ARTIFACT_BYTES as u64 {
        return Err("Result artifact exceeds the 128 MiB limit".into());
    }
    let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
    let mut records = Vec::new();
    let mut ids = BTreeSet::new();
    let lines: Vec<_> = bytes.split(|byte| *byte == b'\n').collect();
    for (index, line) in lines.iter().enumerate() {
        if line.is_empty() && index + 1 == lines.len() {
            continue;
        }
        if line.is_empty() {
            return Err("Empty line inside retrieval result artifact".into());
        }
        let record: RetrievalRecord = serde_json::from_slice(line)
            .map_err(|error| format!("Invalid retrieval record at line {}: {error}", index + 1))?;
        validate_record(&record)?;
        if !ids.insert(record.run_id.clone()) {
            return Err("Duplicate retrieval run identity".into());
        }
        records.push(StoredRecord {
            record,
            record_sha256: crate::manifest::hash(line),
        });
    }
    if records.is_empty() {
        return Err("Retrieval result artifact is empty".into());
    }
    Ok(records)
}

#[cfg(test)]
pub(crate) fn fixture_record() -> RetrievalRecord {
    fixture_record_for("T02")
}

#[cfg(test)]
pub(crate) fn fixture_record_for(case_id: &str) -> RetrievalRecord {
    use super::{catalog, types::*};
    use crate::retrieval::{PreparedCorpus, RetrievalConfig, RetrievalRequest, SearchMethod};
    use crate::storage::{LayoutStats, StorageProfile};
    let plan = catalog::plan(
        &[SearchMethod::Bm25],
        &[StorageProfile::DocumentsSections],
        &[case_id.into()],
        &RetrievalConfig::default(),
    )
    .unwrap();
    let case = &plan.cases[0];
    let config = &plan.configurations[0];
    let sections = crate::source::parse_sources(crate::DOCUMENTS)
        .unwrap()
        .into_iter()
        .flat_map(|source| source.sections)
        .collect();
    let corpus = PreparedCorpus::new(sections, None, None).unwrap();
    let outcome = crate::retrieval::retrieve(
        &corpus,
        &RetrievalRequest {
            query: case.question.clone(),
            filters: Default::default(),
        },
        &config.config.retrieval,
        None,
    )
    .unwrap();
    RetrievalRecord {
        schema_version: 1,
        batch_id: "fixture-batch".into(),
        run_id: format!("fixture-{case_id}-bm25"),
        created_at_unix_ms: 1,
        case_id: case.id.clone(),
        family: case.family.clone(),
        split: case.split.clone(),
        kind: case.kind.clone(),
        question: case.question.clone(),
        config_id: config.config_id.clone(),
        config: config.config.clone(),
        request_filters: Default::default(),
        query_inputs_sha256: None,
        execution_kind: ExecutionKind::Fixture,
        execution_status: ExecutionStatus::Completed,
        dataset: plan.dataset,
        preparation: PreparationRecord {
            preparation_id: Some("fixture-preparation".into()),
            actual_profile: Some(config.config.profile),
            corpus_identity: Some(corpus.identity),
            layout: Some(LayoutStats {
                canonical_sections: 30,
                activity_records: 0,
                activity_section_links: 0,
            }),
            ..Default::default()
        },
        shared_preparation: None,
        outcome: Some(outcome),
        timings: RetrievalTimings {
            retrieval_ms: Some(0.1),
            ..Default::default()
        },
        error: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    pub(crate) fn output() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "coa-lab-record-{}-{}.jsonl",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn stored_identity_uses_exact_bytes_and_output_is_never_overwritten() {
        let path = output();
        let record = fixture_record();
        write_records(&path, &[record]).unwrap();
        let original = std::fs::read(&path).unwrap();
        let stored = read_records(&path).unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(
            stored[0].record_sha256,
            crate::manifest::hash(original.strip_suffix(b"\n").unwrap())
        );
        assert!(write_records(&path, &[fixture_record()]).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original);
        let mut exact_line = original.strip_suffix(b"\n").unwrap().to_vec();
        exact_line.extend_from_slice(b" \n");
        std::fs::write(&path, &exact_line).unwrap();
        let reread = read_records(&path).unwrap();
        assert_eq!(
            reread[0].record_sha256,
            crate::manifest::hash(exact_line.strip_suffix(b"\n").unwrap())
        );
        assert_ne!(reread[0].record_sha256, stored[0].record_sha256);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn invalid_record_schema_duplicate_identity_and_false_completion_are_rejected() {
        let path = output();
        let record = fixture_record();
        let line = serde_json::to_string(&record).unwrap();
        std::fs::write(&path, format!("{line}\n{line}\n")).unwrap();
        assert!(read_records(&path).is_err());
        let mut record = record;
        record.schema_version = 99;
        std::fs::write(&path, serde_json::to_vec(&record).unwrap()).unwrap();
        assert!(read_records(&path).is_err());
        record.schema_version = 1;
        record.outcome = None;
        std::fs::write(&path, serde_json::to_vec(&record).unwrap()).unwrap();
        assert!(read_records(&path).is_err());
        std::fs::remove_file(path).unwrap();
    }
}
