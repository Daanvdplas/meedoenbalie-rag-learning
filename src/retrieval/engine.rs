use crate::Section;
use serde::{Deserialize, Serialize};

use super::{CorpusIdentity, PreparedCorpus, RetrievalConfig, RetrievalRequest};

pub const RETRIEVAL_VERSION: &str = "retrieval-v1";
pub const TOKEN_ESTIMATOR: &str = "unicode-scalar-ceil-div4-json-section-v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct SectionKey {
    pub source_id: String,
    pub section_id: String,
}

impl From<&Section> for SectionKey {
    fn from(section: &Section) -> Self {
        Self {
            source_id: section.source_id.clone(),
            section_id: section.id.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ScoreBreakdown {
    pub rank_score: f64,
    pub dense_similarity: Option<f32>,
    pub bm25_score: Option<f32>,
    pub dense_rank: Option<usize>,
    pub keyword_rank: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RankedCandidate {
    pub key: SectionKey,
    pub scores: ScoreBreakdown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Inclusion {
    Seed,
    ActivityExpansion,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetrievalHit {
    pub section: Section,
    pub scores: Option<ScoreBreakdown>,
    pub inclusion: Inclusion,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FilterUnknown {
    pub source_id: String,
    pub fields: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FilterReport {
    pub requested: bool,
    pub applied: bool,
    pub ignored_reason: Option<String>,
    pub matched_activity_ids: Vec<String>,
    pub excluded_activity_ids: Vec<String>,
    pub unknown_activities: Vec<FilterUnknown>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ContextOmission {
    pub key: SectionKey,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RetrievalWork {
    pub dense_vectors_scored: usize,
    pub lexical_sections_scored: usize,
    pub eligible_sections: usize,
    pub union_candidates: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetrievalOutcome {
    pub version: String,
    pub query: String,
    pub config: RetrievalConfig,
    pub corpus_identity: CorpusIdentity,
    pub filters: FilterReport,
    pub candidates: Vec<RankedCandidate>,
    pub seeds: Vec<SectionKey>,
    pub hits: Vec<RetrievalHit>,
    pub omitted: Vec<ContextOmission>,
    pub estimated_context_tokens: usize,
    pub token_estimator: String,
    pub work: RetrievalWork,
}

pub fn retrieve(
    _corpus: &PreparedCorpus,
    _request: &RetrievalRequest,
    _config: &RetrievalConfig,
    _query_vector: Option<&[f32]>,
) -> Result<RetrievalOutcome, String> {
    Err("Retrieval is not implemented yet".into())
}

pub fn estimate_section_tokens(section: &Section) -> usize {
    serde_json::to_string(section)
        .expect("Serializing a section of strings is infallible")
        .chars()
        .count()
        .div_ceil(4)
}
