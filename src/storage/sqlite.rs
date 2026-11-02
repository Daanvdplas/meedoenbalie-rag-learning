use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};

use super::{CacheKey, CorpusStore, LayoutStats, StorageStructure, SyncStats};
use crate::retrieval::{ActivityRecord, PreparedCorpus};

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
                .execute_batch(
                    "CREATE TABLE store_meta (
                    singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
                    structure TEXT NOT NULL
                );
                CREATE TABLE embedding_cache (
                    identity_sha256 TEXT NOT NULL,
                    input_sha256 TEXT NOT NULL,
                    dimensions INTEGER NOT NULL CHECK(dimensions > 0),
                    vector BLOB NOT NULL,
                    PRIMARY KEY(identity_sha256, input_sha256, dimensions)
                );",
                )
                .map_err(|e| e.to_string())?;
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
}

impl CorpusStore for SqliteStore {
    fn structure(&self) -> StorageStructure {
        self.structure
    }

    fn sync(&mut self, _corpus: &PreparedCorpus) -> Result<SyncStats, String> {
        Err("SQLite corpus sync is not implemented yet".into())
    }

    fn load(&self) -> Result<PreparedCorpus, String> {
        Err("SQLite corpus not stored yet".into())
    }

    fn activity(&self, _source_id: &str) -> Result<Option<ActivityRecord>, String> {
        Err("SQLite corpus not stored yet".into())
    }

    fn layout_stats(&self) -> Result<LayoutStats, String> {
        Err("SQLite corpus not stored yet".into())
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
