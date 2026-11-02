mod cache;
mod documents;
#[cfg(feature = "sqlite")]
mod sqlite;

pub use cache::{CacheKey, EmbeddingCache};
pub use documents::DocumentsStore;
#[cfg(feature = "sqlite")]
pub use sqlite::SqliteStore;

use crate::retrieval::{ActivityRecord, PreparedCorpus};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SyncStats {
    pub sections_total: usize,
    pub sections_inserted: usize,
    pub sections_updated: usize,
    pub sections_removed: usize,
    pub sections_unchanged: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LayoutStats {
    pub canonical_sections: usize,
    pub activity_records: usize,
    pub activity_section_links: usize,
}

pub trait CorpusStore {
    fn structure(&self) -> StorageStructure;
    fn sync(&mut self, corpus: &PreparedCorpus) -> Result<SyncStats, String>;
    fn load(&self) -> Result<PreparedCorpus, String>;
    fn activity(&self, source_id: &str) -> Result<Option<ActivityRecord>, String>;
    fn layout_stats(&self) -> Result<LayoutStats, String>;
    fn get_embedding(&self, key: &CacheKey) -> Result<Option<Vec<f32>>, String>;
    fn put_embedding(&mut self, key: &CacheKey, vector: &[f32]) -> Result<(), String>;
}

fn sync_stats(old: &[crate::Section], new: &[crate::Section]) -> Result<SyncStats, String> {
    use std::collections::BTreeMap;
    let old_by_key: BTreeMap<_, _> = old
        .iter()
        .map(|s| ((s.source_id.as_str(), s.id.as_str()), s))
        .collect();
    let mut stats = SyncStats {
        sections_total: new.len(),
        ..SyncStats::default()
    };
    for section in new {
        match old_by_key.get(&(section.source_id.as_str(), section.id.as_str())) {
            None => stats.sections_inserted += 1,
            Some(previous)
                if serde_json::to_vec(*previous).map_err(|e| e.to_string())?
                    == serde_json::to_vec(section).map_err(|e| e.to_string())? =>
            {
                stats.sections_unchanged += 1
            }
            Some(_) => stats.sections_updated += 1,
        }
    }
    stats.sections_removed = old.len() - stats.sections_updated - stats.sections_unchanged;
    Ok(stats)
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StorageStructure {
    Sections,
    Activities,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StorageProfile {
    DocumentsSections,
    DocumentsActivities,
    SqliteSections,
    SqliteActivities,
}

impl StorageProfile {
    pub fn structure(self) -> StorageStructure {
        match self {
            Self::DocumentsSections | Self::SqliteSections => StorageStructure::Sections,
            Self::DocumentsActivities | Self::SqliteActivities => StorageStructure::Activities,
        }
    }

    pub fn is_sqlite(self) -> bool {
        matches!(self, Self::SqliteSections | Self::SqliteActivities)
    }

    pub fn supported(self) -> bool {
        !self.is_sqlite() || cfg!(feature = "sqlite")
    }
}
