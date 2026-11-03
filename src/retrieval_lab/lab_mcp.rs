//! Fixed-configuration, read-only MCP over the same local retriever as the lab.
use super::{
    preparation::LocalRetriever,
    types::{PreparationRecord, RetrievalTimings, RunConfig},
};
use crate::{
    codex::CodexRun,
    retrieval::{
        CorpusIdentity, QueryFilters, RetrievalHit, RetrievalOutcome, RetrievalRequest,
        SearchMethod,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    io::{BufRead, Write},
    path::PathBuf,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionConfig {
    pub schema_version: u32,
    pub config: RunConfig,
    pub filters: QueryFilters,
    pub model_cache: PathBuf,
    pub sqlite_dir: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transport: Option<super::mcp_trace::TransportConfig>,
}

impl SessionConfig {
    pub fn validate(&self) -> Result<(), String> {
        if !matches!(self.schema_version, 1 | 2)
            || (self.schema_version == 2) != self.transport.is_some()
            || !self.model_cache.is_absolute()
            || !self.sqlite_dir.is_absolute()
        {
            return Err("Invalid selected retrieval session schema or cache location".into());
        }
        if let Some(transport) = &self.transport {
            transport.validate()?;
        }
        if !self.config.profile.supported() {
            return Err("Selected SQLite profile is unsupported; enable the sqlite feature".into());
        }
        crate::retrieval::validate_config(&self.config.retrieval, &self.filters)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct SourceInfo {
    id: String,
    date: String,
    title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LabManifest {
    pub schema_version: u32,
    pub config_id: String,
    pub config: RunConfig,
    pub filters: QueryFilters,
    pub score_kind: SearchMethod,
    pub corpus_identity: CorpusIdentity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchResponse {
    schema_version: u32,
    question: String,
    top_k: usize,
    sources: Vec<SourceInfo>,
    as_of: String,
    manifest: LabManifest,
    hits: Vec<RetrievalHit>,
    outcome: RetrievalOutcome,
    preparation: PreparationRecord,
    timings: RetrievalTimings,
}

pub struct SearchSession {
    settings: SessionConfig,
    retriever: Option<LocalRetriever>,
    trace: Option<super::mcp_trace::TraceWriter>,
}

impl SearchSession {
    pub fn new(settings: SessionConfig) -> Result<Self, String> {
        settings.validate()?;
        let trace = settings
            .transport
            .clone()
            .map(super::mcp_trace::TraceWriter::open)
            .transpose()?;
        Ok(Self {
            trace,
            settings,
            retriever: None,
        })
    }
    pub fn search(&mut self, query: &str, top_k: usize) -> Result<Value, String> {
        let query = crate::validate_query(query)?;
        if top_k != self.settings.config.retrieval.top_k {
            return Err("The retrieval configuration is fixed for this session".into());
        }
        if self.retriever.is_none() {
            self.retriever = Some(LocalRetriever::new(
                self.settings.config.clone(),
                &self.settings.model_cache,
                &self.settings.sqlite_dir,
            )?);
        }
        let retriever = self.retriever.as_mut().unwrap();
        let (outcome, timings) = retriever.search(&RetrievalRequest {
            query: query.clone(),
            filters: self.settings.filters.clone(),
        })?;
        let response = SearchResponse {
            schema_version: 1,
            question: query,
            top_k,
            sources: source_info(&retriever.corpus().sections),
            as_of: crate::AS_OF.into(),
            manifest: LabManifest {
                schema_version: 1,
                config_id: super::catalog::config_id(&self.settings.config)?,
                config: self.settings.config.clone(),
                filters: self.settings.filters.clone(),
                score_kind: self.settings.config.retrieval.method,
                corpus_identity: outcome.corpus_identity.clone(),
            },
            hits: outcome.hits.clone(),
            outcome,
            preparation: retriever.preparation().clone(),
            timings,
        };
        let full = serde_json::to_value(response)
            .map_err(|_| "Cannot encode selected retrieval evidence")?;
        match &mut self.trace {
            Some(trace) => trace.record(full),
            None => Ok(full),
        }
    }
}

fn source_info(sections: &[crate::Section]) -> Vec<SourceInfo> {
    let mut seen = BTreeSet::new();
    sections
        .iter()
        .filter(|section| seen.insert(section.source_id.clone()))
        .map(|section| SourceInfo {
            id: section.source_id.clone(),
            date: section.source_date.clone(),
            title: section.source_title.clone(),
        })
        .collect()
}

pub fn serve(
    settings: SessionConfig,
    reader: impl BufRead,
    writer: impl Write,
) -> Result<(), String> {
    settings.validate()?;
    let policy = crate::mcp::SearchPolicy::fixed(
        settings.config.retrieval.top_k,
        format!(
            "Search five fictional Dutch activity sources with the fixed selected configuration {}. Use top_k={} for every call. Method/profile/settings and typed filters cannot be changed by this tool. Ranking scores are not confidence. Returned exact sections retain document ownership and source date. Search again only for missing information, at most four calls. No current availability may be inferred from a schedule.",
            super::catalog::config_id(&settings.config)?,
            settings.config.retrieval.top_k
        ),
    )?;
    let mut session = SearchSession::new(settings)?;
    crate::mcp::serve_with_policy(
        reader,
        writer,
        |query, top_k| session.search(query, top_k),
        policy,
    )
}

/// Independently bind the runtime's actual tool evidence to this session and corpus.
/// Ranking scores remain search signals; this is not an answer-quality judge.
pub fn validate_run(run: &CodexRun, settings: &SessionConfig) -> Result<(), String> {
    settings.validate()?;
    let bound;
    let run = if let Some(transport) = &settings.transport {
        bound = super::mcp_trace::bind_run(run, transport)?.0;
        &bound
    } else {
        run
    };
    if run.status != "completed"
        || run.tool_calls.is_empty()
        || run.tool_calls.len() > crate::mcp::MAX_SEARCH_CALLS
    {
        return Err("output: Invalid selected retrieval completion or tool count".into());
    }
    let sections: Vec<_> = crate::source::parse_sources(crate::DOCUMENTS)?
        .into_iter()
        .flat_map(|source| source.sections)
        .collect();
    let corpus = crate::retrieval::PreparedCorpus::new(sections, None, None)?;
    let sources = source_info(&corpus.sections);
    if run.sources
        != serde_json::to_value(&sources)
            .map_err(|_| "output: Invalid source metadata")?
            .as_array()
            .unwrap()
            .clone()
        || run.as_of != crate::AS_OF
    {
        return Err("output: Selected retrieval source metadata changed".into());
    }
    let config_id = super::catalog::config_id(&settings.config)?;
    let mut offered = Vec::new();
    let mut first_manifest = None;
    for call in &run.tool_calls {
        if call.server != "coa"
            || call.tool != "search_document"
            || call.top_k != settings.config.retrieval.top_k
        {
            return Err("output: Selected retrieval treatment or tool changed".into());
        }
        if call.status != "completed" {
            if call.result.is_some() {
                return Err("output: Failed search invented a result".into());
            }
            continue;
        }
        let response: SearchResponse = serde_json::from_value(
            call.result
                .clone()
                .ok_or("output: Missing selected search result")?,
        )
        .map_err(|_| "output: Invalid selected search result schema")?;
        let identity = &response.manifest.corpus_identity;
        let mut filter_config = settings.config.retrieval.clone();
        filter_config.method = SearchMethod::Bm25;
        let expected_filters = crate::retrieval::retrieve(
            &corpus,
            &RetrievalRequest {
                query: call.query.clone(),
                filters: settings.filters.clone(),
            },
            &filter_config,
            None,
        )?
        .filters;
        if response.schema_version != 1
            || response.manifest.schema_version != 1
            || response.question != call.query
            || response.outcome.query != call.query
            || response.top_k != call.top_k
            || response.sources != sources
            || response.as_of != crate::AS_OF
            || response.manifest.config_id != config_id
            || response.manifest.config != settings.config
            || response.manifest.filters != settings.filters
            || response.manifest.score_kind != settings.config.retrieval.method
            || response.outcome.config != settings.config.retrieval
            || response.outcome.version != crate::retrieval::RETRIEVAL_VERSION
            || response.outcome.filters != expected_filters
            || response.outcome.corpus_identity != *identity
            || identity.source_sha256 != corpus.identity.source_sha256
            || identity.embedding_inputs_sha256 != corpus.identity.embedding_inputs_sha256
            || response.preparation.actual_profile != Some(settings.config.profile)
            || response.preparation.corpus_identity.as_ref() != Some(identity)
            || response.outcome.estimated_context_tokens
                > settings.config.retrieval.context_token_limit
            || response.outcome.token_estimator != crate::retrieval::TOKEN_ESTIMATOR
            || response.outcome.estimated_context_tokens
                != response
                    .outcome
                    .hits
                    .iter()
                    .map(|hit| crate::retrieval::estimate_section_tokens(&hit.section))
                    .sum::<usize>()
            || serde_json::to_value(&response.hits).map_err(|_| "output: Invalid hits")?
                != serde_json::to_value(&response.outcome.hits)
                    .map_err(|_| "output: Invalid offered evidence")?
        {
            return Err("output: Selected retrieval configuration or corpus changed".into());
        }
        let dense = settings.config.retrieval.method != SearchMethod::Bm25;
        if dense != identity.embedding.is_some() || dense != identity.vectors_sha256.is_some() {
            return Err("output: Selected retrieval silently changed embedding mode".into());
        }
        if identity
            .embedding
            .as_ref()
            .is_some_and(|model| model.dimensions != 384 || model.provider != "local-fastembed")
        {
            return Err("output: Unexpected selected retrieval embedding provider".into());
        }
        for candidate in &response.outcome.candidates {
            if !corpus
                .sections
                .iter()
                .any(|section| crate::retrieval::SectionKey::from(section) == candidate.key)
                || !candidate.scores.rank_score.is_finite()
                || candidate.scores.dense_similarity.is_some() != dense
                || candidate.scores.bm25_score.is_some()
                    != (settings.config.retrieval.method != SearchMethod::Dense)
                || candidate
                    .scores
                    .dense_similarity
                    .is_some_and(|score| !score.is_finite())
                || candidate
                    .scores
                    .bm25_score
                    .is_some_and(|score| !score.is_finite())
            {
                return Err("output: Selected retrieval score kind or candidate changed".into());
            }
        }
        let mut identities = BTreeSet::new();
        for hit in response.hits {
            let key = crate::retrieval::SectionKey::from(&hit.section);
            let original = corpus
                .sections
                .iter()
                .find(|section| crate::retrieval::SectionKey::from(*section) == key)
                .ok_or("output: Selected retrieval returned an unknown section")?;
            if !identities.insert(key)
                || serde_json::to_value(original).map_err(|_| "output: Invalid canonical source")?
                    != serde_json::to_value(&hit.section)
                        .map_err(|_| "output: Invalid offered source")?
            {
                return Err(
                    "output: Selected retrieval changed canonical source text or identity".into(),
                );
            }
            offered.push(hit.section);
        }
        let manifest = serde_json::to_value(&response.manifest)
            .map_err(|_| "output: Invalid selected manifest")?;
        if manifest != run.manifest
            || first_manifest
                .as_ref()
                .is_some_and(|first| first != &manifest)
        {
            return Err("output: Selected retrieval manifest changed during the run".into());
        }
        first_manifest = Some(manifest);
    }
    if first_manifest.is_none() {
        return Err("output: No successful selected retrieval".into());
    }
    if run.answer.text.trim().is_empty()
        || (run.answer.citations.is_empty() && run.answer.unknowns.is_empty())
    {
        return Err("output: Answer needs evidence or stated unknowns".into());
    }
    for citation in &run.answer.citations {
        if citation.quote.trim().is_empty()
            || !offered.iter().any(|section| {
                section.source_id == citation.source_id
                    && section.source_date == citation.source_date
                    && section.id == citation.section_id
                    && section.text.contains(&citation.quote)
            })
        {
            return Err("output: Citation is absent from selected retrieved evidence".into());
        }
    }
    Ok(())
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::{
        codex::{Citation, GroundedAnswer, ToolCall},
        retrieval::{RetrievalConfig, SearchMethod},
        storage::StorageProfile,
    };
    use serde_json::json;

    pub(in crate::retrieval_lab) fn settings() -> SessionConfig {
        SessionConfig {
            schema_version: 1,
            transport: None,
            config: RunConfig {
                profile: StorageProfile::DocumentsActivities,
                retrieval: RetrievalConfig {
                    method: SearchMethod::Bm25,
                    ..Default::default()
                },
            },
            filters: Default::default(),
            model_cache: std::env::temp_dir()
                .join(format!("coa-live-no-model-{}", std::process::id())),
            sqlite_dir: std::env::temp_dir()
                .join(format!("coa-live-unused-sqlite-{}", std::process::id())),
        }
    }

    fn run_with(response: Value) -> CodexRun {
        let section = &response["hits"][0]["section"];
        CodexRun {
            id: "fixture-live-run".into(),
            status: "completed".into(),
            question: "dinsdag binnen".into(),
            answer: GroundedAnswer {
                text: "Een brongebonden antwoord.".into(),
                citations: vec![Citation {
                    source_id: section["source_id"].as_str().unwrap().into(),
                    source_date: section["source_date"].as_str().unwrap().into(),
                    section_id: section["id"].as_str().unwrap().into(),
                    quote: section["text"].as_str().unwrap().into(),
                }],
                unknowns: vec![],
            },
            tool_calls: vec![ToolCall {
                server: "coa".into(),
                tool: "search_document".into(),
                query: "dinsdag binnen".into(),
                top_k: 3,
                status: "completed".into(),
                result: Some(response.clone()),
                error: None,
            }],
            sources: response["sources"].as_array().unwrap().clone(),
            as_of: crate::AS_OF.into(),
            manifest: response["manifest"].clone(),
            client_version: "fixture".into(),
            model: "fixture".into(),
            elapsed_ms: 1,
            usage: None,
        }
    }

    #[test]
    fn selected_keyword_mcp_preserves_real_scores_sources_and_treatment_without_encoder() {
        let config = settings();
        let mut session = SearchSession::new(config.clone()).unwrap();
        let response = session.search("dinsdag binnen", 3).unwrap();
        assert_eq!(
            response["manifest"]["config"]["profile"],
            "documents_activities"
        );
        assert_eq!(response["manifest"]["score_kind"], "bm25");
        assert_eq!(response["outcome"]["config"]["method"], "bm25");
        assert!(!response["hits"].as_array().unwrap().is_empty());
        assert!(response["hits"][0].get("similarity").is_none());
        assert_eq!(
            response["hits"][0]["scores"]["dense_similarity"],
            Value::Null
        );
        assert!(!config.model_cache.exists());
        validate_run(&run_with(response), &config).unwrap();
        assert!(session.search("dinsdag binnen", 6).is_err());
    }

    #[test]
    fn selected_guard_rejects_changed_config_or_unretrieved_and_changed_source_text() {
        let config = settings();
        let response = SearchSession::new(config.clone())
            .unwrap()
            .search("dinsdag binnen", 3)
            .unwrap();
        let mut changed = run_with(response.clone());
        changed.tool_calls[0].result.as_mut().unwrap()["manifest"]["config"]["retrieval"]["method"] =
            json!("dense");
        assert!(validate_run(&changed, &config).is_err());
        let mut changed = run_with(response.clone());
        changed.tool_calls[0].result.as_mut().unwrap()["hits"][0]["section"]["text"] =
            json!("Verzonnen broninhoud.");
        assert!(validate_run(&changed, &config).is_err());
        let mut changed = run_with(response.clone());
        changed.answer.citations[0].source_date = "2026-11-02".into();
        assert!(validate_run(&changed, &config).is_err());
        let mut changed = run_with(response);
        changed.tool_calls[0].top_k = 6;
        assert!(validate_run(&changed, &config).is_err());
    }

    #[test]
    fn selected_guard_rejects_changed_filter_report_and_context_accounting() {
        let config = settings();
        let response = SearchSession::new(config.clone())
            .unwrap()
            .search("dinsdag binnen", 3)
            .unwrap();
        let mut changed = run_with(response.clone());
        changed.tool_calls[0].result.as_mut().unwrap()["outcome"]["filters"]["applied"] =
            json!(true);
        assert!(
            validate_run(&changed, &config).is_err(),
            "A trace must not invent applied filters"
        );
        let mut changed = run_with(response);
        changed.tool_calls[0].result.as_mut().unwrap()["outcome"]["estimated_context_tokens"] =
            json!(0);
        assert!(
            validate_run(&changed, &config).is_err(),
            "A trace must preserve context accounting"
        );
    }
}
