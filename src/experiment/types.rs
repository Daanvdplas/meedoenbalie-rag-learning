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
    #[serde(default)]
    pub budget: Option<super::budget::LedgerState>,
    #[serde(default)]
    pub embedding_revision: Option<String>,
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
    #[serde(default)]
    pub retrieval_steps: Vec<RetrievalStep>,
    #[serde(default)]
    pub trace_complete: bool,
    pub offered_sections: Vec<Section>,
    pub elapsed_ms: u64,
    pub usage: Option<TokenUsage>,
    pub error: Option<String>,
    #[serde(default)]
    pub diagnostic: Option<RuntimeDiagnostic>,
    pub metrics: Option<serde_json::Value>,
    pub judgement: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RankedSection {
    pub section: Section,
    pub score: f64,
    pub score_kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetrievalStep {
    pub origin: String,
    pub query: String,
    pub top_k: usize,
    pub hits: Vec<RankedSection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeDiagnostic {
    pub stage: String,
    pub category: String,
    pub exit_code: Option<i32>,
    pub event_types: Vec<String>,
    pub item_types: Vec<String>,
    pub provider_error_code: Option<String>,
    /// Fixed reason codes only; never model arguments, names or event text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_event_category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub json_event_lines_valid: Option<bool>,
}
impl RuntimeDiagnostic {
    pub fn validate(&self) -> Result<(), String> {
        if !matches!(
            self.stage.as_str(),
            "process_exit" | "isolated_event_validation" | "agent_event_validation" | "deadline"
        ) || !matches!(
            self.category.as_str(),
            "runtime_failed"
                | "provider_transport_failed"
                | "provider_rate_limited"
                | "provider_authentication_failed"
                | "provider_request_rejected"
                | "provider_server_failed"
                | "unexpected_tool"
                | "invalid_output"
                | "model_output_invalid"
                | "model_contract_violation"
                | "timeout"
        ) || self
            .exit_code
            .is_some_and(|code| !(0..=255).contains(&code))
            || self.event_types.len() > 10
            || self.item_types.len() > 10
            || self.event_types.iter().any(|t| {
                !matches!(
                    t.as_str(),
                    "thread.started"
                        | "turn.started"
                        | "turn.completed"
                        | "turn.failed"
                        | "error"
                        | "item.started"
                        | "item.updated"
                        | "item.completed"
                        | "unrecognized"
                )
            })
            || self.item_types.iter().any(|t| {
                !matches!(
                    t.as_str(),
                    "reasoning"
                        | "agent_message"
                        | "mcp_tool_call"
                        | "command_execution"
                        | "file_change"
                        | "web_search"
                        | "todo_list"
                        | "error"
                        | "unrecognized"
                )
            })
            || self.provider_error_code.as_ref().is_some_and(|c| {
                !matches!(
                    c.as_str(),
                    "rate_limit_exceeded"
                        | "insufficient_quota"
                        | "invalid_api_key"
                        | "invalid_request_error"
                        | "model_not_found"
                        | "server_error"
                        | "context_length_exceeded"
                        | "unsupported_parameter"
                        | "stream_disconnected"
                        | "websocket_error"
                )
            })
            || self.validation_reason.as_deref().is_some_and(|value| {
                !matches!(
                    value,
                    "missing_required_search"
                        | "failed_search"
                        | "invalid_search_arguments"
                        | "outside_contract_tool"
                        | "search_limit_exceeded"
                        | "invalid_model_answer"
                        | "invalid_event_stream"
                        | "source_or_parser_failure"
                )
            })
            || self.tool_event_category.as_deref().is_some_and(|value| {
                !matches!(
                    value,
                    "none"
                        | "search_document"
                        | "failed_search_document"
                        | "other_mcp_tool"
                        | "command_execution"
                        | "file_change"
                        | "web_search"
                        | "unclassified"
                )
            })
        {
            return Err("Invalid public runtime diagnostic".into());
        }
        if self.category == "model_contract_violation"
            && (self.stage != "agent_event_validation"
                || self.exit_code != Some(0)
                || self.json_event_lines_valid != Some(true)
                || self.provider_error_code.is_some()
                || self
                    .event_types
                    .iter()
                    .any(|kind| matches!(kind.as_str(), "error" | "turn.failed"))
                || self.item_types.iter().any(|kind| kind == "error")
                || !matches!(
                    self.validation_reason.as_deref(),
                    Some(
                        "missing_required_search"
                            | "invalid_search_arguments"
                            | "outside_contract_tool"
                            | "search_limit_exceeded"
                    )
                ))
        {
            return Err("Unproven model contract violation".into());
        }
        if self.category == "model_contract_violation" {
            let completed = self.event_types.iter().any(|kind| kind == "turn.completed");
            let agent_message = self.item_types.iter().any(|kind| kind == "agent_message");
            let mcp = self.item_types.iter().any(|kind| kind == "mcp_tool_call");
            let consistent = match (
                self.validation_reason.as_deref(),
                self.tool_event_category.as_deref(),
            ) {
                (Some("missing_required_search"), Some("none")) => {
                    completed
                        && agent_message
                        && !mcp
                        && self
                            .item_types
                            .iter()
                            .all(|kind| matches!(kind.as_str(), "reasoning" | "agent_message"))
                }
                (
                    Some("invalid_search_arguments" | "search_limit_exceeded"),
                    Some("search_document"),
                ) => mcp,
                (Some("outside_contract_tool"), Some("other_mcp_tool")) => mcp,
                (
                    Some("outside_contract_tool"),
                    Some(kind @ ("command_execution" | "file_change" | "web_search")),
                ) => self.item_types.iter().any(|item| item == kind),
                _ => false,
            };
            if !consistent {
                return Err("Inconsistent model action evidence".into());
            }
        }
        Ok(())
    }
}
