//! Storage-independent retrieval over canonical, source-backed sections.
mod config;
mod engine;
mod metadata;
#[cfg(test)]
mod tests;

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
        sections: Vec<Section>,
        vectors: Option<Vec<Vec<f32>>>,
        embedding: Option<EmbeddingIdentity>,
    ) -> Result<Self, String> {
        use std::collections::BTreeSet;

        if sections.is_empty() {
            return Err("Retrieval corpus is empty".into());
        }
        let mut keys = BTreeSet::new();
        for section in &sections {
            if section.source_id.trim().is_empty()
                || section.id.trim().is_empty()
                || section.source_title.trim().is_empty()
                || section.heading.trim().is_empty()
                || section.text.trim().is_empty()
                || !keys.insert(SectionKey::from(section))
            {
                return Err("Missing or duplicate canonical section identity/text".into());
            }
            crate::source::validate_date(&section.source_date)?;
        }
        match (&vectors, &embedding) {
            (Some(vectors), Some(identity)) => {
                if vectors.len() != sections.len()
                    || identity.dimensions == 0
                    || [
                        &identity.provider,
                        &identity.model,
                        &identity.sdk,
                        &identity.encoding,
                    ]
                    .iter()
                    .any(|value| value.trim().is_empty())
                    || vectors
                        .iter()
                        .any(|vector| vector.len() != identity.dimensions)
                {
                    return Err(
                        "Embedding identity, count or dimensions do not match corpus".into(),
                    );
                }
                for vector in vectors {
                    crate::ranking::cosine(vector, vector)?;
                }
            }
            (None, None) => {}
            _ => return Err("Vectors and embedding identity must be supplied together".into()),
        }
        let activities = metadata::activity_records(&sections)?;
        let inputs: Vec<_> = sections.iter().map(embedding_input).collect();
        let identity = CorpusIdentity {
            source_sha256: crate::manifest::hash(
                &serde_json::to_vec(&sections).map_err(|error| error.to_string())?,
            ),
            embedding_inputs_sha256: crate::manifest::hash(
                &serde_json::to_vec(&inputs).map_err(|error| error.to_string())?,
            ),
            vectors_sha256: vectors.as_ref().map(|vectors| {
                crate::manifest::hash(
                    &vectors
                        .iter()
                        .flatten()
                        .flat_map(|value| value.to_le_bytes())
                        .collect::<Vec<_>>(),
                )
            }),
            embedding,
        };
        Ok(Self {
            sections,
            vectors,
            activities,
            identity,
        })
    }

    pub fn validate(&self) -> Result<(), String> {
        let reconstructed = Self::new(
            self.sections.clone(),
            self.vectors.clone(),
            self.identity.embedding.clone(),
        )?;
        if self.identity != reconstructed.identity || self.activities != reconstructed.activities {
            return Err(
                "Corpus identity or source-backed activity facts changed after preparation".into(),
            );
        }
        Ok(())
    }
}

pub fn embedding_input(section: &Section) -> String {
    format!(
        "{}\n{}\n{}",
        section.source_title, section.heading, section.text
    )
}
