use crate::Section;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use super::{
    CorpusIdentity, PreparedCorpus, QueryFilters, RetrievalConfig, RetrievalRequest, SearchMethod,
    SelectionMode,
};

pub const RETRIEVAL_VERSION: &str = "retrieval-v1";
pub const TOKEN_ESTIMATOR: &str = "unicode-scalar-ceil-div4-json-section-v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct SectionKey {
    pub source_id: String,
    pub section_id: String,
}

impl From<&Section> for SectionKey {
    fn from(section: &Section) -> Self {
        Self {
            source_id: section.source_id.clone(),
            section_id: section.id.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ScoreBreakdown {
    pub rank_score: f64,
    pub dense_similarity: Option<f32>,
    pub bm25_score: Option<f32>,
    pub dense_rank: Option<usize>,
    pub keyword_rank: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RankedCandidate {
    pub key: SectionKey,
    pub scores: ScoreBreakdown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Inclusion {
    Seed,
    ActivityExpansion,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetrievalHit {
    pub section: Section,
    pub scores: Option<ScoreBreakdown>,
    pub inclusion: Inclusion,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FilterUnknown {
    pub source_id: String,
    pub fields: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FilterReport {
    pub requested: bool,
    pub applied: bool,
    pub ignored_reason: Option<String>,
    pub matched_activity_ids: Vec<String>,
    pub excluded_activity_ids: Vec<String>,
    pub unknown_activities: Vec<FilterUnknown>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ContextOmission {
    pub key: SectionKey,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RetrievalWork {
    pub dense_vectors_scored: usize,
    pub lexical_sections_scored: usize,
    pub eligible_sections: usize,
    pub union_candidates: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetrievalOutcome {
    pub version: String,
    pub query: String,
    pub config: RetrievalConfig,
    pub corpus_identity: CorpusIdentity,
    pub filters: FilterReport,
    pub candidates: Vec<RankedCandidate>,
    pub seeds: Vec<SectionKey>,
    pub hits: Vec<RetrievalHit>,
    pub omitted: Vec<ContextOmission>,
    pub estimated_context_tokens: usize,
    pub token_estimator: String,
    pub work: RetrievalWork,
}

pub fn retrieve(
    corpus: &PreparedCorpus,
    request: &RetrievalRequest,
    config: &RetrievalConfig,
    query_vector: Option<&[f32]>,
) -> Result<RetrievalOutcome, String> {
    corpus.validate()?;
    let query = crate::validate_query(&request.query)?;
    validate_config(config, &request.filters)?;
    let (filters, eligible) = filter_activities(corpus, request, config);
    let uses_dense = config.method != SearchMethod::Bm25;
    let uses_keyword = config.method != SearchMethod::Dense;
    let dense_scores = if uses_dense {
        let query = query_vector.ok_or(
            "This search method requires an explicit query embedding; no fallback was used",
        )?;
        let vectors = corpus
            .vectors
            .as_ref()
            .ok_or("This search method requires document embeddings; no fallback was used")?;
        Some(
            vectors
                .iter()
                .map(|vector| crate::ranking::cosine(query, vector))
                .collect::<Result<Vec<_>, _>>()?,
        )
    } else {
        None
    };
    let keyword_scores = uses_keyword.then(|| bm25_scores(&corpus.sections, &query));
    let dense_order = dense_scores
        .as_ref()
        .map(|scores| ranked_indices(corpus, scores, &eligible, config.candidate_k, false))
        .unwrap_or_default();
    let keyword_order = keyword_scores
        .as_ref()
        .map(|scores| ranked_indices(corpus, scores, &eligible, config.candidate_k, true))
        .unwrap_or_default();
    let dense_ranks: BTreeMap<_, _> = dense_order
        .iter()
        .enumerate()
        .map(|(rank, index)| (*index, rank + 1))
        .collect();
    let keyword_ranks: BTreeMap<_, _> = keyword_order
        .iter()
        .enumerate()
        .map(|(rank, index)| (*index, rank + 1))
        .collect();
    let max_keyword = keyword_order
        .first()
        .map(|index| keyword_scores.as_ref().unwrap()[*index])
        .unwrap_or(0.0);
    let union: BTreeSet<_> = dense_order.into_iter().chain(keyword_order).collect();
    let mut candidates: Vec<_> = union
        .into_iter()
        .map(|index| {
            let dense = dense_scores.as_ref().map(|scores| scores[index]);
            let keyword = keyword_scores.as_ref().map(|scores| scores[index]);
            let dense_rank = dense_ranks.get(&index).copied();
            let keyword_rank = keyword_ranks.get(&index).copied();
            let rank_score = match config.method {
                SearchMethod::Dense => f64::from(dense.unwrap()),
                SearchMethod::Bm25 => f64::from(keyword.unwrap()),
                SearchMethod::HybridRrf => {
                    dense_rank
                        .map(|rank| 1.0 / (config.rrf_k + rank as f64))
                        .unwrap_or(0.0)
                        + keyword_rank
                            .map(|rank| 1.0 / (config.rrf_k + rank as f64))
                            .unwrap_or(0.0)
                }
                SearchMethod::HybridWeighted => {
                    // Explicit v1 scaling: cosine [-1,1] -> [0,1], BM25 / best positive eligible BM25.
                    let vector = (f64::from(dense.unwrap()) + 1.0) / 2.0;
                    let text = if max_keyword > 0.0 {
                        f64::from(keyword.unwrap()) / f64::from(max_keyword)
                    } else {
                        0.0
                    };
                    config.vector_weight * vector + (1.0 - config.vector_weight) * text
                }
            };
            RankedCandidate {
                key: SectionKey::from(&corpus.sections[index]),
                scores: ScoreBreakdown {
                    rank_score,
                    dense_similarity: dense,
                    bm25_score: keyword,
                    dense_rank,
                    keyword_rank,
                },
            }
        })
        .collect();
    candidates.sort_by(|left, right| {
        right
            .scores
            .rank_score
            .total_cmp(&left.scores.rank_score)
            .then(left.key.cmp(&right.key))
    });
    let by_key: BTreeMap<_, _> = corpus
        .sections
        .iter()
        .map(|section| (SectionKey::from(section), section))
        .collect();
    let selected = if config.mmr {
        mmr_select(&candidates, &by_key, config.top_k, config.mmr_lambda)
    } else {
        (0..candidates.len().min(config.top_k)).collect()
    };
    let seeds: Vec<_> = selected
        .iter()
        .map(|index| candidates[*index].key.clone())
        .collect();
    let mut hits: Vec<_> = selected
        .iter()
        .map(|index| {
            let candidate = &candidates[*index];
            RetrievalHit {
                section: (*by_key[&candidate.key]).clone(),
                scores: Some(candidate.scores.clone()),
                inclusion: Inclusion::Seed,
            }
        })
        .collect();
    let mut estimated_context_tokens = hits
        .iter()
        .map(|hit| estimate_section_tokens(&hit.section))
        .sum::<usize>();
    if estimated_context_tokens > config.context_token_limit {
        return Err(format!(
            "Whole seed sections need {estimated_context_tokens} estimated context tokens, exceeding limit {}; seeds were not silently truncated",
            config.context_token_limit
        ));
    }
    let mut omitted = Vec::new();
    if config.selection == SelectionMode::ActivityExpansion {
        let activities: BTreeSet<_> = seeds.iter().map(|key| key.source_id.as_str()).collect();
        let seed_keys: BTreeSet<_> = seeds.iter().cloned().collect();
        for (key, section) in &by_key {
            if !activities.contains(key.source_id.as_str()) || seed_keys.contains(key) {
                continue;
            }
            let tokens = estimate_section_tokens(section);
            if estimated_context_tokens + tokens > config.context_token_limit {
                omitted.push(ContextOmission {
                    key: key.clone(),
                    reason: "Whole section does not fit the shared estimated context-token ceiling"
                        .into(),
                });
                continue;
            }
            estimated_context_tokens += tokens;
            hits.push(RetrievalHit {
                section: (*section).clone(),
                scores: candidates
                    .iter()
                    .find(|candidate| candidate.key == *key)
                    .map(|candidate| candidate.scores.clone()),
                inclusion: Inclusion::ActivityExpansion,
            });
        }
    }
    let work = RetrievalWork {
        dense_vectors_scored: if uses_dense { corpus.sections.len() } else { 0 },
        lexical_sections_scored: if uses_keyword {
            corpus.sections.len()
        } else {
            0
        },
        eligible_sections: corpus
            .sections
            .iter()
            .filter(|section| eligible.contains(&section.source_id))
            .count(),
        union_candidates: candidates.len(),
    };
    Ok(RetrievalOutcome {
        version: RETRIEVAL_VERSION.into(),
        query,
        config: config.clone(),
        corpus_identity: corpus.identity.clone(),
        filters,
        candidates,
        seeds,
        hits,
        omitted,
        estimated_context_tokens,
        token_estimator: TOKEN_ESTIMATOR.into(),
        work,
    })
}

/// Validate selectable parameters before preparing an index or initializing a model.
pub fn validate_config(config: &RetrievalConfig, filters: &QueryFilters) -> Result<(), String> {
    crate::ranking::validate_top_k(config.top_k)?;
    if !(1..=1000).contains(&config.candidate_k)
        || config.context_token_limit == 0
        || !config.rrf_k.is_finite()
        || config.rrf_k <= 0.0
        || !config.vector_weight.is_finite()
        || !(0.0..=1.0).contains(&config.vector_weight)
        || !config.mmr_lambda.is_finite()
        || !(0.0..=1.0).contains(&config.mmr_lambda)
        || filters
            .time
            .is_some_and(|time| time.start_minutes >= time.end_minutes || time.end_minutes > 1440)
    {
        return Err(
            "Invalid retrieval candidate/context budget, fusion/MMR parameter or typed time filter"
                .into(),
        );
    }
    Ok(())
}

fn filter_activities(
    corpus: &PreparedCorpus,
    request: &RetrievalRequest,
    config: &RetrievalConfig,
) -> (FilterReport, BTreeSet<String>) {
    let requested = !request.filters.weekdays.is_empty()
        || request.filters.environment.is_some()
        || request.filters.time.is_some();
    let mut report = FilterReport {
        requested,
        applied: requested && config.apply_filters,
        ignored_reason: (requested && !config.apply_filters).then(|| {
            "Typed filters were supplied but filtering is disabled by this configuration".into()
        }),
        matched_activity_ids: Vec::new(),
        excluded_activity_ids: Vec::new(),
        unknown_activities: Vec::new(),
    };
    if !report.applied {
        return (
            report,
            corpus
                .activities
                .iter()
                .map(|activity| activity.id.clone())
                .collect(),
        );
    }
    let mut eligible = BTreeSet::new();
    for activity in &corpus.activities {
        let mut mismatch = false;
        let mut unknown = Vec::new();
        if !request.filters.weekdays.is_empty() || request.filters.time.is_some() {
            if let Some(schedule) = &activity.schedule {
                if !request.filters.weekdays.is_empty()
                    && !schedule
                        .value
                        .weekdays
                        .iter()
                        .any(|day| request.filters.weekdays.contains(day))
                {
                    mismatch = true;
                }
                if let Some(time) = request.filters.time {
                    mismatch |= schedule.value.time.start_minutes < time.start_minutes
                        || schedule.value.time.end_minutes > time.end_minutes;
                }
            } else {
                unknown.push("schedule".into());
            }
        }
        if let Some(environment) = request.filters.environment {
            if let Some(fact) = &activity.environment {
                mismatch |= fact.value != environment;
            } else {
                unknown.push("environment".into());
            }
        }
        if mismatch {
            report.excluded_activity_ids.push(activity.id.clone());
        }
        if !unknown.is_empty() {
            report.unknown_activities.push(FilterUnknown {
                source_id: activity.id.clone(),
                fields: unknown,
            });
        } else if !mismatch {
            eligible.insert(activity.id.clone());
            report.matched_activity_ids.push(activity.id.clone());
        }
    }
    (report, eligible)
}

fn tokens(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_owned)
        .collect()
}

pub(crate) fn bm25_scores(sections: &[Section], query: &str) -> Vec<f32> {
    let documents: Vec<_> = sections
        .iter()
        .map(|section| tokens(&super::embedding_input(section)))
        .collect();
    let average_length =
        documents.iter().map(Vec::len).sum::<usize>() as f64 / documents.len() as f64;
    let terms: BTreeSet<_> = tokens(query).into_iter().collect();
    documents
        .iter()
        .map(|document| {
            terms
                .iter()
                .map(|term| {
                    let frequency = document.iter().filter(|word| *word == term).count() as f64;
                    if frequency == 0.0 {
                        return 0.0;
                    }
                    let document_frequency = documents
                        .iter()
                        .filter(|document| document.contains(term))
                        .count() as f64;
                    let idf = (1.0
                        + (documents.len() as f64 - document_frequency + 0.5)
                            / (document_frequency + 0.5))
                        .ln();
                    idf * frequency * 2.2
                        / (frequency + 1.2 * (0.25 + 0.75 * document.len() as f64 / average_length))
                })
                .sum::<f64>() as f32
        })
        .collect()
}

fn ranked_indices(
    corpus: &PreparedCorpus,
    scores: &[f32],
    eligible: &BTreeSet<String>,
    limit: usize,
    positive_only: bool,
) -> Vec<usize> {
    let mut indices: Vec<_> = (0..scores.len())
        .filter(|index| {
            eligible.contains(&corpus.sections[*index].source_id)
                && (!positive_only || scores[*index] > 0.0)
        })
        .collect();
    indices.sort_by(|left, right| {
        scores[*right].total_cmp(&scores[*left]).then(
            SectionKey::from(&corpus.sections[*left])
                .cmp(&SectionKey::from(&corpus.sections[*right])),
        )
    });
    indices.truncate(limit);
    indices
}

fn mmr_select(
    candidates: &[RankedCandidate],
    sections: &BTreeMap<SectionKey, &Section>,
    count: usize,
    lambda: f64,
) -> Vec<usize> {
    if candidates.is_empty() {
        return Vec::new();
    }
    let max = candidates[0].scores.rank_score;
    let min = candidates[candidates.len() - 1].scores.rank_score;
    let words: Vec<BTreeSet<_>> = candidates
        .iter()
        .map(|candidate| tokens(&sections[&candidate.key].text).into_iter().collect())
        .collect();
    let mut selected = vec![0];
    while selected.len() < count.min(candidates.len()) {
        let objective = |index: usize| {
            let relevance = if max > min {
                (candidates[index].scores.rank_score - min) / (max - min)
            } else {
                1.0
            };
            let redundancy = selected
                .iter()
                .map(|chosen| {
                    let union = words[index].union(&words[*chosen]).count();
                    if union == 0 {
                        0.0
                    } else {
                        words[index].intersection(&words[*chosen]).count() as f64 / union as f64
                    }
                })
                .fold(0.0_f64, f64::max);
            lambda * relevance - (1.0 - lambda) * redundancy
        };
        let next = (0..candidates.len())
            .filter(|index| !selected.contains(index))
            .max_by(|left, right| {
                objective(*left)
                    .total_cmp(&objective(*right))
                    .then(right.cmp(left))
            })
            .unwrap();
        selected.push(next);
    }
    selected
}

pub fn estimate_section_tokens(section: &Section) -> usize {
    serde_json::to_string(section)
        .expect("Serializing a section of strings is infallible")
        .chars()
        .count()
        .div_ceil(4)
}
