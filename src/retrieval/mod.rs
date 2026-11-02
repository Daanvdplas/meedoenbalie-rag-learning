//! Storage-independent retrieval over canonical, source-backed sections.
mod config;
mod engine;
mod metadata;

pub use config::*;
pub use engine::*;
pub use metadata::*;

use crate::Section;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EmbeddingIdentity {
    pub provider: String,
    pub model: String,
    pub sdk: String,
    pub dimensions: usize,
    pub encoding: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CorpusIdentity {
    pub source_sha256: String,
    pub embedding_inputs_sha256: String,
    pub vectors_sha256: Option<String>,
    pub embedding: Option<EmbeddingIdentity>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreparedCorpus {
    pub sections: Vec<Section>,
    pub vectors: Option<Vec<Vec<f32>>>,
    pub activities: Vec<ActivityRecord>,
    pub identity: CorpusIdentity,
}

impl PreparedCorpus {
    pub fn new(
        _sections: Vec<Section>,
        _vectors: Option<Vec<Vec<f32>>>,
        _embedding: Option<EmbeddingIdentity>,
    ) -> Result<Self, String> {
        Err("Corpus preparation is not implemented yet".into())
    }

    pub fn validate(&self) -> Result<(), String> {
        Err("Corpus validation is not implemented yet".into())
    }
}

pub fn embedding_input(section: &Section) -> String {
    format!("{}\n{}\n{}", section.source_title, section.heading, section.text)
}
