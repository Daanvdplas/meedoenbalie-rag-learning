use crate::{
    Section,
    retrieval::{ActivityRecord, CorpusIdentity, PreparedCorpus},
};

use super::{CacheKey, CorpusStore, EmbeddingCache, LayoutStats, StorageStructure, SyncStats};

pub struct DocumentsStore {
    structure: StorageStructure,
    cache: EmbeddingCache,
    stored: Option<StoredCorpus>,
}

struct StoredCorpus {
    data: StoredData,
    vectors: Option<Vec<Vec<f32>>>,
    identity: CorpusIdentity,
}

enum StoredData {
    Sections(Vec<Section>),
    Activities(Vec<StoredActivity>),
}

struct StoredActivity {
    record: ActivityRecord,
    sections: Vec<(usize, Section)>,
}

impl StoredCorpus {
    fn sections(&self) -> Vec<Section> {
        match &self.data {
            StoredData::Sections(sections) => sections.clone(),
            StoredData::Activities(activities) => {
                let mut sections: Vec<_> =
                    activities.iter().flat_map(|a| a.sections.clone()).collect();
                sections.sort_by_key(|(position, _)| *position);
                sections.into_iter().map(|(_, section)| section).collect()
            }
        }
    }
}

impl DocumentsStore {
    pub fn new(structure: StorageStructure) -> Self {
        Self {
            structure,
            cache: EmbeddingCache::default(),
            stored: None,
        }
    }
}

impl CorpusStore for DocumentsStore {
    fn structure(&self) -> StorageStructure {
        self.structure
    }
    fn sync(&mut self, corpus: &PreparedCorpus) -> Result<SyncStats, String> {
        corpus.validate()?;
        let previous = self
            .stored
            .as_ref()
            .map(StoredCorpus::sections)
            .unwrap_or_default();
        let stats = super::sync_stats(&previous, &corpus.sections)?;
        let data = match self.structure {
            StorageStructure::Sections => StoredData::Sections(corpus.sections.clone()),
            StorageStructure::Activities => StoredData::Activities(
                corpus
                    .activities
                    .iter()
                    .map(|record| StoredActivity {
                        record: record.clone(),
                        sections: corpus
                            .sections
                            .iter()
                            .enumerate()
                            .filter(|(_, section)| section.source_id == record.id)
                            .map(|(position, section)| (position, section.clone()))
                            .collect(),
                    })
                    .collect(),
            ),
        };
        self.stored = Some(StoredCorpus {
            data,
            vectors: corpus.vectors.clone(),
            identity: corpus.identity.clone(),
        });
        Ok(stats)
    }
    fn load(&self) -> Result<PreparedCorpus, String> {
        let stored = self.stored.as_ref().ok_or("Corpus not stored yet")?;
        let corpus = PreparedCorpus::new(
            stored.sections(),
            stored.vectors.clone(),
            stored.identity.embedding.clone(),
        )?;
        if corpus.identity != stored.identity {
            return Err("Stored document identity does not match its canonical evidence".into());
        }
        if let StoredData::Activities(activities) = &stored.data {
            let records = activities
                .iter()
                .map(|activity| activity.record.clone())
                .collect::<Vec<_>>();
            if records != corpus.activities {
                return Err("Stored activity records do not match source-backed facts".into());
            }
        }
        Ok(corpus)
    }
    fn activity(&self, source_id: &str) -> Result<Option<ActivityRecord>, String> {
        Ok(self
            .load()?
            .activities
            .into_iter()
            .find(|a| a.id == source_id))
    }
    fn layout_stats(&self) -> Result<LayoutStats, String> {
        let stored = self.stored.as_ref().ok_or("Corpus not stored yet")?;
        let canonical_sections = stored.sections().len();
        Ok(match &stored.data {
            StoredData::Sections(_) => LayoutStats {
                canonical_sections,
                activity_records: 0,
                activity_section_links: 0,
            },
            StoredData::Activities(activities) => LayoutStats {
                canonical_sections,
                activity_records: activities.len(),
                activity_section_links: canonical_sections,
            },
        })
    }
    fn get_embedding(&self, key: &CacheKey) -> Result<Option<Vec<f32>>, String> {
        self.cache.get(key)
    }
    fn put_embedding(&mut self, key: &CacheKey, vector: &[f32]) -> Result<(), String> {
        self.cache.put(key, vector)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Section, retrieval::EmbeddingIdentity};

    fn corpus() -> PreparedCorpus {
        let sections = crate::source::parse_sources(crate::DOCUMENTS)
            .unwrap()
            .into_iter()
            .flat_map(|s| s.sections)
            .collect::<Vec<_>>();
        let vectors = sections.iter().map(|_| vec![0.25, -0.0, 0.75]).collect();
        PreparedCorpus::new(
            sections,
            Some(vectors),
            Some(EmbeddingIdentity {
                provider: "local-fastembed".into(),
                model: "test-model".into(),
                sdk: "7.1.2".into(),
                dimensions: 3,
                encoding: "title-heading-text-v2".into(),
            }),
        )
        .unwrap()
    }

    fn section_bytes(sections: &[Section]) -> Vec<u8> {
        serde_json::to_vec(sections).unwrap()
    }

    #[test]
    fn document_profiles_have_distinct_layouts_and_identical_source_evidence() {
        let corpus = corpus();
        for structure in [StorageStructure::Sections, StorageStructure::Activities] {
            let mut store = DocumentsStore::new(structure);
            assert!(store.load().is_err());
            let stats = store.sync(&corpus).unwrap();
            assert_eq!(stats.sections_inserted, 30);
            let loaded = store.load().unwrap();
            assert_eq!(
                section_bytes(&loaded.sections),
                section_bytes(&corpus.sections)
            );
            assert_eq!(loaded.identity, corpus.identity);
            assert_eq!(loaded.activities, corpus.activities);
            let layout = store.layout_stats().unwrap();
            assert_eq!(layout.canonical_sections, 30);
            assert_eq!(
                layout.activity_records,
                if structure == StorageStructure::Activities {
                    5
                } else {
                    0
                }
            );
            assert_eq!(
                layout.activity_section_links,
                if structure == StorageStructure::Activities {
                    30
                } else {
                    0
                }
            );
            let activity = store.activity("ACT-FIETS-002").unwrap().unwrap();
            assert!(activity.schedule.is_some());
            assert!(activity.environment.is_some());
            assert!(store.activity("missing").unwrap().is_none());
            assert_eq!(store.sync(&corpus).unwrap().sections_unchanged, 30);
        }
    }

    #[test]
    fn document_sync_rejects_bad_identity_without_replacing_valid_corpus() {
        let original = corpus();
        let mut store = DocumentsStore::new(StorageStructure::Activities);
        store.sync(&original).unwrap();
        let mut invalid = original.clone();
        invalid.sections[0].text.push_str(" fabricated change");
        assert!(store.sync(&invalid).is_err());
        assert_eq!(store.load().unwrap().identity, original.identity);
        assert_eq!(
            section_bytes(&store.load().unwrap().sections),
            section_bytes(&original.sections)
        );
    }

    #[test]
    fn document_refresh_counts_changes_and_preserves_unmodified_embeddings() {
        let original = corpus();
        let mut store = DocumentsStore::new(StorageStructure::Activities);
        store.sync(&original).unwrap();
        let mut sections = original.sections.clone();
        sections[0].heading = "Nieuwe titel".into();
        sections.remove(1);
        let mut vectors = original.vectors.clone().unwrap();
        vectors.remove(1);
        let changed =
            PreparedCorpus::new(sections, Some(vectors), original.identity.embedding.clone())
                .unwrap();
        let stats = store.sync(&changed).unwrap();
        assert_eq!(
            (
                stats.sections_inserted,
                stats.sections_updated,
                stats.sections_removed,
                stats.sections_unchanged
            ),
            (0, 1, 1, 28)
        );
        assert_eq!(store.load().unwrap().identity, changed.identity);
    }
}
