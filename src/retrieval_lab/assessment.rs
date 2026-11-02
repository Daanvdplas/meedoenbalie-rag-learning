use crate::experiment::{
    dataset::{FactCriterion, SectionRef},
    metrics::{HardCheck, Metric},
};
use serde::{Deserialize, Serialize};

use super::types::StoredRecord;

pub const ASSESSOR_VERSION: &str = "retrieval-assessment-v1";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AssessmentKind {
    Deterministic,
    Manual,
    Ai,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AssessmentStatus {
    Completed,
    Unreviewed,
    Paused,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ManualVerdict {
    Good,
    Bad,
    ReviewNeeded,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManualReview {
    pub verdict: ManualVerdict,
    pub notes: String,
    pub reviewed_at_unix_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpectedFact {
    pub criterion: FactCriterion,
    pub covered: Option<bool>,
    pub missing_by_alternative: Vec<Vec<SectionRef>>,
    pub not_applicable_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeterministicAssessment {
    pub hard_checks: Vec<HardCheck>,
    pub hard_checks_passed: bool,
    pub metrics: Vec<Metric>,
    pub expected_facts: Vec<ExpectedFact>,
    pub relevance_labels_complete: bool,
    pub relevant_sections: Vec<SectionRef>,
    pub conditions: Vec<String>,
    pub forbidden_claims: Vec<String>,
    pub required_unknowns: Vec<String>,
    pub interpretation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssessmentRecord {
    pub schema_version: u32,
    pub assessment_id: String,
    pub run_id: String,
    pub case_id: String,
    pub config_id: String,
    pub retrieval_sha256: String,
    pub record_hash_policy: String,
    pub kind: AssessmentKind,
    pub status: AssessmentStatus,
    pub assessor_version: String,
    pub created_at_unix_ms: u64,
    pub expectations_sha256: Option<String>,
    pub deterministic: Option<DeterministicAssessment>,
    pub manual: Option<ManualReview>,
    pub note: Option<String>,
}

fn expectations() -> Result<
    (
        std::collections::BTreeMap<String, crate::experiment::dataset::CaseCriteria>,
        String,
    ),
    String,
> {
    let texts = [
        include_str!("../../fixtures/experiments/expectations.json"),
        include_str!("../../fixtures/experiments/heldout-expectations.json"),
    ];
    let mut cases = std::collections::BTreeMap::new();
    for (questions, labels) in super::dataset::QUESTION_INPUTS.iter().zip(texts) {
        let (_, labels) = crate::experiment::dataset::parse(questions, labels)?;
        for case in labels.cases {
            if cases.insert(case.case_id.clone(), case).is_some() {
                return Err("Duplicate expectation case".into());
            }
        }
    }
    Ok((
        cases,
        crate::manifest::hash(&serde_json::to_vec(&texts).map_err(|error| error.to_string())?),
    ))
}

fn ratio(id: &str, numerator: usize, denominator: usize, definition: &str) -> Metric {
    Metric {
        id: id.into(),
        value: (denominator > 0).then(|| numerator as f64 / denominator as f64),
        numerator: Some(numerator),
        denominator: Some(denominator),
        direction: "higher_is_better".into(),
        definition: definition.into(),
        not_applicable_reason: (denominator == 0)
            .then(|| "No positive targets or retrieved sections in this denominator".into()),
    }
}

fn unavailable(id: &str, definition: &str, reason: &str) -> Metric {
    Metric {
        id: id.into(),
        value: None,
        numerator: None,
        denominator: None,
        direction: "higher_is_better".into(),
        definition: definition.into(),
        not_applicable_reason: Some(reason.into()),
    }
}

fn diagnostic(id: &str, value: Option<f64>, definition: &str) -> Metric {
    Metric {
        id: id.into(),
        value,
        numerator: None,
        denominator: None,
        direction: "diagnostic".into(),
        definition: definition.into(),
        not_applicable_reason: value
            .is_none()
            .then(|| "This stage was not measured".into()),
    }
}

fn reference(section: &crate::Section) -> SectionRef {
    SectionRef {
        source_id: section.source_id.clone(),
        section_id: section.id.clone(),
    }
}

fn deterministic(
    record: &super::types::RetrievalRecord,
    criteria: &crate::experiment::dataset::CaseCriteria,
) -> Result<DeterministicAssessment, String> {
    use super::types::{ExecutionKind, ExecutionStatus};
    use crate::retrieval::{Inclusion, SectionKey, estimate_section_tokens};
    use std::collections::BTreeSet;
    let catalog = super::dataset::bundled_questions()?;
    let corpus = crate::source::parse_sources(crate::DOCUMENTS)?
        .into_iter()
        .flat_map(|source| source.sections)
        .collect::<Vec<_>>();
    let prepared = crate::retrieval::PreparedCorpus::new(corpus.clone(), None, None)?;
    let completed = record.execution_status == ExecutionStatus::Completed;
    let identity = record.dataset == catalog.identity
        && catalog.cases.iter().any(|case| {
            case.id == record.case_id
                && case.question == record.question
                && case.family == record.family
                && case.split == record.split
                && case.kind == record.kind
        })
        && record.config_id == super::catalog::config_id(&record.config)?;
    let hits = record
        .outcome
        .as_ref()
        .map(|outcome| outcome.hits.as_slice())
        .unwrap_or_default();
    let offered: BTreeSet<_> = hits.iter().map(|hit| reference(&hit.section)).collect();
    let canonical = completed
        && offered.len() == hits.len()
        && hits.iter().all(|hit| {
            corpus.iter().any(|section| {
                serde_json::to_value(section).ok() == serde_json::to_value(&hit.section).ok()
            })
        })
        && record
            .preparation
            .corpus_identity
            .as_ref()
            .is_some_and(|identity| {
                identity.source_sha256 == prepared.identity.source_sha256
                    && identity.embedding_inputs_sha256 == prepared.identity.embedding_inputs_sha256
            });
    let trace = record.outcome.as_ref().is_some_and(|outcome| {
        let candidates: BTreeSet<_> = outcome
            .candidates
            .iter()
            .map(|candidate| candidate.key.clone())
            .collect();
        let seeds: BTreeSet<_> = outcome.seeds.iter().cloned().collect();
        let actual_seeds: BTreeSet<_> = hits
            .iter()
            .filter(|hit| hit.inclusion == Inclusion::Seed)
            .map(|hit| SectionKey::from(&hit.section))
            .collect();
        outcome.config == record.config.retrieval
            && outcome.query == record.question
            && record.preparation.corpus_identity.as_ref() == Some(&outcome.corpus_identity)
            && candidates.len() == outcome.candidates.len()
            && seeds.len() == outcome.seeds.len()
            && seeds.is_subset(&candidates)
            && seeds == actual_seeds
            && seeds.len() <= record.config.retrieval.top_k
            && hits
                .iter()
                .filter(|hit| hit.inclusion == Inclusion::ActivityExpansion)
                .all(|hit| {
                    seeds
                        .iter()
                        .any(|key| key.source_id == hit.section.source_id)
                })
    });
    let budget = record.outcome.as_ref().is_some_and(|outcome| {
        outcome.token_estimator == crate::retrieval::TOKEN_ESTIMATOR
            && outcome.estimated_context_tokens
                == hits
                    .iter()
                    .map(|hit| estimate_section_tokens(&hit.section))
                    .sum::<usize>()
            && outcome.estimated_context_tokens <= record.config.retrieval.context_token_limit
    });
    let layout = record.preparation.layout.as_ref().is_some_and(|layout| {
        layout.canonical_sections == corpus.len()
            && match record.config.profile.structure() {
                crate::storage::StorageStructure::Sections => {
                    layout.activity_records == 0 && layout.activity_section_links == 0
                }
                crate::storage::StorageStructure::Activities => {
                    layout.activity_records == prepared.activities.len()
                        && layout.activity_section_links == corpus.len()
                }
            }
    });
    let filter_check = record.outcome.as_ref().is_some_and(|outcome| {
        if !outcome.filters.applied {
            return !record.config.retrieval.apply_filters || !outcome.filters.requested;
        }
        record.config.retrieval.apply_filters
            && hits.iter().all(|hit| {
                outcome
                    .filters
                    .matched_activity_ids
                    .contains(&hit.section.source_id)
                    && prepared
                        .activities
                        .iter()
                        .find(|activity| activity.id == hit.section.source_id)
                        .is_some_and(|activity| {
                            let filters = &record.request_filters;
                            let schedule = filters.weekdays.is_empty() && filters.time.is_none()
                                || activity.schedule.as_ref().is_some_and(|schedule| {
                                    (filters.weekdays.is_empty()
                                        || schedule
                                            .value
                                            .weekdays
                                            .iter()
                                            .any(|day| filters.weekdays.contains(day)))
                                        && filters.time.is_none_or(|time| {
                                            schedule.value.time.start_minutes >= time.start_minutes
                                                && schedule.value.time.end_minutes
                                                    <= time.end_minutes
                                        })
                                });
                            schedule
                                && filters.environment.is_none_or(|environment| {
                                    activity
                                        .environment
                                        .as_ref()
                                        .is_some_and(|fact| fact.value == environment)
                                })
                        })
            })
    });
    let hard_checks: Vec<_> = [
        (
            "execution_completed",
            completed,
            "The search completed; this is not an answer-quality verdict",
        ),
        (
            "record_identity",
            identity,
            "Question/configuration/dataset identity matches the recorded regression input",
        ),
        (
            "canonical_evidence",
            canonical,
            "Every offered passage is unchanged source text with its document, section and date",
        ),
        (
            "retrieval_trace",
            trace,
            "Candidates, seeds, expansion and actual configuration agree",
        ),
        (
            "context_budget",
            budget,
            "Whole-section context matches the declared estimator and ceiling",
        ),
        (
            "storage_layout",
            layout,
            "The actual layout contains the declared section/activity representation",
        ),
        (
            "typed_filters",
            filter_check,
            "Applied hard filters have verified matching fields; unknown values are not confirmed",
        ),
        (
            "measured_execution",
            record.execution_kind == ExecutionKind::LocalMeasured,
            "Fixture executions are examples, not live measurements",
        ),
    ]
    .into_iter()
    .map(|(id, passed, detail)| HardCheck {
        id: id.into(),
        passed,
        detail: detail.into(),
    })
    .collect();
    let valid_evidence = completed && identity && canonical && trace && budget && filter_check;
    let positive_count = criteria
        .facts
        .iter()
        .filter(|fact| !fact.evidence_alternatives.is_empty())
        .count();
    let facts: Vec<_> = criteria.facts.iter().map(|criterion| {
        let missing_by_alternative: Vec<Vec<_>> = criterion.evidence_alternatives.iter().map(|alternative| alternative.iter().filter(|key| !offered.contains(key)).cloned().collect()).collect();
        let applicable = valid_evidence && !criterion.evidence_alternatives.is_empty();
        ExpectedFact { criterion: criterion.clone(), covered: applicable.then(|| missing_by_alternative.iter().any(Vec::is_empty)), missing_by_alternative,
            not_applicable_reason: (!applicable).then(|| if criterion.evidence_alternatives.is_empty() { "No positive source-evidence target; unknown-answer behavior requires a separate answer assessment".into() } else { "Retrieval is incomplete or its evidence/identity checks failed".into() }) }
    }).collect();
    let covered = facts
        .iter()
        .filter(|fact| fact.covered == Some(true))
        .count();
    let mut metrics = vec![if valid_evidence {
        ratio(
            "evidence_coverage",
            covered,
            positive_count,
            "Complete expected evidence groups covered by offered passages; OR between alternatives, AND within each group",
        )
    } else {
        unavailable(
            "evidence_coverage",
            "Complete expected evidence groups offered",
            "No valid canonical retrieval outcome",
        )
    }];
    let relevant: BTreeSet<_> = criteria.relevant_sections.iter().cloned().collect();
    if valid_evidence && criteria.relevance_labels_complete && !relevant.is_empty() {
        let found = offered.intersection(&relevant).count();
        metrics.push(ratio(
            "section_precision",
            found,
            offered.len(),
            "Fraction of offered sections labelled relevant; expansion counts as offered context",
        ));
        metrics.push(ratio("section_recall", found, relevant.len(), "Fraction of all labelled relevant sections offered; differs from complete fact-goal coverage"));
        let rank = record
            .outcome
            .as_ref()
            .unwrap()
            .seeds
            .iter()
            .position(|key| {
                relevant.contains(&SectionRef {
                    source_id: key.source_id.clone(),
                    section_id: key.section_id.clone(),
                })
            });
        metrics.push(match rank {
            Some(index) => ratio(
                "first_relevant_reciprocal_rank",
                1,
                index + 1,
                "Reciprocal rank of first relevant seed; activity expansion is not a ranking",
            ),
            None => ratio(
                "first_relevant_reciprocal_rank",
                0,
                1,
                "No relevant seed was retrieved",
            ),
        });
    } else {
        for (id, definition) in [
            (
                "section_precision",
                "Relevant offered sections / offered sections",
            ),
            (
                "section_recall",
                "Relevant offered sections / all relevant sections",
            ),
            (
                "first_relevant_reciprocal_rank",
                "Reciprocal rank of first relevant seed",
            ),
        ] {
            metrics.push(unavailable(id, definition, "Relevance labels are incomplete, have no positive target, or retrieval evidence is invalid"));
        }
    }
    metrics.push(diagnostic(
        "retrieval_ms",
        record.timings.retrieval_ms,
        "Measured local retrieve() call only; excludes embedding and storage preparation",
    ));
    metrics.push(diagnostic(
        "estimated_context_tokens",
        record
            .outcome
            .as_ref()
            .map(|outcome| outcome.estimated_context_tokens as f64),
        "Declared character-based section estimate, not provider token usage",
    ));
    metrics.push(diagnostic(
        "local_search_calls",
        record.timings.retrieval_ms.map(|_| 1.0),
        "One local retrieval attempt; no Codex generation or AI judge",
    ));
    metrics.push(unavailable(
        "answer_correctness",
        "Correctness of a generated answer",
        "Retrieval-only run: no answer was generated",
    ));
    metrics.push(unavailable(
        "answer_citation_validity",
        "Validity of citations in a generated answer",
        "Retrieval-only run: no answer was generated",
    ));
    Ok(DeterministicAssessment {hard_checks_passed: hard_checks.iter().all(|check| check.passed), hard_checks, metrics, expected_facts: facts,
        relevance_labels_complete: criteria.relevance_labels_complete, relevant_sections: criteria.relevant_sections.clone(), conditions: criteria.conditions.clone(), forbidden_claims: criteria.forbidden_claims.clone(), required_unknowns: criteria.required_unknowns.clone(),
        interpretation: "Evidence coverage is not answer correctness. No calibrated overall retrieval-quality pass/fail threshold is defined. Human review starts blank; AI judging is paused.".into()})
}

pub fn assess(
    records: &[StoredRecord],
    kinds: &[AssessmentKind],
) -> Result<Vec<AssessmentRecord>, String> {
    if records.is_empty()
        || kinds.is_empty()
        || kinds
            .iter()
            .enumerate()
            .any(|(index, kind)| kinds[..index].contains(kind))
    {
        return Err("Select nonempty distinct assessments for existing stored runs".into());
    }
    let (criteria, expectations_sha256) = expectations()?;
    let created = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_millis() as u64;
    let mut assessments = Vec::new();
    for stored in records {
        super::records::validate_record(&stored.record)?;
        let record = &stored.record;
        for kind in kinds {
            let (status, deterministic, note) = match kind {
                AssessmentKind::Deterministic => (
                    AssessmentStatus::Completed,
                    Some(deterministic(
                        record,
                        criteria
                            .get(&record.case_id)
                            .ok_or("No expectations for recorded case")?,
                    )?),
                    None,
                ),
                AssessmentKind::Manual => (
                    AssessmentStatus::Unreviewed,
                    None,
                    Some(
                        "Awaiting an explicit human decision; no agent supplied a human label"
                            .into(),
                    ),
                ),
                AssessmentKind::Ai => (
                    AssessmentStatus::Paused,
                    None,
                    Some("AI judging is paused; no model calls were made".into()),
                ),
            };
            let identity = serde_json::to_vec(&(
                &record.run_id,
                &stored.record_sha256,
                kind,
                ASSESSOR_VERSION,
            ))
            .map_err(|error| error.to_string())?;
            assessments.push(AssessmentRecord {
                schema_version: 1,
                assessment_id: format!("assessment-{}", crate::manifest::hash(&identity)),
                run_id: record.run_id.clone(),
                case_id: record.case_id.clone(),
                config_id: record.config_id.clone(),
                retrieval_sha256: stored.record_sha256.clone(),
                record_hash_policy: super::types::RECORD_HASH_POLICY.into(),
                kind: *kind,
                status,
                assessor_version: ASSESSOR_VERSION.into(),
                created_at_unix_ms: created,
                expectations_sha256: (*kind == AssessmentKind::Deterministic)
                    .then(|| expectations_sha256.clone()),
                deterministic,
                manual: None,
                note,
            });
        }
    }
    Ok(assessments)
}

pub fn validate_bindings(
    records: &[StoredRecord],
    assessments: &[AssessmentRecord],
) -> Result<(), String> {
    let mut identities = std::collections::BTreeSet::new();
    for assessment in assessments {
        let stored = records
            .iter()
            .find(|stored| stored.record.run_id == assessment.run_id)
            .ok_or("Assessment refers to an absent retrieval run")?;
        if assessment.schema_version != 1
            || assessment.assessor_version != ASSESSOR_VERSION
            || assessment.record_hash_policy != super::types::RECORD_HASH_POLICY
            || assessment.retrieval_sha256 != stored.record_sha256
            || assessment.case_id != stored.record.case_id
            || assessment.config_id != stored.record.config_id
            || !identities.insert((
                &assessment.run_id,
                serde_json::to_string(&assessment.kind).unwrap(),
            ))
            || assessment.assessment_id.trim().is_empty()
        {
            return Err("Assessment is stale, duplicated or bound to different retrieval bytes/configuration".into());
        }
        let valid = match assessment.kind {
            AssessmentKind::Deterministic => {
                assessment.status == AssessmentStatus::Completed
                    && assessment.deterministic.is_some()
                    && assessment.manual.is_none()
                    && assessment.expectations_sha256.is_some()
            }
            AssessmentKind::Ai => {
                assessment.status == AssessmentStatus::Paused
                    && assessment.deterministic.is_none()
                    && assessment.manual.is_none()
                    && assessment.expectations_sha256.is_none()
            }
            AssessmentKind::Manual => {
                assessment.deterministic.is_none()
                    && assessment.expectations_sha256.is_none()
                    && match (&assessment.status, &assessment.manual) {
                        (AssessmentStatus::Unreviewed, None) => true,
                        (AssessmentStatus::Completed, Some(manual)) => {
                            manual.notes.chars().count() <= 4000 && manual.reviewed_at_unix_ms > 0
                        }
                        _ => false,
                    }
            }
        };
        if !valid {
            return Err("Assessment status and payload disagree".into());
        }
    }
    Ok(())
}

pub fn read_assessments(
    path: &std::path::Path,
    records: &[StoredRecord],
) -> Result<Vec<AssessmentRecord>, String> {
    if std::fs::metadata(path)
        .map_err(|error| error.to_string())?
        .len()
        > super::records::MAX_ARTIFACT_BYTES as u64
    {
        return Err("Assessment artifact exceeds the size limit".into());
    }
    let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
    let assessments = if bytes.iter().find(|byte| !byte.is_ascii_whitespace()) == Some(&b'[') {
        serde_json::from_slice(&bytes)
            .map_err(|error| format!("Invalid assessment array: {error}"))?
    } else {
        let mut values = Vec::new();
        for line in bytes
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
        {
            values.push(
                serde_json::from_slice(line)
                    .map_err(|error| format!("Invalid assessment record: {error}"))?,
            );
        }
        values
    };
    validate_bindings(records, &assessments)?;
    Ok(assessments)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        retrieval::{Inclusion, RetrievalHit},
        retrieval_lab::{records, types::RetrievalRecord},
    };

    fn stored(record: RetrievalRecord) -> StoredRecord {
        let raw = serde_json::to_vec(&record).unwrap();
        StoredRecord {
            record,
            record_sha256: crate::manifest::hash(&raw),
        }
    }

    #[test]
    fn assessment_reuses_a_stored_run_and_leaves_manual_and_ai_ungraded() {
        let input = stored(records::fixture_record());
        let before = serde_json::to_vec(&input.record).unwrap();
        let result = assess(
            std::slice::from_ref(&input),
            &[
                AssessmentKind::Deterministic,
                AssessmentKind::Manual,
                AssessmentKind::Ai,
            ],
        )
        .unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].status, AssessmentStatus::Completed);
        assert_eq!(result[1].status, AssessmentStatus::Unreviewed);
        assert!(result[1].manual.is_none());
        assert_eq!(result[2].status, AssessmentStatus::Paused);
        assert!(result[2].deterministic.is_none());
        assert_eq!(serde_json::to_vec(&input.record).unwrap(), before);
        assert!(
            result
                .iter()
                .all(|row| row.retrieval_sha256 == input.record_sha256)
        );
        let deterministic = result[0].deterministic.as_ref().unwrap();
        for id in ["answer_correctness", "answer_citation_validity"] {
            let metric = deterministic
                .metrics
                .iter()
                .find(|metric| metric.id == id)
                .unwrap();
            assert!(metric.value.is_none());
            assert!(metric.not_applicable_reason.is_some());
        }
        validate_bindings(std::slice::from_ref(&input), &result).unwrap();
        let mut stale = input;
        stale.record_sha256 = "a".repeat(64);
        assert!(validate_bindings(&[stale], &result).is_err());
    }

    #[test]
    fn t02_coverage_requires_complete_and_evidence_groups() {
        let source = crate::source::parse_sources(crate::DOCUMENTS)
            .unwrap()
            .into_iter()
            .find(|source| source.id == "ACT-FIETS-002")
            .unwrap();
        for (ids, expected) in [(vec!["S2"], 0), (vec!["S3"], 1), (vec!["S2", "S3"], 2)] {
            let mut input = records::fixture_record();
            let outcome = input.outcome.as_mut().unwrap();
            outcome.hits = source
                .sections
                .iter()
                .filter(|section| ids.contains(&section.id.as_str()))
                .map(|section| RetrievalHit {
                    section: section.clone(),
                    scores: None,
                    inclusion: Inclusion::Seed,
                })
                .collect();
            outcome.seeds = outcome
                .hits
                .iter()
                .map(|hit| crate::retrieval::SectionKey::from(&hit.section))
                .collect();
            outcome.estimated_context_tokens = outcome
                .hits
                .iter()
                .map(|hit| crate::retrieval::estimate_section_tokens(&hit.section))
                .sum();
            let result = assess(&[stored(input)], &[AssessmentKind::Deterministic]).unwrap();
            let assessment = result[0].deterministic.as_ref().unwrap();
            let metric = assessment
                .metrics
                .iter()
                .find(|metric| metric.id == "evidence_coverage")
                .unwrap();
            assert_eq!(metric.numerator, Some(expected));
            assert_eq!(metric.denominator, Some(2));
            assert_eq!(metric.value, Some(expected as f64 / 2.0));
        }
    }

    #[test]
    fn incomplete_labels_and_unknown_facts_are_not_perfect_scores() {
        let inputs = vec![
            stored(records::fixture_record_for("P06")),
            stored(records::fixture_record_for("P05")),
        ];
        let result = assess(&inputs, &[AssessmentKind::Deterministic]).unwrap();
        let partial = result[0].deterministic.as_ref().unwrap();
        assert!(!partial.relevance_labels_complete);
        for id in [
            "section_precision",
            "section_recall",
            "first_relevant_reciprocal_rank",
        ] {
            assert!(
                partial
                    .metrics
                    .iter()
                    .find(|metric| metric.id == id)
                    .unwrap()
                    .value
                    .is_none()
            );
        }
        let unknown = result[1].deterministic.as_ref().unwrap();
        let coverage = unknown
            .metrics
            .iter()
            .find(|metric| metric.id == "evidence_coverage")
            .unwrap();
        assert_eq!(coverage.value, None);
        assert_eq!(coverage.denominator, Some(0));
        assert!(
            unknown
                .expected_facts
                .iter()
                .all(|fact| fact.covered.is_none())
        );
    }
}
