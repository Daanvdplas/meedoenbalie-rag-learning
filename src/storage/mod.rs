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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_profiles_report_optional_sqlite_support_without_silent_fallback() {
        for profile in [
            StorageProfile::DocumentsSections,
            StorageProfile::DocumentsActivities,
        ] {
            assert!(profile.supported());
            assert!(!profile.is_sqlite());
        }
        for profile in [
            StorageProfile::SqliteSections,
            StorageProfile::SqliteActivities,
        ] {
            assert!(profile.is_sqlite());
            assert_eq!(profile.supported(), cfg!(feature = "sqlite"));
        }
        assert_eq!(
            serde_json::to_string(&StorageProfile::DocumentsActivities).unwrap(),
            "\"documents_activities\""
        );
        assert_eq!(
            StorageProfile::SqliteActivities.structure(),
            StorageStructure::Activities
        );
    }

    #[test]
    fn embedding_cache_tracks_source_input_and_every_runtime_identity_field() {
        let section = crate::source::parse_sources(crate::DOCUMENTS)
            .unwrap()
            .remove(0)
            .sections
            .remove(0);
        let identity = crate::retrieval::EmbeddingIdentity {
            provider: "local-fastembed".into(),
            model: "mini".into(),
            sdk: "7.1.2".into(),
            dimensions: 3,
            encoding: "title-heading-text-v2".into(),
        };
        let original =
            CacheKey::new(&identity, &crate::retrieval::embedding_input(&section), 3).unwrap();
        let mut store = DocumentsStore::new(StorageStructure::Sections);
        store.put_embedding(&original, &[0.25, -0.0, 0.75]).unwrap();
        let mut changed_inputs = Vec::new();
        let mut title = section.clone();
        title.source_title.push_str(" gewijzigd");
        changed_inputs.push(title);
        let mut heading = section.clone();
        heading.heading.push_str(" gewijzigd");
        changed_inputs.push(heading);
        let mut text = section.clone();
        text.text.push_str(" gewijzigd");
        changed_inputs.push(text);
        for changed in changed_inputs {
            let key =
                CacheKey::new(&identity, &crate::retrieval::embedding_input(&changed), 3).unwrap();
            assert!(store.get_embedding(&key).unwrap().is_none());
        }
        let mut changed_identities = Vec::new();
        let mut provider = identity.clone();
        provider.provider = "different-local-provider".into();
        changed_identities.push(provider);
        let mut sdk = identity.clone();
        sdk.sdk = "next-sdk".into();
        changed_identities.push(sdk);
        let mut encoding = identity.clone();
        encoding.encoding = "different-encoding".into();
        changed_identities.push(encoding);
        for changed in changed_identities {
            let key =
                CacheKey::new(&changed, &crate::retrieval::embedding_input(&section), 3).unwrap();
            assert!(store.get_embedding(&key).unwrap().is_none());
        }
        let mut date = section.clone();
        date.source_date = "2026-11-02".into();
        let same_embedding_input =
            CacheKey::new(&identity, &crate::retrieval::embedding_input(&date), 3).unwrap();
        assert_eq!(
            original, same_embedding_input,
            "A source-date update changes provenance, not the title/heading/text embedding input"
        );
    }
}
