use crate::experiment::dataset::{InputCase, QuestionSet};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const QUESTION_INPUTS: [&str; 2] = [
    include_str!("../../fixtures/experiments/questions.json"),
    include_str!("../../fixtures/experiments/heldout-questions.json"),
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DatasetIdentity {
    pub questions_sha256: String,
    pub corpus_sha256: String,
    pub as_of: String,
    pub dataset_versions: Vec<String>,
    pub limitation: String,
}

#[derive(Debug, Clone)]
pub struct QuestionCatalog {
    pub cases: Vec<InputCase>,
    pub identity: DatasetIdentity,
}

fn questions_from_inputs(inputs: &[&str], independent: bool) -> Result<QuestionCatalog, String> {
    let sources = crate::source::parse_sources(crate::DOCUMENTS)?;
    let corpus_sha256 =
        crate::manifest::hash(crate::DOCUMENTS.join("\n---document---\n").as_bytes());
    let mut cases = Vec::new();
    let mut ids = BTreeSet::new();
    let mut versions = Vec::new();
    let mut sets = Vec::new();
    for input in inputs {
        let set: QuestionSet = serde_json::from_str(input)
            .map_err(|error| format!("Invalid question input: {error}"))?;
        if set.schema_version != "1"
            || set.as_of != crate::AS_OF
            || set.corpus_sha256 != corpus_sha256
            || set.sources.len() != sources.len()
        {
            return Err("Question input does not identify the current source corpus".into());
        }
        for (source, text) in sources.iter().zip(crate::DOCUMENTS) {
            let matches = set
                .sources
                .iter()
                .filter(|fingerprint| {
                    fingerprint.id == source.id
                        && fingerprint.date == source.date
                        && fingerprint.sha256 == crate::manifest::hash(text.as_bytes())
                })
                .count();
            if matches != 1 {
                return Err("Question input source identity changed".into());
            }
        }
        for case in &set.cases {
            if case.id.trim().is_empty()
                || !ids.insert(case.id.clone())
                || crate::validate_query(&case.question)? != case.question
            {
                return Err("Question input has invalid or duplicate cases".into());
            }
            cases.push(case.clone());
        }
        versions.push(set.dataset_version.clone());
        sets.push(set);
    }
    Ok(QuestionCatalog {
        cases,
        identity: DatasetIdentity {
            questions_sha256: if independent { crate::experiment::dataset::questions_hash(&sets[0])? } else { crate::manifest::hash(&serde_json::to_vec(&sets).map_err(|error| error.to_string())?) },
            corpus_sha256,
            as_of: crate::AS_OF.into(),
            dataset_versions: versions,
            limitation: if independent { "Prospectively frozen AI-authored and AI-reviewed questions on the same five known documents; not human calibration or unseen-corpus/intent generalization" } else { "Existing development/regression cases; previously seen comparison questions are not a new held-out benchmark" }.into(),
        },
    })
}

pub fn bundled_questions() -> Result<QuestionCatalog, String> {
    questions_from_inputs(&QUESTION_INPUTS, false)
}

pub const INDEPENDENT_QUESTIONS: &str =
    include_str!("../../fixtures/experiments/independent-20261103/questions.json");
pub const INDEPENDENT_EXPECTATIONS: &str =
    include_str!("../../fixtures/experiments/independent-20261103/expectations.json");
pub const FROZEN: &str =
    include_str!("../../fixtures/experiments/independent-20261103/FROZEN.json");
pub const FROZEN_SHA256: &str = "351a7df4d85e5295e36d3fbbb31c62948619425158b038bd8370a11c982e2fe6";
const USER_INPUTS: &str =
    include_str!("../../fixtures/experiments/independent-20261103/user-query-inputs.json");

#[derive(Debug, Clone)]
pub struct QuestionDataset {
    pub id: String,
    pub catalog: QuestionCatalog,
    pub query_inputs: std::collections::BTreeMap<String, crate::retrieval::QueryFilters>,
    pub query_inputs_sha256: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UserInputs {
    schema_version: String,
    dataset: String,
    questions_sha256: String,
    frozen_reference_sha256: String,
    provenance: String,
    semantics: String,
    cases: Vec<UserInput>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UserInput {
    case_id: String,
    question: String,
    filters: crate::retrieval::QueryFilters,
}

/// Input-only boundary: never loads expectations or reference-details.
pub fn load(id: &str) -> Result<QuestionDataset, String> {
    match id {
        "legacy" => Ok(QuestionDataset {
            id: id.into(),
            catalog: bundled_questions()?,
            query_inputs: Default::default(),
            query_inputs_sha256: None,
        }),
        "independent-20261103" => {
            if crate::manifest::hash(FROZEN.as_bytes()) != FROZEN_SHA256 {
                return Err("Frozen reference manifest changed".into());
            }
            let freeze: serde_json::Value =
                serde_json::from_str(FROZEN).map_err(|e| e.to_string())?;
            if freeze["integrity"]["artifacts"][0]["sha256_bytes"]
                != crate::manifest::hash(INDEPENDENT_QUESTIONS.as_bytes())
            {
                return Err("Frozen question bytes changed".into());
            }
            let catalog = questions_from_inputs(&[INDEPENDENT_QUESTIONS], true)?;
            let inputs: UserInputs = serde_json::from_str(USER_INPUTS)
                .map_err(|e| format!("Invalid typed user inputs: {e}"))?;
            if inputs.schema_version != "user-query-inputs-v1"
                || inputs.dataset != id
                || inputs.questions_sha256 != catalog.identity.questions_sha256
                || inputs.frozen_reference_sha256 != FROZEN_SHA256
                || inputs.provenance.is_empty()
                || inputs.semantics.is_empty()
                || inputs.cases.len() != catalog.cases.len()
            {
                return Err("Typed user inputs do not match frozen questions".into());
            }
            let mut query_inputs = std::collections::BTreeMap::new();
            for input in inputs.cases {
                if !catalog
                    .cases
                    .iter()
                    .any(|case| case.id == input.case_id && case.question == input.question)
                    || query_inputs
                        .insert(input.case_id, input.filters.clone())
                        .is_some()
                {
                    return Err(
                        "Typed user input has unknown/duplicate case or different question".into(),
                    );
                }
                crate::retrieval::validate_config(&Default::default(), &input.filters)?;
            }
            Ok(QuestionDataset {
                id: id.into(),
                catalog,
                query_inputs,
                query_inputs_sha256: Some(crate::manifest::hash(USER_INPUTS.as_bytes())),
            })
        }
        _ => Err("Unknown dataset; select legacy or independent-20261103 explicitly".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn question_catalogue_has_eighteen_distinct_inputs_and_no_gold() {
        let catalog = bundled_questions().unwrap();
        assert_eq!(catalog.cases.len(), 18);
        assert_eq!(
            catalog
                .cases
                .iter()
                .map(|case| &case.id)
                .collect::<BTreeSet<_>>()
                .len(),
            18
        );
        assert_eq!(
            catalog
                .cases
                .iter()
                .filter(|case| case.id.starts_with('P'))
                .count(),
            12
        );
        assert_eq!(
            catalog
                .cases
                .iter()
                .filter(|case| case.id.starts_with('T'))
                .count(),
            6
        );
        assert_eq!(catalog.identity.as_of, crate::AS_OF);
        let source_hash =
            crate::manifest::hash(crate::DOCUMENTS.join("\n---document---\n").as_bytes());
        assert_eq!(catalog.identity.corpus_sha256, source_hash);
        let input = serde_json::to_string(&catalog.cases).unwrap();
        assert!(!input.contains("evidence_alternatives"));
        assert!(!input.contains("forbidden_claims"));
        assert_eq!(catalog.identity.questions_sha256.len(), 64);
    }
}

#[cfg(test)]
mod input_boundary_tests {
    use super::*;
    #[test]
    fn typed_inputs_accept_only_user_fields_and_keep_soft_preferences_out() {
        let dataset = load("independent-20261103").unwrap();
        let values: serde_json::Value = serde_json::from_str(USER_INPUTS).unwrap();
        assert_eq!(dataset.query_inputs.len(), 12);
        assert_eq!(
            dataset
                .query_inputs
                .values()
                .filter(|filters| **filters != Default::default())
                .count(),
            5
        );
        assert!(
            dataset.query_inputs["IND26-02"].environment.is_none(),
            "outside is only a soft preference"
        );
        assert_eq!(
            dataset.query_inputs["IND26-10"].time.unwrap().end_minutes,
            659
        );
        for field in [
            "facts",
            "expected_action",
            "supported_candidate_activity_ids",
            "fact_roles",
            "evidence",
            "required_unknowns",
        ] {
            let mut changed = values.clone();
            changed["cases"][0][field] = serde_json::json!(["PROTECTED_GOLD"]);
            assert!(
                serde_json::from_value::<UserInputs>(changed).is_err(),
                "unexpected {field} must never enter typed user inputs"
            );
        }
        let mut changed = values;
        changed["cases"][0]["filters"]["activity_ids"] = serde_json::json!(["ACT-FIETS-002"]);
        assert!(serde_json::from_value::<UserInputs>(changed).is_err());
    }
}
