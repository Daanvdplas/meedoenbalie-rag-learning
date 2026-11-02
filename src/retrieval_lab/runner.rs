use super::{
    preparation::{PreparationOptions, PreparationSession},
    types::{ExperimentPlan, RunSummary},
};
use crate::retrieval::QueryFilters;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct RunOptions {
    pub output: PathBuf,
    pub preparation: PreparationOptions,
    pub filters: QueryFilters,
}

pub fn run(plan: &ExperimentPlan, options: &RunOptions) -> Result<RunSummary, String> {
    use super::types::{
        ExecutionKind, ExecutionStatus, PreparationRecord, RetrievalRecord, RetrievalTimings,
    };
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    if options.output.exists() {
        return Err("Retrieval output already exists; choose a new file".into());
    }
    if !options
        .output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("."))
        .is_dir()
    {
        return Err("Retrieval output parent directory does not exist".into());
    }
    if plan.schema_version != 1
        || plan.planned_rows != plan.cases.len() * plan.configurations.len()
        || plan.planned_rows == 0
        || plan.planned_rows > 10000
        || plan.codex_calls != 0
        || plan.ai_judge_calls != 0
    {
        return Err("Invalid local retrieval plan or row budget".into());
    }
    for config in &plan.configurations {
        crate::retrieval::validate_config(&config.config.retrieval, &options.filters)?;
    }
    let created = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_millis() as u64;
    let batch_id = format!(
        "retrieval-{created}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    let mut session = PreparationSession::new(&options.preparation)?;
    let mut profiles = Vec::new();
    for config in &plan.configurations {
        if !profiles.contains(&config.config.profile) {
            profiles.push(config.config.profile);
        }
    }
    let mut records = Vec::new();
    for profile in profiles {
        let configs: Vec<_> = plan
            .configurations
            .iter()
            .filter(|config| config.config.profile == profile)
            .collect();
        let with_vectors = configs
            .iter()
            .any(|config| config.config.retrieval.method != crate::retrieval::SearchMethod::Bm25);
        let mut prepared = if profile.supported() {
            Some(session.prepare(profile, with_vectors, &batch_id))
        } else {
            None
        };
        let mut keyword_prepared = if with_vectors
            && matches!(&prepared, Some(Err(_)))
            && configs.iter().any(|config| {
                config.config.retrieval.method == crate::retrieval::SearchMethod::Bm25
            }) {
            Some(session.prepare(profile, false, &batch_id))
        } else {
            None
        };
        for config in configs {
            for case in &plan.cases {
                let mut record = RetrievalRecord {
                    schema_version: 1,
                    batch_id: batch_id.clone(),
                    run_id: format!("{batch_id}--{}--{}", case.id, config.config_id),
                    created_at_unix_ms: created,
                    case_id: case.id.clone(),
                    family: case.family.clone(),
                    split: case.split.clone(),
                    kind: case.kind.clone(),
                    question: case.question.clone(),
                    config_id: config.config_id.clone(),
                    config: config.config.clone(),
                    request_filters: options.filters.clone(),
                    execution_kind: ExecutionKind::LocalMeasured,
                    execution_status: ExecutionStatus::Failed,
                    dataset: plan.dataset.clone(),
                    preparation: PreparationRecord::default(),
                    shared_preparation: None,
                    outcome: None,
                    timings: RetrievalTimings::default(),
                    error: Some("Not executed".into()),
                };
                let active = if config.config.retrieval.method
                    == crate::retrieval::SearchMethod::Bm25
                    && keyword_prepared.is_some()
                {
                    &mut keyword_prepared
                } else {
                    &mut prepared
                };
                match active {
                    None => {
                        record.execution_status = ExecutionStatus::Unsupported;
                        record.error = Some(
                            "SQLite is unavailable in this build; enable the sqlite feature".into(),
                        );
                    }
                    Some(Err(error)) => {
                        record.error = Some(format!("Preparation failed: {error}"));
                    }
                    Some(Ok(prepared)) => {
                        record.preparation = prepared.record.clone();
                        let attempt = session.search(
                            prepared,
                            &crate::retrieval::RetrievalRequest {
                                query: case.question.clone(),
                                filters: options.filters.clone(),
                            },
                            &config.config,
                        );
                        record.timings = attempt.timings;
                        match attempt.outcome {
                            Ok(outcome) => {
                                record.outcome = Some(outcome);
                                record.execution_status = ExecutionStatus::Completed;
                                record.error = None;
                            }
                            Err(error) => {
                                record.error = Some(error);
                            }
                        }
                    }
                }
                records.push(record);
            }
        }
    }
    let shared = session.shared(&batch_id);
    for record in &mut records {
        record.shared_preparation = Some(shared.clone());
    }
    super::records::write_records(&options.output, &records)?;
    let count = |status| {
        records
            .iter()
            .filter(|record| record.execution_status == status)
            .count()
    };
    Ok(RunSummary {schema_version: 1, batch_id, planned_rows: plan.planned_rows, completed: count(ExecutionStatus::Completed), failed: count(ExecutionStatus::Failed), unsupported: count(ExecutionStatus::Unsupported), codex_calls: 0, ai_judge_calls: 0,
        output_sha256: crate::manifest::hash(&std::fs::read(&options.output).map_err(|error| error.to_string())?),
        timing_note: "retrieval_ms measures local ranking/selection only. Corpus preparation is shared by profile and query embeddings/caches are reused across configurations; do not sum repeated preparation fields or call this Codex answer latency.".into()})
}

#[cfg(test)]
mod tests {
    use super::super::{
        catalog, records,
        types::{ExecutionKind, ExecutionStatus},
    };
    use super::*;
    use crate::{
        retrieval::{RetrievalConfig, SearchMethod},
        storage::StorageProfile,
    };
    #[test]
    fn local_keyword_matrix_preserves_every_planned_row_without_model_calls() {
        let plan = catalog::plan(
            &[SearchMethod::Bm25],
            &[
                StorageProfile::DocumentsSections,
                StorageProfile::DocumentsActivities,
            ],
            &["T02".into(), "P05".into()],
            &RetrievalConfig::default(),
        )
        .unwrap();
        let output = std::env::temp_dir().join(format!(
            "coa-matrix-{}-{}.jsonl",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let options = RunOptions {
            output: output.clone(),
            preparation: PreparationOptions {
                model_cache: output.with_extension("missing-model"),
                sqlite_dir: output.with_extension("sqlite"),
            },
            filters: Default::default(),
        };
        let result = run(&plan, &options).unwrap();
        assert_eq!(result.planned_rows, 4);
        assert_eq!(result.completed, 4);
        assert_eq!(result.failed + result.unsupported, 0);
        assert_eq!(result.codex_calls + result.ai_judge_calls, 0);
        let stored = records::read_records(&output).unwrap();
        assert_eq!(stored.len(), 4);
        assert!(
            stored.iter().all(|row| row
                .record
                .shared_preparation
                .as_ref()
                .is_some_and(|shared| shared.document_inputs_encoded == 0
                    && shared.query_inputs_encoded == 0
                    && shared.model_load_ms.is_none()))
        );
        assert!(stored.iter().all(|row| {
            row.record.execution_status == ExecutionStatus::Completed
                && row.record.execution_kind == ExecutionKind::LocalMeasured
                && row
                    .record
                    .preparation
                    .corpus_identity
                    .as_ref()
                    .unwrap()
                    .embedding
                    .is_none()
        }));
        let bytes = std::fs::read(&output).unwrap();
        assert_eq!(crate::manifest::hash(&bytes), result.output_sha256);
        assert!(
            !String::from_utf8(bytes)
                .unwrap()
                .contains("evidence_alternatives")
        );
        assert!(!options.preparation.model_cache.exists());
        assert!(run(&plan, &options).is_err());
        std::fs::remove_file(output).unwrap();
    }

    #[test]
    fn a_new_relative_output_filename_works_in_the_current_directory() {
        let output = PathBuf::from(format!(
            "coa-lab-relative-{}-{}.jsonl",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let plan = catalog::plan(
            &[SearchMethod::Bm25],
            &[StorageProfile::DocumentsSections],
            &["T02".into()],
            &RetrievalConfig::default(),
        )
        .unwrap();
        let result = run(
            &plan,
            &RunOptions {
                output: output.clone(),
                preparation: PreparationOptions {
                    model_cache: "/missing-unused-model".into(),
                    sqlite_dir: "/missing-unused-sqlite".into(),
                },
                filters: Default::default(),
            },
        )
        .unwrap();
        assert_eq!(result.completed, 1);
        std::fs::remove_file(output).unwrap();
    }

    #[test]
    fn broken_dense_preparation_does_not_erase_the_independent_keyword_variant() {
        let base = std::env::temp_dir().join(format!("coa-lab-bad-model-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let cache =
            base.join("model/models--Qdrant--paraphrase-multilingual-MiniLM-L12-v2-onnx-Q/refs");
        std::fs::create_dir_all(&cache).unwrap();
        std::fs::write(cache.join("main"), "INVALID-REVISION").unwrap();
        let output = base.join("runs.jsonl");
        let plan = catalog::plan(
            &catalog::METHODS,
            &[StorageProfile::DocumentsSections],
            &["T02".into()],
            &RetrievalConfig::default(),
        )
        .unwrap();
        let result = run(
            &plan,
            &RunOptions {
                output: output.clone(),
                preparation: PreparationOptions {
                    model_cache: base.join("model"),
                    sqlite_dir: base.join("sqlite"),
                },
                filters: Default::default(),
            },
        )
        .unwrap();
        assert_eq!(result.planned_rows, 4);
        assert_eq!(result.failed, 3);
        assert_eq!(result.completed, 1);
        let records = records::read_records(&output).unwrap();
        let keyword = records
            .iter()
            .find(|row| row.record.config.retrieval.method == SearchMethod::Bm25)
            .unwrap();
        assert_eq!(keyword.record.execution_status, ExecutionStatus::Completed);
        assert!(
            records
                .iter()
                .filter(|row| row.record.execution_status == ExecutionStatus::Failed)
                .all(
                    |row| row.record.config.retrieval.method != SearchMethod::Bm25
                        && row.record.outcome.is_none()
                )
        );
        std::fs::remove_dir_all(base).unwrap();
    }

    #[cfg(not(feature = "sqlite"))]
    #[test]
    fn unsupported_sqlite_variants_are_accounted_without_attempting_dense_initialization() {
        let plan = catalog::plan(
            &[SearchMethod::Dense],
            &[StorageProfile::SqliteActivities],
            &["T02".into()],
            &RetrievalConfig::default(),
        )
        .unwrap();
        let output =
            std::env::temp_dir().join(format!("coa-unsupported-{}.jsonl", std::process::id()));
        let _ = std::fs::remove_file(&output);
        let result = run(
            &plan,
            &RunOptions {
                output: output.clone(),
                preparation: PreparationOptions {
                    model_cache: "/missing-unused-model".into(),
                    sqlite_dir: "/missing-unused-sqlite".into(),
                },
                filters: Default::default(),
            },
        )
        .unwrap();
        assert_eq!(result.unsupported, 1);
        assert_eq!(result.completed + result.failed, 0);
        let stored = records::read_records(&output).unwrap();
        assert_eq!(
            stored[0].record.execution_status,
            ExecutionStatus::Unsupported
        );
        assert!(stored[0].record.outcome.is_none());
        std::fs::remove_file(output).unwrap();
    }
}
