//! Protected criteria are loaded only for assessment and report generation.
use super::{
    dataset::{self, QuestionDataset},
    types::RetrievalRecord,
};
use crate::experiment::dataset::CaseCriteria;
use std::collections::BTreeMap;

#[derive(Debug, Clone, serde::Serialize)]
pub struct ReferenceMetadata {
    pub expected_action: String,
    pub fact_roles: BTreeMap<String, String>,
    pub origin: String,
}

pub struct ExpectationsContext {
    pub dataset: QuestionDataset,
    pub criteria: BTreeMap<String, CaseCriteria>,
    pub expectations_sha256: String,
    pub metadata: BTreeMap<String, ReferenceMetadata>,
}

impl ExpectationsContext {
    pub fn load(id: &str) -> Result<Self, String> {
        let dataset = dataset::load(id)?;
        let (questions, labels): (Vec<&str>, Vec<&str>) = if id == "legacy" {
            (
                dataset::QUESTION_INPUTS.to_vec(),
                vec![
                    include_str!("../../fixtures/experiments/expectations.json"),
                    include_str!("../../fixtures/experiments/heldout-expectations.json"),
                ],
            )
        } else {
            (
                vec![dataset::INDEPENDENT_QUESTIONS],
                vec![dataset::INDEPENDENT_EXPECTATIONS],
            )
        };
        let mut criteria = BTreeMap::new();
        for (questions, labels) in questions.iter().zip(&labels) {
            let (_, parsed) = crate::experiment::dataset::parse(questions, labels)?;
            for case in parsed.cases {
                if criteria.insert(case.case_id.clone(), case).is_some() {
                    return Err("Duplicate expectation case".into());
                }
            }
        }
        let mut metadata = BTreeMap::new();
        if id != "legacy" {
            let details = include_str!(
                "../../fixtures/experiments/independent-20261103/reference-details.json"
            );
            let frozen: serde_json::Value =
                serde_json::from_str(dataset::FROZEN).map_err(|e| e.to_string())?;
            for (index, text) in [(1, dataset::INDEPENDENT_EXPECTATIONS), (2, details)] {
                if frozen["integrity"]["artifacts"][index]["sha256_bytes"]
                    != crate::manifest::hash(text.as_bytes())
                {
                    return Err(
                        "Frozen reference bytes changed; create a new dataset version".into(),
                    );
                }
            }
            let parsed: serde_json::Value =
                serde_json::from_str(details).map_err(|e| e.to_string())?;
            for case in parsed["cases"]
                .as_array()
                .ok_or("Missing reference metadata")?
            {
                let id = case["case_id"].as_str().ok_or("Missing reference case")?;
                let criterion = criteria
                    .get(id)
                    .ok_or("Reference metadata has unknown case")?;
                let roles: BTreeMap<String, String> =
                    serde_json::from_value(case["fact_roles"].clone())
                        .map_err(|e| e.to_string())?;
                if roles.len() != criterion.facts.len()
                    || criterion
                        .facts
                        .iter()
                        .any(|fact| !roles.contains_key(&fact.id))
                    || roles.values().any(|role| {
                        !["support", "exclusion", "unknown", "clarification"]
                            .contains(&role.as_str())
                    })
                {
                    return Err("Reference roles disagree with criteria".into());
                }
                let value = ReferenceMetadata {
                    expected_action: case["expected_action"]
                        .as_str()
                        .ok_or("Missing expected action")?
                        .into(),
                    origin: case["origin"]
                        .as_str()
                        .ok_or("Missing reference provenance")?
                        .into(),
                    fact_roles: roles,
                };
                if metadata.insert(id.into(), value).is_some() {
                    return Err("Duplicate reference metadata".into());
                }
            }
            if metadata.len() != criteria.len() {
                return Err("Incomplete reference metadata".into());
            }
        }
        Ok(Self {
            dataset,
            criteria,
            metadata,
            expectations_sha256: crate::manifest::hash(
                &serde_json::to_vec(&labels).map_err(|e| e.to_string())?,
            ),
        })
    }

    pub fn validate_record(&self, record: &RetrievalRecord) -> Result<(), String> {
        if record.dataset != self.dataset.catalog.identity
            || !self.dataset.catalog.cases.iter().any(|case| {
                case.id == record.case_id
                    && case.question == record.question
                    && case.family == record.family
                    && case.split == record.split
                    && case.kind == record.kind
            })
        {
            return Err("Stored run does not match the selected dataset/case/question; select the original --dataset explicitly".into());
        }
        if record.query_inputs_sha256 != self.dataset.query_inputs_sha256
            || (self.dataset.id != "legacy"
                && self.dataset.query_inputs.get(&record.case_id) != Some(&record.request_filters))
        {
            return Err(
                "Stored typed user inputs do not match selected input-only artifact".into(),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::retrieval_lab::{
        assessment::{self, AssessmentKind},
        types::StoredRecord,
    };

    fn input(context: &ExpectationsContext) -> StoredRecord {
        let mut record = crate::retrieval_lab::records::fixture_record();
        let case = &context.dataset.catalog.cases[0];
        record.case_id = case.id.clone();
        record.family = case.family.clone();
        record.split = case.split.clone();
        record.kind = case.kind.clone();
        record.question = case.question.clone();
        record.outcome.as_mut().unwrap().query = case.question.clone();
        record.dataset = context.dataset.catalog.identity.clone();
        record.request_filters = context.dataset.query_inputs[&case.id].clone();
        record.query_inputs_sha256 = context.dataset.query_inputs_sha256.clone();
        StoredRecord {
            record_sha256: crate::manifest::hash(&serde_json::to_vec(&record).unwrap()),
            record,
        }
    }

    #[test]
    fn custom_reference_import_recomputes_scores_even_with_the_correct_hash() {
        let context = ExpectationsContext::load("independent-20261103").unwrap();
        let input = input(&context);
        let mut grades = assessment::assess_with_context(
            std::slice::from_ref(&input),
            &[AssessmentKind::Deterministic],
            &context,
        )
        .unwrap();
        assessment::validate_bindings_with_context(std::slice::from_ref(&input), &grades, &context)
            .unwrap();
        assert_eq!(
            grades[0].expectations_sha256.as_ref(),
            Some(&context.expectations_sha256)
        );
        let metric = grades[0]
            .deterministic
            .as_mut()
            .unwrap()
            .metrics
            .iter_mut()
            .find(|metric| metric.id == "evidence_coverage")
            .unwrap();
        metric.value = Some(0.12345);
        assert!(
            assessment::validate_bindings_with_context(
                std::slice::from_ref(&input),
                &grades,
                &context
            )
            .is_err()
        );
        assert!(assessment::validate_bindings(&[input], &grades).is_err());
    }

    #[test]
    fn identity_checks_bind_question_filters_and_source_dataset() {
        let context = ExpectationsContext::load("independent-20261103").unwrap();
        let input = input(&context);
        for index in 0..6 {
            let mut record = input.record.clone();
            match index {
                0 => record.dataset.corpus_sha256 = "a".repeat(64),
                1 => record.dataset.questions_sha256 = "a".repeat(64),
                2 => record.case_id = "IND26-99".into(),
                3 => record.question.push_str(" changed"),
                4 => record.request_filters = Default::default(),
                _ => record.query_inputs_sha256 = Some("a".repeat(64)),
            }
            assert!(context.validate_record(&record).is_err());
        }
    }

    #[test]
    fn independent_metadata_preserves_unknown_exclusion_and_clarification_roles() {
        let context = ExpectationsContext::load("independent-20261103").unwrap();
        assert_eq!(context.criteria.len(), 12);
        assert_eq!(context.metadata.len(), 12);
        for role in ["support", "exclusion", "unknown", "clarification"] {
            assert!(
                context
                    .metadata
                    .values()
                    .any(|case| case.fact_roles.values().any(|value| value == role))
            );
        }
        assert_eq!(
            context.metadata["IND26-11"].expected_action,
            "ask_which_activity"
        );
        assert_ne!(
            context.expectations_sha256,
            ExpectationsContext::load("legacy")
                .unwrap()
                .expectations_sha256
        );
    }
}
