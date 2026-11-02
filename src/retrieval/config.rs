use serde::{Deserialize, Serialize};

use super::{Environment, TimeWindow, Weekday};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SearchMethod {
    Dense,
    Bm25,
    HybridRrf,
    HybridWeighted,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SelectionMode {
    Sections,
    ActivityExpansion,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RetrievalConfig {
    pub method: SearchMethod,
    pub selection: SelectionMode,
    pub top_k: usize,
    pub candidate_k: usize,
    pub context_token_limit: usize,
    pub apply_filters: bool,
    pub mmr: bool,
    pub rrf_k: f64,
    pub vector_weight: f64,
    pub mmr_lambda: f64,
}

impl Default for RetrievalConfig {
    fn default() -> Self {
        Self {
            method: SearchMethod::Dense,
            selection: SelectionMode::Sections,
            top_k: 3,
            candidate_k: 30,
            context_token_limit: 1200,
            apply_filters: false,
            mmr: false,
            rrf_k: 60.0,
            vector_weight: 0.7,
            mmr_lambda: 0.7,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct QueryFilters {
    /// Acceptable days are alternatives: the activity must occur on any one.
    pub weekdays: Vec<Weekday>,
    pub environment: Option<Environment>,
    /// The whole activity must fit inside this availability window.
    pub time: Option<TimeWindow>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RetrievalRequest {
    pub query: String,
    #[serde(default)]
    pub filters: QueryFilters,
}
