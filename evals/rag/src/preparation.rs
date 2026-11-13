use super::types::{PreparationRecord, RetrievalTimings, RunConfig, SharedPreparation};
use crate::{
    Section,
    retrieval::{
        EmbeddingIdentity, PreparedCorpus, RetrievalOutcome, RetrievalRequest, SearchMethod,
        embedding_input,
    },
    storage::{CacheKey, CorpusStore, DocumentsStore, EmbeddingCache, StorageProfile},
};
use std::{
    path::{Path, PathBuf},
    time::Instant,
};

pub struct LocalRetriever {
    session: PreparationSession,
    prepared: PreparedSearch,
    config: RunConfig,
}

impl LocalRetriever {
    pub fn new(config: RunConfig, model_cache: &Path, sqlite_dir: &Path) -> Result<Self, String> {
        Self::for_corpus(
            config,
            model_cache,
            sqlite_dir,
            crate::corpus::CorpusId::BaselineFive,
        )
    }

    pub fn for_corpus(
        config: RunConfig,
        model_cache: &Path,
        sqlite_dir: &Path,
        corpus: crate::corpus::CorpusId,
    ) -> Result<Self, String> {
        crate::retrieval::validate_config(&config.retrieval, &Default::default())?;
        let options = PreparationOptions {
            model_cache: model_cache.into(),
            sqlite_dir: sqlite_dir.into(),
        };
        let mut session = PreparationSession::for_corpus(&options, corpus)?;
        let prepared = session.prepare(
            config.profile,
            config.retrieval.method != SearchMethod::Bm25,
            "selected",
        )?;
        Ok(Self {
            session,
            prepared,
            config,
        })
    }
    pub fn preparation(&self) -> &PreparationRecord {
        &self.prepared.record
    }
    pub fn corpus(&self) -> &PreparedCorpus {
        &self.prepared.corpus
    }
    pub fn config(&self) -> &RunConfig {
        &self.config
    }
    pub fn shared_preparation(&self) -> SharedPreparation {
        self.session.shared("selected")
    }
    pub fn search(
        &mut self,
        request: &RetrievalRequest,
    ) -> Result<(RetrievalOutcome, RetrievalTimings), String> {
        let attempt = self
            .session
            .search(&mut self.prepared, request, &self.config);
        attempt.outcome.map(|outcome| (outcome, attempt.timings))
    }
}

#[derive(Debug, Clone)]
pub struct PreparationOptions {
    pub model_cache: PathBuf,
    pub sqlite_dir: PathBuf,
}

pub(crate) struct PreparedSearch {
    store: Box<dyn CorpusStore>,
    pub corpus: PreparedCorpus,
    pub record: PreparationRecord,
}

pub(crate) struct SearchAttempt {
    pub outcome: Result<RetrievalOutcome, String>,
    pub timings: RetrievalTimings,
}

pub(crate) struct PreparationSession {
    sections: Vec<Section>,
    #[cfg(feature = "sqlite")]
    sqlite_dir: PathBuf,
    embeddings: LocalEmbeddings,
    memory_cache: EmbeddingCache,
    source_parse_ms: f64,
}

impl PreparationSession {
    pub fn for_corpus(
        options: &PreparationOptions,
        corpus: crate::corpus::CorpusId,
    ) -> Result<Self, String> {
        let started = Instant::now();
        let sections = corpus
            .sources()?
            .into_iter()
            .flat_map(|source| source.sections)
            .collect();
        Ok(Self {
            sections,
            #[cfg(feature = "sqlite")]
            sqlite_dir: if corpus.is_baseline() {
                options.sqlite_dir.clone()
            } else {
                options.sqlite_dir.join(corpus.sha256())
            },
            embeddings: LocalEmbeddings::new(&options.model_cache),
            memory_cache: EmbeddingCache::default(),
            source_parse_ms: elapsed_ms(started),
        })
    }

    pub fn shared(&self, batch_id: &str) -> SharedPreparation {
        SharedPreparation {preparation_id: format!("{batch_id}-shared-embeddings"), source_parse_ms: self.source_parse_ms,
            model_identity_ms: self.embeddings.identity_ms, model_load_ms: self.embeddings.load_ms,
            document_inference_ms: self.embeddings.document_ms, query_inference_ms: self.embeddings.query_ms,
            document_inputs_encoded: self.embeddings.document_count, query_inputs_encoded: self.embeddings.query_count,
            scoring_path: "Canonical store snapshot loaded into the same Rust in-memory scorer; no native SQLite FTS/ANN ranking".into(),
            interpretation: "One shared preparation summary per batch, repeated by identity for inspectability. Do not sum copies. Profile preparation reflects this run order/cache state and is not an independent cold-start storage benchmark.".into()}
    }

    pub fn prepare(
        &mut self,
        profile: StorageProfile,
        with_vectors: bool,
        batch_id: &str,
    ) -> Result<PreparedSearch, String> {
        let preparation_started = Instant::now();
        let mut store: Box<dyn CorpusStore> = if profile.is_sqlite() {
            #[cfg(feature = "sqlite")]
            {
                std::fs::create_dir_all(&self.sqlite_dir).map_err(|error| error.to_string())?;
                let name = match profile.structure() {
                    crate::storage::StorageStructure::Sections => "sections.sqlite",
                    crate::storage::StorageStructure::Activities => "activities.sqlite",
                };
                Box::new(crate::storage::SqliteStore::open(
                    self.sqlite_dir.join(name),
                    profile.structure(),
                )?)
            }
            #[cfg(not(feature = "sqlite"))]
            {
                return Err(
                    "SQLite is unavailable in this build; enable the sqlite feature".into(),
                );
            }
        } else {
            Box::new(DocumentsStore::new(profile.structure()))
        };
        let mut record = PreparationRecord::default();
        let (vectors, identity) = if with_vectors {
            let identity = self.embeddings.identity()?;
            let started = Instant::now();
            let (vectors, hits, misses) = document_vectors(
                &self.sections,
                &identity,
                store.as_mut(),
                &mut self.memory_cache,
                |texts| self.embeddings.embed(texts, EncodingPurpose::Document),
            )?;
            record.document_embedding_ms = Some(elapsed_ms(started));
            record.document_cache_hits = hits;
            record.document_cache_misses = misses;
            record.model_load_ms = self.embeddings.last_load_ms.take();
            (Some(vectors), Some(identity))
        } else {
            (None, None)
        };
        let canonical = PreparedCorpus::new(self.sections.clone(), vectors, identity)?;
        let started = Instant::now();
        record.sync = Some(store.sync(&canonical)?);
        record.storage_sync_ms = Some(elapsed_ms(started));
        let started = Instant::now();
        let corpus = store.load()?;
        record.storage_load_ms = Some(elapsed_ms(started));
        if corpus.identity != canonical.identity {
            return Err("Storage changed the canonical source or embedding identity".into());
        }
        record.preparation_id = Some(format!(
            "{batch_id}-{}",
            &crate::manifest::hash(
                &serde_json::to_vec(&(profile, &corpus.identity))
                    .map_err(|error| error.to_string())?
            )[..16]
        ));
        record.actual_profile = Some(profile);
        record.corpus_identity = Some(corpus.identity.clone());
        record.layout = Some(store.layout_stats()?);
        record.profile_preparation_ms = Some(elapsed_ms(preparation_started));
        Ok(PreparedSearch {
            store,
            corpus,
            record,
        })
    }

    pub fn search(
        &mut self,
        prepared: &mut PreparedSearch,
        request: &RetrievalRequest,
        config: &RunConfig,
    ) -> SearchAttempt {
        let mut timings = RetrievalTimings::default();
        let query = match crate::validate_query(&request.query) {
            Ok(query) => query,
            Err(error) => {
                return SearchAttempt {
                    outcome: Err(error),
                    timings,
                };
            }
        };
        let request = RetrievalRequest {
            query,
            filters: request.filters.clone(),
        };
        let validation = crate::retrieval::validate_config(&config.retrieval, &request.filters);
        if let Err(error) = validation {
            return SearchAttempt {
                outcome: Err(error),
                timings,
            };
        }
        let vector = if config.retrieval.method != SearchMethod::Bm25 {
            let started = Instant::now();
            let result = (|| {
                let identity = self.embeddings.identity()?;
                let key = CacheKey::new(
                    &serde_json::json!({"identity": identity, "purpose": "raw-query-v1"}),
                    &request.query,
                    identity.dimensions,
                )?;
                if let Some(vector) = prepared.store.get_embedding(&key)? {
                    return Ok((vector, true));
                }
                if let Some(vector) = self.memory_cache.get(&key)? {
                    prepared.store.put_embedding(&key, &vector)?;
                    return Ok((vector, true));
                }
                let mut vectors = self
                    .embeddings
                    .embed(std::slice::from_ref(&request.query), EncodingPurpose::Query)?;
                if vectors.len() != 1 {
                    return Err("Expected one query embedding".into());
                }
                let vector = vectors.remove(0);
                prepared.store.put_embedding(&key, &vector)?;
                self.memory_cache.put(&key, &vector)?;
                Ok((vector, false))
            })();
            timings.query_embedding_ms = Some(elapsed_ms(started));
            self.embeddings.last_load_ms.take();
            match result {
                Ok((vector, hit)) => {
                    timings.query_embedding_cache_hit = Some(hit);
                    Some(vector)
                }
                Err(error) => {
                    return SearchAttempt {
                        outcome: Err(error),
                        timings,
                    };
                }
            }
        } else {
            None
        };
        let started = Instant::now();
        let outcome = crate::retrieval::retrieve(
            &prepared.corpus,
            &request,
            &config.retrieval,
            vector.as_deref(),
        );
        timings.retrieval_ms = Some(elapsed_ms(started));
        SearchAttempt { outcome, timings }
    }
}

fn document_vectors(
    sections: &[Section],
    identity: &EmbeddingIdentity,
    store: &mut dyn CorpusStore,
    memory: &mut EmbeddingCache,
    mut embed: impl FnMut(&[String]) -> Result<Vec<Vec<f32>>, String>,
) -> Result<(Vec<Vec<f32>>, usize, usize), String> {
    let mut vectors = vec![None; sections.len()];
    let mut missing: Vec<(CacheKey, String, Vec<usize>)> = Vec::new();
    let mut hits = 0;
    for (index, section) in sections.iter().enumerate() {
        let text = embedding_input(section);
        let key = CacheKey::new(identity, &text, identity.dimensions)?;
        if let Some(vector) = store.get_embedding(&key)?.or(memory.get(&key)?) {
            store.put_embedding(&key, &vector)?;
            memory.put(&key, &vector)?;
            vectors[index] = Some(vector);
            hits += 1;
        } else if let Some((_, _, indices)) =
            missing.iter_mut().find(|(pending, _, _)| *pending == key)
        {
            indices.push(index);
            hits += 1;
        } else {
            missing.push((key, text, vec![index]));
        }
    }
    let misses = missing.len();
    if !missing.is_empty() {
        let embedded = embed(
            &missing
                .iter()
                .map(|(_, text, _)| text.clone())
                .collect::<Vec<_>>(),
        )?;
        if embedded.len() != missing.len() {
            return Err("Embedding count differs from missing document inputs".into());
        }
        for ((key, _, indices), vector) in missing.into_iter().zip(embedded) {
            store.put_embedding(&key, &vector)?;
            memory.put(&key, &vector)?;
            for index in indices {
                vectors[index] = Some(vector.clone());
            }
        }
    }
    Ok((
        vectors
            .into_iter()
            .map(|vector| vector.expect("Every cache miss was embedded"))
            .collect(),
        hits,
        misses,
    ))
}

struct LocalEmbeddings {
    cache: PathBuf,
    model: Option<fastembed::TextEmbedding>,
    identity: Option<EmbeddingIdentity>,
    last_load_ms: Option<f64>,
    identity_ms: Option<f64>,
    load_ms: Option<f64>,
    document_ms: Option<f64>,
    query_ms: Option<f64>,
    document_count: usize,
    query_count: usize,
}

enum EncodingPurpose {
    Document,
    Query,
}

impl LocalEmbeddings {
    fn new(cache: &Path) -> Self {
        Self {
            cache: cache.into(),
            model: None,
            identity: None,
            last_load_ms: None,
            identity_ms: None,
            load_ms: None,
            document_ms: None,
            query_ms: None,
            document_count: 0,
            query_count: 0,
        }
    }
    fn load(&mut self) -> Result<(), String> {
        if self.model.is_none() {
            let started = Instant::now();
            self.model = Some(
                fastembed::TextEmbedding::try_new(
                    fastembed::TextInitOptions::new(
                        fastembed::EmbeddingModel::ParaphraseMLMiniLML12V2Q,
                    )
                    .with_cache_dir(self.cache.clone())
                    .with_show_download_progress(false)
                    .with_max_length(512)
                    .with_intra_threads(2),
                )
                .map_err(|error| error.to_string())?,
            );
            self.last_load_ms = Some(elapsed_ms(started));
            self.load_ms = self.last_load_ms;
        }
        Ok(())
    }
    fn identity(&mut self) -> Result<EmbeddingIdentity, String> {
        if let Some(identity) = &self.identity {
            return Ok(identity.clone());
        }
        let base = self
            .cache
            .join("models--Qdrant--paraphrase-multilingual-MiniLM-L12-v2-onnx-Q");
        if !base.join("refs/main").is_file() {
            self.load()?;
        }
        let started = Instant::now();
        let revision =
            std::fs::read_to_string(base.join("refs/main")).map_err(|error| error.to_string())?;
        let revision = revision.trim();
        if revision.len() != 40 || !revision.bytes().all(|value| value.is_ascii_hexdigit()) {
            return Err("Invalid cached embedding model revision".into());
        }
        let mut files = Vec::new();
        for name in [
            "model_optimized.onnx",
            "config.json",
            "tokenizer.json",
            "tokenizer_config.json",
            "special_tokens_map.json",
        ] {
            let bytes = std::fs::read(base.join("snapshots").join(revision).join(name))
                .map_err(|error| error.to_string())?;
            files.push((name, crate::manifest::hash(&bytes)));
        }
        let artifact_hash =
            crate::manifest::hash(&serde_json::to_vec(&files).map_err(|error| error.to_string())?);
        let identity = EmbeddingIdentity {
            provider: "local-fastembed".into(),
            model: format!("{}@{revision}", crate::MODEL),
            sdk: "fastembed 7.1.2".into(),
            dimensions: 384,
            encoding: format!(
                "unprefixed sentence transformer; title + newline + heading + newline + exact text; raw query; max_tokens=512; model_files_sha256={artifact_hash}"
            ),
        };
        self.identity = Some(identity.clone());
        self.identity_ms = Some(elapsed_ms(started));
        Ok(identity)
    }
    fn embed(
        &mut self,
        texts: &[String],
        purpose: EncodingPurpose,
    ) -> Result<Vec<Vec<f32>>, String> {
        self.load()?;
        let started = Instant::now();
        let result = self
            .model
            .as_mut()
            .unwrap()
            .embed(texts, None)
            .map_err(|error| error.to_string())?;
        let (duration, count) = match purpose {
            EncodingPurpose::Document => (&mut self.document_ms, &mut self.document_count),
            EncodingPurpose::Query => (&mut self.query_ms, &mut self.query_count),
        };
        *duration = Some(duration.unwrap_or(0.0) + elapsed_ms(started));
        *count += texts.len();
        Ok(result)
    }
}

pub(crate) fn elapsed_ms(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        retrieval::{RetrievalConfig, SearchMethod},
        storage::StorageProfile,
    };
    #[test]
    fn keyword_only_selection_never_needs_an_embedding_model_cache() {
        let config = RunConfig {
            profile: StorageProfile::DocumentsActivities,
            retrieval: RetrievalConfig {
                method: SearchMethod::Bm25,
                ..Default::default()
            },
        };
        let missing_cache =
            std::env::temp_dir().join(format!("coa-unused-model-{}", std::process::id()));
        let mut search =
            LocalRetriever::new(config, &missing_cache, Path::new("/missing-unused-sqlite"))
                .unwrap();
        let (outcome, timings) = search
            .search(&RetrievalRequest {
                query: "dinsdag binnen".into(),
                filters: Default::default(),
            })
            .unwrap();
        assert!(!outcome.hits.is_empty());
        assert!(outcome.corpus_identity.embedding.is_none());
        assert_eq!(outcome.work.dense_vectors_scored, 0);
        assert_eq!(
            search
                .preparation()
                .layout
                .as_ref()
                .unwrap()
                .activity_records,
            5
        );
        assert!(search.preparation().profile_preparation_ms.is_some());
        assert_eq!(timings.query_embedding_ms, None);
        assert!(!missing_cache.exists());
    }

    #[test]
    fn equal_embedding_inputs_are_encoded_once_and_changes_invalidate_only_affected_inputs() {
        let mut sections =
            crate::source::parse_sources(crate::DOCUMENTS).unwrap()[0].sections[..1].to_vec();
        let mut duplicate = sections[0].clone();
        duplicate.id = "S99".into();
        sections.push(duplicate);
        let identity = EmbeddingIdentity {
            provider: "fixture".into(),
            model: "mini-fixture".into(),
            sdk: "fixture-v1".into(),
            dimensions: 3,
            encoding: "fixture".into(),
        };
        let mut store = DocumentsStore::new(crate::storage::StorageStructure::Sections);
        let mut memory = EmbeddingCache::default();
        let (first, hits, misses) =
            document_vectors(&sections, &identity, &mut store, &mut memory, |texts| {
                assert_eq!(
                    texts.len(),
                    1,
                    "Equal title/heading/text must not be embedded twice"
                );
                Ok(vec![vec![1.0, 0.0, 0.0]])
            })
            .unwrap();
        assert_eq!(first.len(), 2);
        assert_eq!(first[0], first[1]);
        assert_eq!((hits, misses), (1, 1));
        sections[1].text.push_str(" Changed text.");
        let (_, hits, misses) =
            document_vectors(&sections, &identity, &mut store, &mut memory, |texts| {
                assert_eq!(texts.len(), 1);
                Ok(vec![vec![0.0, 1.0, 0.0]])
            })
            .unwrap();
        assert_eq!((hits, misses), (1, 1));
        let mut empty_memory = EmbeddingCache::default();
        let (_, hits, misses) =
            document_vectors(&sections, &identity, &mut store, &mut empty_memory, |_| {
                panic!("Persisted valid cache must avoid encoding")
            })
            .unwrap();
        assert_eq!((hits, misses), (2, 0));
    }
    #[test]
    fn expanded_storage_profiles_preserve_sixty_sections_and_baseline_cache() {
        let base = std::env::temp_dir().join(format!(
            "coa-corpus-storage-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let options = PreparationOptions {
            model_cache: base.join("unused-model"),
            sqlite_dir: base.join("sqlite"),
        };
        let expanded = crate::corpus::CorpusId::ExpandedTen;
        let mut expected_identity = None;
        for profile in crate::retrieval_lab::catalog::PROFILES
            .into_iter()
            .filter(|profile| profile.supported())
        {
            let mut baseline_session =
                PreparationSession::for_corpus(&options, crate::corpus::CorpusId::BaselineFive)
                    .unwrap();
            let baseline = baseline_session.prepare(profile, false, "old").unwrap();
            assert_eq!(baseline.corpus.sections.len(), 30);
            let mut expanded_session = PreparationSession::for_corpus(&options, expanded).unwrap();
            let prepared = expanded_session.prepare(profile, false, "new").unwrap();
            assert_eq!(prepared.corpus.sections.len(), 60);
            assert_eq!(prepared.corpus.activities.len(), 10);
            assert_ne!(prepared.corpus.identity, baseline.corpus.identity);
            if let Some(identity) = &expected_identity {
                assert_eq!(&prepared.corpus.identity, identity);
            } else {
                expected_identity = Some(prepared.corpus.identity.clone());
            }
            drop(prepared);
            drop(baseline);
            let unchanged = baseline_session
                .prepare(profile, false, "old-again")
                .unwrap();
            assert_eq!(unchanged.corpus.sections.len(), 30);
            assert_eq!(
                unchanged.record.sync.unwrap().sections_unchanged,
                if profile.is_sqlite() { 30 } else { 0 }
            );
        }
        assert!(!options.model_cache.exists());
        if cfg!(feature = "sqlite") {
            assert!(
                options
                    .sqlite_dir
                    .join(expanded.sha256())
                    .join("sections.sqlite")
                    .exists()
            );
        }
        if base.exists() {
            std::fs::remove_dir_all(base).unwrap();
        }
    }
}
