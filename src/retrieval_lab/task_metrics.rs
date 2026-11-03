//! Derived, versioned task measurements. These do not change stored v2 assessments.
use std::collections::BTreeSet;

use serde::Serialize;

use super::{assessment::DeterministicAssessment, types::RetrievalRecord};
use crate::experiment::dataset::SectionRef;

pub const TASK_METRICS_VERSION: &str = "retrieval-task-metrics-v1";

#[derive(Debug, Serialize)]
pub struct TaskMetrics {
    pub complete_goal_set: Option<bool>,
    pub covered_goals: Option<usize>,
    pub target_goals: usize,
    pub partial_goal_coverage: Option<f64>,
    pub first_goal_support_reciprocal_rank: Option<f64>,
    pub goal_support_rank_unavailable_reason: Option<String>,
    pub minimum_required_sections: Option<usize>,
    pub section_capacity_limit: Option<usize>,
    pub minimum_required_activities: Option<usize>,
    pub seed_activity_capacity_limit: Option<usize>,
    pub goal_set_exceeds_capacity: Option<bool>,
    pub unknown_obligations: usize,
    pub answer_review: &'static str,
    pub not_applicable_reason: Option<String>,
}

/// Call only with a freshly recomputed deterministic assessment of this record.
/// Imported metrics must never be a source of truth for these derived values.
pub fn derive(record: &RetrievalRecord, assessment: &DeterministicAssessment) -> TaskMetrics {
    let goals: Vec<_> = assessment
        .expected_facts
        .iter()
        .filter(|fact| !fact.criterion.evidence_alternatives.is_empty())
        .collect();
    let valid_evidence = [
        "execution_completed",
        "record_identity",
        "canonical_evidence",
    ]
    .iter()
    .all(|id| {
        assessment
            .hard_checks
            .iter()
            .any(|check| check.id == *id && check.passed)
    });
    let applicable = valid_evidence && !goals.is_empty();
    let covered = goals
        .iter()
        .filter(|fact| fact.covered == Some(true))
        .count();
    let support: BTreeSet<_> = goals
        .iter()
        .flat_map(|fact| &fact.criterion.evidence_alternatives)
        .flatten()
        .cloned()
        .collect();
    let rank_valid = applicable
        && assessment
            .hard_checks
            .iter()
            .any(|check| check.id == "retrieval_trace" && check.passed);
    let first_goal_support_reciprocal_rank = rank_valid.then(|| {
        record
            .outcome
            .as_ref()
            .and_then(|outcome| {
                outcome.seeds.iter().position(|key| {
                    support.contains(&SectionRef {
                        source_id: key.source_id.clone(),
                        section_id: key.section_id.clone(),
                    })
                })
            })
            .map_or(0.0, |rank| 1.0 / (rank + 1) as f64)
    });
    let minimum_required_sections = minimum_sections(
        goals
            .iter()
            .map(|fact| fact.criterion.evidence_alternatives.as_slice()),
    );
    let section_capacity_limit = (record.config.retrieval.selection
        == crate::retrieval::SelectionMode::Sections)
        .then_some(record.config.retrieval.top_k);
    let activity_goals: Vec<Vec<Vec<String>>> = goals
        .iter()
        .map(|fact| {
            fact.criterion
                .evidence_alternatives
                .iter()
                .map(|alternative| {
                    alternative
                        .iter()
                        .map(|key| key.source_id.clone())
                        .collect()
                })
                .collect()
        })
        .collect();
    let minimum_required_activities = minimum_sections(activity_goals.iter().map(Vec::as_slice));
    let seed_activity_capacity_limit = (record.config.retrieval.selection
        == crate::retrieval::SelectionMode::ActivityExpansion)
        .then_some(record.config.retrieval.top_k);
    // This is only a necessary bound. Passing it does not establish feasibility:
    // available candidates, typed filters and the token budget can still prevent a complete proof.
    let exceeds_activity_capacity = minimum_required_activities
        .zip(seed_activity_capacity_limit)
        .and_then(|(minimum, capacity)| (minimum > capacity).then_some(true));
    let not_applicable_reason = if goals.is_empty() {
        Some("No labelled source-evidence target; answer behavior remains unassessed".into())
    } else if !valid_evidence {
        Some("No completed canonical retrieval with the expected input identity".into())
    } else {
        None
    };
    TaskMetrics {
        complete_goal_set: applicable.then_some(covered == goals.len()),
        covered_goals: valid_evidence.then_some(covered),
        target_goals: goals.len(),
        partial_goal_coverage: applicable.then(|| covered as f64 / goals.len() as f64),
        first_goal_support_reciprocal_rank,
        goal_support_rank_unavailable_reason: (!rank_valid).then(|| {
            if !applicable {
                "No applicable source-evidence goals or valid retrieval evidence".into()
            } else {
                "The ranking trace failed structural integrity checks".into()
            }
        }),
        minimum_required_sections,
        section_capacity_limit,
        minimum_required_activities,
        seed_activity_capacity_limit,
        goal_set_exceeds_capacity: minimum_required_sections
            .zip(section_capacity_limit)
            .map(|(minimum, capacity)| minimum > capacity)
            .or(exceeds_activity_capacity),
        unknown_obligations: assessment.required_unknowns.len(),
        answer_review: "not_generated",
        not_applicable_reason,
    }
}

/// Minimum union across one AND alternative per goal. Supersets cannot improve
/// later unions and are removed, so shared sections are never counted twice.
fn minimum_sections<'a, T: Ord + Clone + 'a>(
    goals: impl Iterator<Item = &'a [Vec<T>]>,
) -> Option<usize> {
    let mut frontier = vec![BTreeSet::new()];
    let mut has_goal = false;
    for alternatives in goals {
        has_goal = true;
        let mut next: Vec<BTreeSet<T>> = Vec::new();
        for previous in &frontier {
            for alternative in alternatives {
                let union: BTreeSet<_> = previous.iter().chain(alternative).cloned().collect();
                if next.iter().any(|other| other.is_subset(&union)) {
                    continue;
                }
                next.retain(|other| !union.is_subset(other));
                next.push(union);
            }
        }
        frontier = next;
    }
    has_goal
        .then(|| frontier.iter().map(BTreeSet::len).min())
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::retrieval_lab::{
        assessment::{AssessmentKind, assess},
        records::fixture_record_for,
        types::StoredRecord,
    };

    fn measured(case_id: &str) -> (RetrievalRecord, DeterministicAssessment) {
        let record = fixture_record_for(case_id);
        let stored = StoredRecord {
            record_sha256: crate::manifest::hash(&serde_json::to_vec(&record).unwrap()),
            record: record.clone(),
        };
        let mut assessments = assess(&[stored], &[AssessmentKind::Deterministic]).unwrap();
        (record, assessments.remove(0).deterministic.unwrap())
    }

    fn section(id: &str) -> SectionRef {
        SectionRef {
            source_id: "ACT-TUIN-001".into(),
            section_id: id.into(),
        }
    }

    #[test]
    fn capacity_is_exact_across_and_or_choices_and_shared_sections() {
        // Choosing locally smallest {S1} would require three total sections.
        // Choosing {S2,S3} instead shares the second goal and needs only two.
        let goals = [
            vec![vec![section("S1")], vec![section("S2"), section("S3")]],
            vec![vec![section("S2"), section("S3")]],
        ];
        assert_eq!(minimum_sections(goals.iter().map(Vec::as_slice)), Some(2));
        assert_eq!(
            minimum_sections(std::iter::empty::<&[Vec<SectionRef>]>()),
            None
        );
    }

    #[test]
    fn activity_expansion_top_k_is_not_a_section_capacity_limit() {
        let (mut record, assessment) = measured("T05");
        record.config.retrieval.selection = crate::retrieval::SelectionMode::ActivityExpansion;
        let metrics = derive(&record, &assessment);
        assert_eq!(metrics.minimum_required_sections, Some(4));
        assert_eq!(metrics.section_capacity_limit, None);
        assert_eq!(metrics.goal_set_exceeds_capacity, None);
    }

    #[test]
    fn sourced_uncertainty_is_an_evidence_target_with_answer_review_still_pending() {
        let (record, assessment) = measured("P11");
        assert_eq!(assessment.expected_facts[0].criterion.kind, "unknown");
        let metrics = derive(&record, &assessment);
        assert_eq!(metrics.target_goals, 1);
        assert!(metrics.complete_goal_set.is_some());
        assert_eq!(metrics.unknown_obligations, 1);
        assert_eq!(metrics.answer_review, "not_generated");
    }

    #[test]
    fn relevance_only_sections_do_not_become_goal_support() {
        // Actual saved dense search: the first seed is relevant exclusion
        // evidence; bicycle time appears second, but its location is missing.
        let raw = include_str!("../../fixtures/experiments/regressions/t02-dense-search.json");
        let record: RetrievalRecord = serde_json::from_str(raw).unwrap();
        let stored = StoredRecord {
            record: record.clone(),
            record_sha256: crate::manifest::hash(raw.trim_end().as_bytes()),
        };
        let assessment = assess(&[stored], &[AssessmentKind::Deterministic])
            .unwrap()
            .remove(0)
            .deterministic
            .unwrap();
        assert!(assessment.hard_checks_passed);
        let metrics = derive(&record, &assessment);
        assert_eq!(metrics.complete_goal_set, Some(false));
        assert_eq!(metrics.first_goal_support_reciprocal_rank, Some(0.5));
        let relevant = assessment
            .metrics
            .iter()
            .find(|metric| metric.id == "first_relevant_reciprocal_rank")
            .unwrap();
        assert_eq!(relevant.value, Some(1.0));
    }

    #[test]
    fn invalid_ranking_does_not_hide_valid_evidence_but_cannot_get_an_mrr() {
        let (record, mut assessment) = measured("P01");
        assessment
            .hard_checks
            .iter_mut()
            .find(|check| check.id == "retrieval_trace")
            .unwrap()
            .passed = false;
        let metrics = derive(&record, &assessment);
        assert!(metrics.complete_goal_set.is_some());
        assert!(metrics.first_goal_support_reciprocal_rank.is_none());
        assessment
            .hard_checks
            .iter_mut()
            .find(|check| check.id == "canonical_evidence")
            .unwrap()
            .passed = false;
        let metrics = derive(&record, &assessment);
        assert_eq!(metrics.complete_goal_set, None);
        assert_eq!(metrics.partial_goal_coverage, None);
        assert_eq!(metrics.covered_goals, None);
    }
}

#[cfg(test)]
mod expansion_capacity_tests {
    use super::*;
    #[test]
    fn five_required_source_activities_cannot_fit_three_expansion_seeds() {
        let mut record = crate::retrieval_lab::records::fixture_record();
        let stored = super::super::types::StoredRecord {
            record_sha256: crate::manifest::hash(&serde_json::to_vec(&record).unwrap()),
            record: record.clone(),
        };
        let mut assessment = super::super::assessment::assess(
            &[stored],
            &[super::super::assessment::AssessmentKind::Deterministic],
        )
        .unwrap()
        .remove(0)
        .deterministic
        .unwrap();
        assessment.expected_facts = crate::source::parse_sources(crate::DOCUMENTS)
            .unwrap()
            .iter()
            .map(|source| super::super::assessment::ExpectedFact {
                criterion: crate::experiment::dataset::FactCriterion {
                    id: source.id.clone(),
                    description: "synthetic capacity contract".into(),
                    kind: "fact".into(),
                    evidence_alternatives: vec![vec![SectionRef {
                        source_id: source.id.clone(),
                        section_id: "S2".into(),
                    }]],
                },
                covered: Some(false),
                missing_by_alternative: vec![],
                not_applicable_reason: None,
            })
            .collect();
        record.config.retrieval.selection = crate::retrieval::SelectionMode::ActivityExpansion;
        record.config.retrieval.top_k = 3;
        let metrics = derive(&record, &assessment);
        assert_eq!(
            metrics.goal_set_exceeds_capacity,
            Some(true),
            "top-k3 expansion cannot supply evidence from five different activities"
        );
        let json = serde_json::to_value(metrics).unwrap();
        assert_eq!(json["minimum_required_activities"], 5);
        assert_eq!(json["seed_activity_capacity_limit"], 3);
    }
}
