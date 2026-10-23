use crate::{
    Section,
    codex::{GroundedAnswer, ToolCall},
};
use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Arm {
    AgenticDense,
    FullContext,
    DenseOnce,
    Bm25,
}
impl Arm {
    pub const ALL: [Self; 4] = [
        Self::AgenticDense,
        Self::FullContext,
        Self::DenseOnce,
        Self::Bm25,
    ];
    pub fn parse(value: &str) -> Result<Self, String> {
        serde_json::from_value(serde_json::Value::String(value.to_owned()))
            .map_err(|_| "Unknown experiment arm".into())
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::AgenticDense => "agentic_dense",
            Self::FullContext => "full_context",
            Self::DenseOnce => "dense_once",
            Self::Bm25 => "bm25",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    Completed,
    Failed,
    TimedOut,
    Cancelled,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionKind {
    Live,
    Fixture,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_input_tokens: Option<u64>,
    pub reasoning_output_tokens: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub max_cases: usize,
    pub max_generations: usize,
    pub max_judgements: usize,
    pub max_seconds: u64,
    pub per_call_seconds: u64,
    pub concurrency: usize,
    pub max_search_calls: usize,
    pub top_k: usize,
}
impl Limits {
    pub fn validate(&self) -> Result<(), String> {
        if self.max_cases == 0
            || self.max_cases > 100
            || self.max_generations == 0
            || self.max_generations > 400
            || self.max_judgements > 400
            || self.max_seconds == 0
            || self.max_seconds > 14400
            || self.per_call_seconds == 0
            || self.per_call_seconds > 120
            || self.concurrency != 1
            || self.max_search_calls != 4
            || !(1..=6).contains(&self.top_k)
        {
            return Err("Invalid experiment limits".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceIdentity {
    pub id: String,
    pub title: String,
    pub date: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelFile {
    pub name: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema_version: u32,
    pub experiment_id: String,
    pub created_at_unix_ms: u64,
    pub questions_version: String,
    pub questions_sha256: String,
    pub expectations_version: String,
    pub expectations_sha256: String,
    pub corpus_sha256: String,
    pub sources: Vec<SourceIdentity>,
    pub as_of: String,
    pub generation_prompt_version: String,
    pub generation_prompt_sha256: String,
    pub rubric_version: String,
    pub rubric_sha256: String,
    pub model: String,
    pub client_version: String,
    pub binary_sha256: String,
    pub runtime_sha256: String,
    pub code_mode_host_sha256: String,
    pub generator_route: String,
    pub grader_route: String,
    pub embedding_model: String,
    pub embedding_files: Vec<ModelFile>,
    pub reproducibility_limitations: Vec<String>,
    pub limits: Limits,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExperimentRecord {
    pub schema_version: u32,
    pub run_id: String,
    pub case_id: String,
    pub family: String,
    pub split: String,
    pub kind: String,
    pub arm: Arm,
    pub execution_kind: ExecutionKind,
    pub execution_status: ExecutionStatus,
    pub question: String,
    pub manifest: Manifest,
    pub answer: Option<GroundedAnswer>,
    pub tool_calls: Vec<ToolCall>,
    pub offered_sections: Vec<Section>,
    pub elapsed_ms: u64,
    pub usage: Option<TokenUsage>,
    pub error: Option<String>,
    pub metrics: Option<serde_json::Value>,
    pub judgement: Option<serde_json::Value>,
}
