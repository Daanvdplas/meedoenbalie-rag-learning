use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};

use super::{CacheKey, CorpusStore, LayoutStats, StorageStructure, SyncStats};
use crate::{
    Section,
    retrieval::{ActivityRecord, CorpusIdentity, PreparedCorpus, embedding_input},
};

mod activities;

const APPLICATION_ID: i64 = 0x434f_4131;
pub const SQLITE_SCHEMA_VERSION: i64 = 1;

pub struct SqliteStore {
    connection: Connection,
    structure: StorageStructure,
}

impl SqliteStore {
    pub fn open(path: impl AsRef<Path>, structure: StorageStructure) -> Result<Self, String> {
        let mut connection = Connection::open(path).map_err(|e| e.to_string())?;
        connection
            .busy_timeout(std::time::Duration::from_secs(5))
            .map_err(|e| e.to_string())?;
        connection
            .pragma_update(None, "foreign_keys", true)
            .map_err(|e| e.to_string())?;
        let application_id: i64 = connection
            .pragma_query_value(None, "application_id", |row| row.get(0))
            .map_err(|e| e.to_string())?;
        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .map_err(|e| e.to_string())?;
        let structure_json = serde_json::to_string(&structure).map_err(|e| e.to_string())?;
        if application_id == 0 && version == 0 {
            let objects: i64 = connection
                .query_row(
                    "SELECT count(*) FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'",
                    [],
                    |row| row.get(0),
                )
                .map_err(|e| e.to_string())?;
            if objects != 0 {
                return Err("Refusing to change a database owned by another application".into());
            }
            let transaction = connection.transaction().map_err(|e| e.to_string())?;
            transaction
                .execute_batch(include_str!("schema.sql"))
                .map_err(|e| e.to_string())?;
            if structure == StorageStructure::Activities {
                transaction
                    .execute_batch(include_str!("activities-schema.sql"))
                    .map_err(|e| e.to_string())?;
            }
            transaction
                .execute(
                    "INSERT INTO store_meta(singleton, structure) VALUES(1, ?1)",
                    [&structure_json],
                )
                .map_err(|e| e.to_string())?;
            transaction
                .pragma_update(None, "application_id", APPLICATION_ID)
                .map_err(|e| e.to_string())?;
            transaction
                .pragma_update(None, "user_version", SQLITE_SCHEMA_VERSION)
                .map_err(|e| e.to_string())?;
            transaction.commit().map_err(|e| e.to_string())?;
        } else if application_id != APPLICATION_ID || version != SQLITE_SCHEMA_VERSION {
            return Err(
                "Unsupported SQLite corpus schema; rebuild the derived index in a new file".into(),
            );
        }
        let actual_structure: String = connection
            .query_row(
                "SELECT structure FROM store_meta WHERE singleton = 1",
                [],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;
        if actual_structure != structure_json {
            return Err(
                "SQLite file uses a different storage structure; use a separate index file".into(),
            );
        }
        Ok(Self {
            connection,
            structure,
        })
    }

    fn identity(&self) -> Result<Option<CorpusIdentity>, String> {
        let json: Option<String> = self
            .connection
            .query_row(
                "SELECT identity_json FROM corpus_state WHERE singleton = 1",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        json.map(|json| serde_json::from_str(&json).map_err(|e| e.to_string()))
            .transpose()
    }
}

impl CorpusStore for SqliteStore {
    fn structure(&self) -> StorageStructure {
        self.structure
    }

    fn sync(&mut self, corpus: &PreparedCorpus) -> Result<SyncStats, String> {
        corpus.validate()?;
        let previous = if self.identity()?.is_some() {
            Some(self.load()?)
        } else {
            if row_count(&self.connection, "sections")? != 0 {
                return Err("SQLite contains source rows without a valid corpus identity".into());
            }
            None
        };
        let stats = super::sync_stats(
            previous
                .as_ref()
                .map(|old| old.sections.as_slice())
                .unwrap_or_default(),
            &corpus.sections,
        )?;
        if previous
            .as_ref()
            .is_some_and(|old| old.identity == corpus.identity)
        {
            return Ok(stats);
        }
        let transaction = self.connection.transaction().map_err(|e| e.to_string())?;
        let old_activities: BTreeMap<_, _> = previous
            .as_ref()
            .map(|old| {
                old.activities
                    .iter()
                    .map(|activity| (activity.id.as_str(), activity))
                    .collect()
            })
            .unwrap_or_default();
        let new_activities: BTreeMap<_, _> = corpus
            .activities
            .iter()
            .map(|activity| (activity.id.as_str(), activity))
            .collect();
        if self.structure == StorageStructure::Activities {
            for (id, old) in &old_activities {
                if new_activities.get(id).is_none_or(|new| *new != *old) {
                    transaction
                        .execute("DELETE FROM activities WHERE activity_id = ?1", [id])
                        .map_err(|e| e.to_string())?;
                }
            }
        }
        let new_keys: BTreeSet<_> = corpus
            .sections
            .iter()
            .map(|section| (section.source_id.as_str(), section.id.as_str()))
            .collect();
        for section in previous
            .as_ref()
            .map(|old| old.sections.as_slice())
            .unwrap_or_default()
        {
            if !new_keys.contains(&(section.source_id.as_str(), section.id.as_str())) {
                transaction
                    .execute(
                        "DELETE FROM sections WHERE source_id = ?1 AND section_id = ?2",
                        params![section.source_id, section.id],
                    )
                    .map_err(|e| e.to_string())?;
            }
        }
        for (position, section) in corpus.sections.iter().enumerate() {
            transaction.execute(
                "INSERT INTO sections(source_id, section_id, position, source_date, source_title, heading, text) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(source_id, section_id) DO UPDATE SET position = excluded.position, source_date = excluded.source_date, source_title = excluded.source_title, heading = excluded.heading, text = excluded.text
                 WHERE position IS NOT excluded.position OR source_date IS NOT excluded.source_date OR source_title IS NOT excluded.source_title OR heading IS NOT excluded.heading OR text IS NOT excluded.text",
                params![section.source_id, section.id, position as i64, section.source_date, section.source_title, section.heading, section.text],
            ).map_err(|e| e.to_string())?;
            if let Some(vectors) = &corpus.vectors {
                let bytes: Vec<u8> = vectors[position]
                    .iter()
                    .flat_map(|value| value.to_le_bytes())
                    .collect();
                transaction.execute(
                    "INSERT INTO section_vectors(source_id, section_id, vector) VALUES(?1, ?2, ?3)
                     ON CONFLICT(source_id, section_id) DO UPDATE SET vector = excluded.vector WHERE vector IS NOT excluded.vector",
                    params![section.source_id, section.id, bytes],
                ).map_err(|e| e.to_string())?;
            }
        }
        if corpus.vectors.is_none() {
            transaction
                .execute("DELETE FROM section_vectors", [])
                .map_err(|e| e.to_string())?;
        }
        if self.structure == StorageStructure::Activities {
            for (id, activity) in &new_activities {
                if old_activities.get(id).is_none_or(|old| *old != *activity) {
                    activities::write(&transaction, activity)?;
                }
            }
        }
        let json = serde_json::to_string(&corpus.identity).map_err(|e| e.to_string())?;
        transaction.execute("INSERT INTO corpus_state(singleton, identity_json) VALUES(1, ?1) ON CONFLICT(singleton) DO UPDATE SET identity_json = excluded.identity_json", [json]).map_err(|e| e.to_string())?;
        transaction.commit().map_err(|e| e.to_string())?;
        Ok(stats)
    }

    fn load(&self) -> Result<PreparedCorpus, String> {
        let identity = self.identity()?.ok_or("SQLite corpus not stored yet")?;
        let sections = read_sections(&self.connection)?;
        let mut statement = self
            .connection
            .prepare("SELECT source_id, section_id, vector FROM section_vectors")
            .map_err(|e| e.to_string())?;
        let vector_rows: BTreeMap<(String, String), Vec<u8>> = statement
            .query_map([], |row| Ok(((row.get(0)?, row.get(1)?), row.get(2)?)))
            .map_err(|e| e.to_string())?
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?;
        let vectors = if let Some(embedding) = &identity.embedding {
            if vector_rows.len() != sections.len() {
                return Err("Stored vector count does not match canonical sections".into());
            }
            Some(
                sections
                    .iter()
                    .map(|section| {
                        let key = CacheKey::new(
                            embedding,
                            &embedding_input(section),
                            embedding.dimensions,
                        )?;
                        let bytes = vector_rows
                            .get(&(section.source_id.clone(), section.id.clone()))
                            .ok_or("Missing vector for canonical section")?;
                        decode_vector(&key, bytes)
                    })
                    .collect::<Result<Vec<_>, String>>()?,
            )
        } else {
            if !vector_rows.is_empty() {
                return Err("Keyword-only corpus unexpectedly contains stored vectors".into());
            }
            None
        };
        let corpus = PreparedCorpus::new(sections, vectors, identity.embedding.clone())?;
        if corpus.identity != identity {
            return Err("SQLite corpus identity does not match its stored evidence/vectors".into());
        }
        if self.structure == StorageStructure::Activities
            && activities::read(&self.connection)? != corpus.activities
        {
            return Err("SQLite activity fields do not match source-backed facts".into());
        }
        Ok(corpus)
    }

    fn activity(&self, source_id: &str) -> Result<Option<ActivityRecord>, String> {
        Ok(self
            .load()?
            .activities
            .into_iter()
            .find(|activity| activity.id == source_id))
    }

    fn layout_stats(&self) -> Result<LayoutStats, String> {
        self.load()?;
        Ok(LayoutStats {
            canonical_sections: row_count(&self.connection, "sections")?,
            activity_records: if self.structure == StorageStructure::Activities {
                row_count(&self.connection, "activities")?
            } else {
                0
            },
            activity_section_links: if self.structure == StorageStructure::Activities {
                row_count(&self.connection, "activity_section_links")?
            } else {
                0
            },
        })
    }

    fn get_embedding(&self, key: &CacheKey) -> Result<Option<Vec<f32>>, String> {
        let dimensions =
            i64::try_from(key.dimensions).map_err(|_| "Embedding dimension overflow")?;
        let bytes: Option<Vec<u8>> = self.connection.query_row(
            "SELECT vector FROM embedding_cache WHERE identity_sha256 = ?1 AND input_sha256 = ?2 AND dimensions = ?3",
            params![key.identity_sha256, key.input_sha256, dimensions],
            |row| row.get(0),
        ).optional().map_err(|e| e.to_string())?;
        bytes.map(|bytes| decode_vector(key, &bytes)).transpose()
    }

    fn put_embedding(&mut self, key: &CacheKey, vector: &[f32]) -> Result<(), String> {
        super::cache::validate_vector(key, vector)?;
        let dimensions =
            i64::try_from(key.dimensions).map_err(|_| "Embedding dimension overflow")?;
        let bytes: Vec<u8> = vector
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect();
        self.connection.execute(
            "INSERT INTO embedding_cache(identity_sha256, input_sha256, dimensions, vector) VALUES(?1, ?2, ?3, ?4)
             ON CONFLICT(identity_sha256, input_sha256, dimensions) DO UPDATE SET vector = excluded.vector",
            params![key.identity_sha256, key.input_sha256, dimensions, bytes],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }
}

fn row_count(connection: &Connection, table: &str) -> Result<usize, String> {
    let count: i64 = connection
        .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .map_err(|e| e.to_string())?;
    usize::try_from(count).map_err(|_| "Invalid stored row count".into())
}

fn read_sections(connection: &Connection) -> Result<Vec<Section>, String> {
    let mut statement = connection.prepare("SELECT position, source_id, section_id, source_date, source_title, heading, text FROM sections ORDER BY position").map_err(|e| e.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                Section {
                    source_id: row.get(1)?,
                    id: row.get(2)?,
                    source_date: row.get(3)?,
                    source_title: row.get(4)?,
                    heading: row.get(5)?,
                    text: row.get(6)?,
                },
            ))
        })
        .map_err(|e| e.to_string())?;
    let mut sections = Vec::new();
    for row in rows {
        let (position, section) = row.map_err(|e| e.to_string())?;
        if position != sections.len() as i64 {
            return Err("Invalid canonical section ordering in SQLite".into());
        }
        sections.push(section);
    }
    Ok(sections)
}

fn decode_vector(key: &CacheKey, bytes: &[u8]) -> Result<Vec<f32>, String> {
    if key.dimensions.checked_mul(4) != Some(bytes.len()) {
        return Err("Corrupt cached embedding byte length".into());
    }
    let vector: Vec<_> = bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect();
    super::cache::validate_vector(key, &vector)?;
    Ok(vector)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

    struct TempDb(PathBuf);

    impl TempDb {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "coa-storage-{}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                NEXT_FILE.fetch_add(1, Ordering::Relaxed),
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> PathBuf {
            self.0.join("corpus.sqlite")
        }
    }

    impl Drop for TempDb {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn key(model: &str, input: &str, dimensions: usize) -> CacheKey {
        CacheKey::new(
            &serde_json::json!({"provider":"local-fastembed","model":model,"sdk":"7.1.2","encoding":"title-heading-text-v2"}),
            input,
            dimensions,
        ).unwrap()
    }

    fn corpus() -> PreparedCorpus {
        let sections = crate::source::parse_sources(crate::DOCUMENTS)
            .unwrap()
            .into_iter()
            .flat_map(|s| s.sections)
            .collect::<Vec<_>>();
        let vectors = sections
            .iter()
            .enumerate()
            .map(|(position, _)| vec![f32::from_bits(0x3eaa_aaab), -0.0, position as f32 + 0.75])
            .collect();
        PreparedCorpus::new(
            sections,
            Some(vectors),
            Some(crate::retrieval::EmbeddingIdentity {
                provider: "local-fastembed".into(),
                model: "test-model".into(),
                sdk: "7.1.2".into(),
                dimensions: 3,
                encoding: "title-heading-text-v2".into(),
            }),
        )
        .unwrap()
    }

    #[test]
    fn sqlite_profiles_roundtrip_canonical_sections_and_real_normalized_activities() {
        let expected = corpus();
        for structure in [StorageStructure::Sections, StorageStructure::Activities] {
            let file = TempDb::new();
            {
                let mut store = SqliteStore::open(file.path(), structure).unwrap();
                assert!(store.load().is_err());
                assert_eq!(store.sync(&expected).unwrap().sections_inserted, 30);
                assert_eq!(store.sync(&expected).unwrap().sections_unchanged, 30);
                let stats = store.layout_stats().unwrap();
                assert_eq!(stats.canonical_sections, 30);
                assert_eq!(
                    stats.activity_records,
                    if structure == StorageStructure::Activities {
                        5
                    } else {
                        0
                    }
                );
                assert_eq!(
                    stats.activity_section_links,
                    if structure == StorageStructure::Activities {
                        30
                    } else {
                        0
                    }
                );
                let tables: Vec<String> = store
                    .connection
                    .prepare("SELECT name FROM sqlite_schema WHERE type = 'table'")
                    .unwrap()
                    .query_map([], |row| row.get(0))
                    .unwrap()
                    .collect::<Result<_, _>>()
                    .unwrap();
                assert_eq!(
                    tables.iter().any(|table| table == "activities"),
                    structure == StorageStructure::Activities
                );
                let activity = store.activity("ACT-FIETS-002").unwrap().unwrap();
                assert!(activity.schedule.is_some());
                assert!(activity.environment.is_some());
                assert!(store.activity("missing").unwrap().is_none());
            }
            let store = SqliteStore::open(file.path(), structure).unwrap();
            let actual = store.load().unwrap();
            assert_eq!(actual.identity, expected.identity);
            assert_eq!(actual.activities, expected.activities);
            assert_eq!(
                serde_json::to_vec(&actual.sections).unwrap(),
                serde_json::to_vec(&expected.sections).unwrap()
            );
            assert_eq!(
                actual
                    .vectors
                    .unwrap()
                    .iter()
                    .flatten()
                    .map(|v| v.to_bits())
                    .collect::<Vec<_>>(),
                expected
                    .vectors
                    .as_ref()
                    .unwrap()
                    .iter()
                    .flatten()
                    .map(|v| v.to_bits())
                    .collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn sqlite_sync_is_atomic_and_counts_only_changed_source_records() {
        let file = TempDb::new();
        let mut store = SqliteStore::open(file.path(), StorageStructure::Activities).unwrap();
        let original = corpus();
        store.sync(&original).unwrap();
        let mut sections = original.sections.clone();
        sections[0].heading = "Updated heading".into();
        sections[1].heading = "Rejected heading".into();
        let changed = PreparedCorpus::new(
            sections,
            original.vectors.clone(),
            original.identity.embedding.clone(),
        )
        .unwrap();
        store.connection.execute_batch("CREATE TRIGGER injected_failure BEFORE UPDATE ON sections WHEN NEW.heading = 'Rejected heading' BEGIN SELECT RAISE(ABORT, 'injected storage failure'); END;").unwrap();
        assert!(store.sync(&changed).is_err());
        assert_eq!(store.load().unwrap().identity, original.identity);
        store
            .connection
            .execute_batch("DROP TRIGGER injected_failure;")
            .unwrap();
        let stats = store.sync(&changed).unwrap();
        assert_eq!(
            (
                stats.sections_inserted,
                stats.sections_updated,
                stats.sections_removed,
                stats.sections_unchanged
            ),
            (0, 2, 0, 28)
        );
        assert_eq!(store.load().unwrap().identity, changed.identity);
        let mut invalid = changed.clone();
        invalid.sections[0].text.push_str(" unverified mutation");
        assert!(store.sync(&invalid).is_err());
        assert_eq!(store.load().unwrap().identity, changed.identity);
    }

    #[test]
    fn sqlite_detects_modified_evidence_and_activity_facts_instead_of_silent_fallback() {
        for structure in [StorageStructure::Sections, StorageStructure::Activities] {
            let file = TempDb::new();
            let mut store = SqliteStore::open(file.path(), structure).unwrap();
            let original = corpus();
            store.sync(&original).unwrap();
            store
                .connection
                .execute(
                    "UPDATE sections SET text = text || ' tampered' WHERE position = 0",
                    [],
                )
                .unwrap();
            assert!(store.load().is_err());
            assert!(store.sync(&original).is_err());
        }
        let file = TempDb::new();
        let mut store = SqliteStore::open(file.path(), StorageStructure::Activities).unwrap();
        store.sync(&corpus()).unwrap();
        store.connection.execute("UPDATE activities SET title = 'Invented activity' WHERE activity_id = 'ACT-FIETS-002'", []).unwrap();
        assert!(store.load().is_err());
    }

    #[test]
    fn sqlite_keyword_refresh_removes_old_facts_and_keeps_unknowns_unknown() {
        let file = TempDb::new();
        let mut store = SqliteStore::open(file.path(), StorageStructure::Activities).unwrap();
        let original = corpus();
        store.sync(&original).unwrap();
        let mut sections = original.sections.clone();
        for section in &mut sections {
            if section.source_id == "ACT-FIETS-002" && section.id == "S2" {
                section.text = "Er is nog geen rooster bekend.".into();
            }
            if section.source_id == "ACT-FIETS-002" && section.id == "S3" {
                section.text = "De locatie moet nog worden gecontroleerd.".into();
            }
        }
        let keyword_only = PreparedCorpus::new(sections, None, None).unwrap();
        assert_eq!(store.sync(&keyword_only).unwrap().sections_updated, 2);
        let loaded = store.load().unwrap();
        assert!(loaded.vectors.is_none());
        assert!(loaded.identity.embedding.is_none());
        let activity = store.activity("ACT-FIETS-002").unwrap().unwrap();
        assert!(activity.schedule.is_none());
        assert!(activity.environment.is_none());
        let facts: i64 = store
            .connection
            .query_row(
                "SELECT count(*) FROM activity_fact_evidence WHERE activity_id = 'ACT-FIETS-002'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(facts, 0);
        assert_eq!(row_count(&store.connection, "section_vectors").unwrap(), 0);
    }

    #[test]
    fn sqlite_refresh_preserves_order_and_vectors_and_removes_deleted_activities() {
        let file = TempDb::new();
        let mut store = SqliteStore::open(file.path(), StorageStructure::Activities).unwrap();
        let original = corpus();
        store.sync(&original).unwrap();
        let mut sections = original.sections.clone();
        let mut vectors = original.vectors.clone().unwrap();
        sections.rotate_left(6);
        vectors.rotate_left(6);
        let reordered = PreparedCorpus::new(
            sections.clone(),
            Some(vectors.clone()),
            original.identity.embedding.clone(),
        )
        .unwrap();
        assert_eq!(store.sync(&reordered).unwrap().sections_unchanged, 30);
        assert_eq!(store.load().unwrap().identity, reordered.identity);
        sections.truncate(24);
        vectors.truncate(24);
        let smaller =
            PreparedCorpus::new(sections, Some(vectors), original.identity.embedding.clone())
                .unwrap();
        let stats = store.sync(&smaller).unwrap();
        assert_eq!((stats.sections_removed, stats.sections_unchanged), (6, 24));
        assert_eq!(store.layout_stats().unwrap().activity_records, 4);
        assert!(store.activity("ACT-TUIN-001").unwrap().is_none());
        assert_eq!(store.load().unwrap().identity, smaller.identity);
    }

    #[test]
    fn sqlite_cache_survives_reopen_and_preserves_exact_float_bits() {
        let file = TempDb::new();
        let original = key("mini", "Fietsen\nWanneer\nDinsdag 14–16", 3);
        let vector = [0.25, -0.0, 0.75];
        {
            let mut store = SqliteStore::open(file.path(), StorageStructure::Sections).unwrap();
            assert!(store.get_embedding(&original).unwrap().is_none());
            store.put_embedding(&original, &vector).unwrap();
        }
        let store = SqliteStore::open(file.path(), StorageStructure::Sections).unwrap();
        let actual = store.get_embedding(&original).unwrap().unwrap();
        assert_eq!(
            actual.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
            vector.iter().map(|v| v.to_bits()).collect::<Vec<_>>()
        );
        for changed in [
            key("other", "Fietsen\nWanneer\nDinsdag 14–16", 3),
            key("mini", "Fietsen\nWanneer\nWoensdag 14–16", 3),
            key("mini", "Fietsen\nWanneer\nDinsdag 14–16", 2),
        ] {
            assert!(store.get_embedding(&changed).unwrap().is_none());
        }
    }

    #[test]
    fn sqlite_refuses_foreign_files_and_layout_changes_without_destroying_them() {
        let file = TempDb::new();
        std::fs::write(file.path(), b"original source text").unwrap();
        assert!(SqliteStore::open(file.path(), StorageStructure::Sections).is_err());
        assert_eq!(std::fs::read(file.path()).unwrap(), b"original source text");
        std::fs::remove_file(file.path()).unwrap();
        drop(SqliteStore::open(file.path(), StorageStructure::Sections).unwrap());
        assert!(SqliteStore::open(file.path(), StorageStructure::Activities).is_err());
        let external = Connection::open(file.path()).unwrap();
        external.pragma_update(None, "user_version", 999).unwrap();
        assert!(SqliteStore::open(file.path(), StorageStructure::Sections).is_err());
    }

    #[test]
    fn sqlite_cache_reports_corruption_and_rejects_invalid_writes() {
        let file = TempDb::new();
        let expected = key("mini", "hello", 3);
        let mut store = SqliteStore::open(file.path(), StorageStructure::Sections).unwrap();
        assert!(
            store
                .put_embedding(&expected, &[f32::NAN, 1.0, 2.0])
                .is_err()
        );
        assert!(store.put_embedding(&expected, &[0.0; 3]).is_err());
        store.put_embedding(&expected, &[1.0, 2.0, 3.0]).unwrap();
        store
            .connection
            .execute("UPDATE embedding_cache SET vector = ?1", [&[0_u8; 2][..]])
            .unwrap();
        assert!(store.get_embedding(&expected).is_err());
    }
}
