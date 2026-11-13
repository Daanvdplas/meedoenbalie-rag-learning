use crate::{
    experiment::dataset::InputCase,
    retrieval::{CorpusIdentity, QueryFilters, RetrievalConfig, RetrievalOutcome},
    storage::{LayoutStats, StorageProfile, SyncStats},
};
use serde::{Deserialize, Serialize};

use super::dataset::DatasetIdentity;

pub const RECORD_SCHEMA_VERSION: u32 = 1;
pub const RECORD_HASH_POLICY: &str = "sha256-exact-jsonl-line-without-newline-v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RunConfig {
    pub profile: StorageProfile,
    pub retrieval: RetrievalConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlannedConfig {
    pub config_id: String,
    pub config: RunConfig,
    pub supported: bool,
    pub unsupported_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExperimentPlan {
    pub schema_version: u32,
    pub dataset: DatasetIdentity,
    pub cases: Vec<InputCase>,
    pub configurations: Vec<PlannedConfig>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub query_inputs: std::collections::BTreeMap<String, QueryFilters>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query_inputs_sha256: Option<String>,
    pub planned_rows: usize,
    pub assessment_types: Vec<String>,
    pub codex_calls: usize,
    pub ai_judge_calls: usize,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    Completed,
    Failed,
    Unsupported,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionKind {
    LocalMeasured,
    Fixture,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreparationRecord {
    pub preparation_id: Option<String>,
    pub actual_profile: Option<StorageProfile>,
    pub corpus_identity: Option<CorpusIdentity>,
    pub layout: Option<LayoutStats>,
    pub sync: Option<SyncStats>,
    pub document_cache_hits: usize,
    pub document_cache_misses: usize,
    pub model_load_ms: Option<f64>,
    pub document_embedding_ms: Option<f64>,
    pub storage_sync_ms: Option<f64>,
    pub storage_load_ms: Option<f64>,
    pub profile_preparation_ms: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetrievalTimings {
    pub query_embedding_ms: Option<f64>,
    pub query_embedding_cache_hit: Option<bool>,
    pub retrieval_ms: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SharedPreparation {
    pub preparation_id: String,
    pub source_parse_ms: f64,
    pub model_identity_ms: Option<f64>,
    pub model_load_ms: Option<f64>,
    pub document_inference_ms: Option<f64>,
    pub query_inference_ms: Option<f64>,
    pub document_inputs_encoded: usize,
    pub query_inputs_encoded: usize,
    pub scoring_path: String,
    pub interpretation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetrievalRecord {
    pub schema_version: u32,
    pub batch_id: String,
    pub run_id: String,
    pub created_at_unix_ms: u64,
    pub case_id: String,
    pub family: String,
    pub split: String,
    pub kind: String,
    pub question: String,
    pub config_id: String,
    pub config: RunConfig,
    pub request_filters: QueryFilters,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query_inputs_sha256: Option<String>,
    pub execution_kind: ExecutionKind,
    pub execution_status: ExecutionStatus,
    pub dataset: DatasetIdentity,
    pub preparation: PreparationRecord,
    #[serde(default)]
    pub shared_preparation: Option<SharedPreparation>,
    pub outcome: Option<RetrievalOutcome>,
    pub timings: RetrievalTimings,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct StoredRecord {
    pub record: RetrievalRecord,
    /// SHA-256 of the exact stored JSONL line, excluding its newline.
    pub record_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunSummary {
    pub schema_version: u32,
    pub batch_id: String,
    pub planned_rows: usize,
    pub completed: usize,
    pub failed: usize,
    pub unsupported: usize,
    pub codex_calls: usize,
    pub ai_judge_calls: usize,
    pub output_sha256: String,
    pub timing_note: String,
}
