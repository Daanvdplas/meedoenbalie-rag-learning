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
    questions_for_corpus(inputs, independent, crate::corpus::CorpusId::BaselineFive)
}

fn questions_for_corpus(
    inputs: &[&str],
    independent: bool,
    corpus: crate::corpus::CorpusId,
) -> Result<QuestionCatalog, String> {
    let sources = corpus.sources()?;
    let corpus_sha256 = corpus.sha256();
    let mut cases = Vec::new();
    let mut ids = BTreeSet::new();
    let mut versions = Vec::new();
    let mut sets = Vec::new();
    for input in inputs {
        let set: QuestionSet = serde_json::from_str(input)
            .map_err(|error| format!("Invalid question input: {error}"))?;
        if set.schema_version != "1"
            || set.as_of != corpus.as_of()
            || set.corpus_sha256 != corpus_sha256
            || set.sources.len() != sources.len()
        {
            return Err("Question input does not identify the current source corpus".into());
        }
        for (source, text) in sources.iter().zip(corpus.documents()) {
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
            as_of: corpus.as_of().into(),
            dataset_versions: versions,
            limitation: if !corpus.is_baseline() { "Prospectively frozen AI-authored and independently AI-reviewed source expectations for ten fictional documents; adapted known questions, not human calibration or unseen-intent generalization" } else if independent { "Prospectively frozen AI-authored and AI-reviewed questions on the same five known documents; not human calibration or unseen-corpus/intent generalization" } else { "Existing development/regression cases; previously seen comparison questions are not a new held-out benchmark" }.into(),
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
pub const EXPANDED_ID: &str = "expanded-10-20261105";
pub const EXPANDED_QUESTIONS: &str =
    include_str!("../../fixtures/experiments/expanded-10-20261105/questions.json");
pub const EXPANDED_EXPECTATIONS: &str =
    include_str!("../../fixtures/experiments/expanded-10-20261105/expectations.json");
pub const EXPANDED_DETAILS: &str =
    include_str!("../../fixtures/experiments/expanded-10-20261105/reference-details.json");
pub const EXPANDED_SELECTION: &str =
    include_str!("../../fixtures/experiments/expanded-10-20261105/presentation-selection.json");
const EXPANDED_FROZEN: &str =
    include_str!("../../fixtures/experiments/expanded-10-20261105/FROZEN.json");
// Independently approved exact source-reference freeze, before measurement.
const EXPANDED_FROZEN_SHA256: &str =
    "48d09a54102b9cbb75d9bbe4c6436e746cfb8290b323d284a26a4b900436e394";

pub(super) fn verify_expanded_artifact(name: &str, text: &str) -> Result<(), String> {
    let freeze: serde_json::Value =
        serde_json::from_str(EXPANDED_FROZEN).map_err(|error| error.to_string())?;
    if crate::manifest::hash(EXPANDED_FROZEN.as_bytes()) != EXPANDED_FROZEN_SHA256
        || freeze["review"]["approved"] != true
        || freeze["reference_date"] != crate::corpus::CorpusId::ExpandedTen.as_of()
        || freeze["integrity"]["corpus_sha256"] != crate::corpus::CorpusId::ExpandedTen.sha256()
    {
        return Err(
            "Expanded source/reference freeze is not independently approved or has changed".into(),
        );
    }
    let path = format!("fixtures/experiments/{EXPANDED_ID}/{name}");
    let expected = crate::manifest::hash(text.as_bytes());
    let matches = freeze["integrity"]["artifacts"]
        .as_array()
        .ok_or("Missing expanded freeze artifacts")?
        .iter()
        .filter(|artifact| artifact["path"] == path && artifact["sha256_bytes"] == expected)
        .count();
    if matches != 1 {
        return Err(
            "Expanded frozen artifact changed; create a new independently reviewed dataset version"
                .into(),
        );
    }
    Ok(())
}
const APPROVED_INPUTS: &str =
    include_str!("../../fixtures/experiments/independent-20261103/INPUTS_FROZEN.json");
const APPROVED_INPUTS_SHA256: &str =
    "9ac437b814e19a967036297310c013e0d6fce342f5af4d966a68cff8a38dc04b";
const USER_INPUTS: &str =
    include_str!("../../fixtures/experiments/independent-20261103/user-query-inputs.json");

#[derive(Debug, Clone)]
pub struct QuestionDataset {
    pub id: String,
    pub catalog: QuestionCatalog,
    pub query_inputs: std::collections::BTreeMap<String, crate::retrieval::QueryFilters>,
    pub query_inputs_sha256: Option<String>,
}

impl QuestionDataset {
    pub fn corpus(&self) -> crate::corpus::CorpusId {
        if self.id == EXPANDED_ID {
            crate::corpus::CorpusId::ExpandedTen
        } else {
            crate::corpus::CorpusId::BaselineFive
        }
    }
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
            let query_inputs = load_user_inputs(USER_INPUTS, &catalog)?;
            Ok(QuestionDataset {
                id: id.into(),
                catalog,
                query_inputs,
                query_inputs_sha256: Some(crate::manifest::hash(USER_INPUTS.as_bytes())),
            })
        }
        EXPANDED_ID => {
            verify_expanded_artifact("questions.json", EXPANDED_QUESTIONS)?;
            let catalog = questions_for_corpus(&[EXPANDED_QUESTIONS], true, crate::corpus::CorpusId::ExpandedTen)?;
            if catalog.cases.len() != 10 {
                return Err("Expanded presentation requires exactly ten frozen questions".into());
            }
            Ok(QuestionDataset { id: id.into(), catalog, query_inputs: Default::default(), query_inputs_sha256: None })
        }
        _ => Err("Unknown dataset; select legacy, independent-20261103 or expanded-10-20261105 explicitly".into()),
    }
}

fn load_user_inputs(
    text: &str,
    catalog: &QuestionCatalog,
) -> Result<std::collections::BTreeMap<String, crate::retrieval::QueryFilters>, String> {
    // Pin the independently reviewed approval bytes as well as their bindings.
    // A new file with valid questions/schema is not the same frozen treatment.
    let approval: serde_json::Value =
        serde_json::from_str(APPROVED_INPUTS).map_err(|error| error.to_string())?;
    if crate::manifest::hash(APPROVED_INPUTS.as_bytes()) != APPROVED_INPUTS_SHA256
        || approval["schema_version"] != "independent-user-input-supplemental-freeze-v1"
        || approval["dataset"] != "independent-20261103"
        || approval["approved"] != true
        || approval["input_artifact"] != "user-query-inputs.json"
        || approval["input_sha256_bytes"] != crate::manifest::hash(text.as_bytes())
        || approval["source_question_sha256_bytes"]
            != crate::manifest::hash(INDEPENDENT_QUESTIONS.as_bytes())
        || approval["source_questions_canonical_sha256"] != catalog.identity.questions_sha256
        || approval["original_frozen_manifest_sha256_bytes"]
            != crate::manifest::hash(FROZEN.as_bytes())
    {
        return Err("Frozen user inputs or approval binding changed; use a new independently reviewed dataset version".into());
    }
    let inputs: UserInputs =
        serde_json::from_str(text).map_err(|e| format!("Invalid typed user inputs: {e}"))?;
    if inputs.schema_version != "user-query-inputs-v1"
        || inputs.dataset != "independent-20261103"
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
            return Err("Typed user input has unknown/duplicate case or different question".into());
        }
        crate::retrieval::validate_config(&Default::default(), &input.filters)?;
    }
    Ok(query_inputs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn expanded_freeze_rejects_mutated_artifact_bytes_and_wrong_names() {
        for (name, canonical) in [
            ("questions.json", EXPANDED_QUESTIONS),
            ("expectations.json", EXPANDED_EXPECTATIONS),
            ("reference-details.json", EXPANDED_DETAILS),
            ("presentation-selection.json", EXPANDED_SELECTION),
        ] {
            verify_expanded_artifact(name, canonical).unwrap();
            assert!(verify_expanded_artifact(name, &format!("{canonical}\n")).is_err());
            assert!(verify_expanded_artifact("other.json", canonical).is_err());
        }
    }

    #[test]
    fn expanded_dataset_has_ten_input_only_questions_on_sixty_sections() {
        let dataset = load(EXPANDED_ID).unwrap();
        assert_eq!(dataset.catalog.cases.len(), 10);
        assert_eq!(
            dataset
                .corpus()
                .sources()
                .unwrap()
                .iter()
                .map(|source| source.sections.len())
                .sum::<usize>(),
            60
        );
        assert_eq!(dataset.catalog.identity.as_of, "2026-11-05");
        assert_eq!(
            dataset.catalog.identity.corpus_sha256,
            dataset.corpus().sha256()
        );
        assert!(dataset.query_inputs.is_empty());
        assert!(dataset.query_inputs_sha256.is_none());
        let inputs = serde_json::to_string(&dataset.catalog.cases).unwrap();
        for protected in [
            "evidence_alternatives",
            "evaluation_only_canary",
            "forbidden_claims",
            "fact_roles",
        ] {
            assert!(!inputs.contains(protected));
        }
        assert_ne!(
            dataset.catalog.identity.corpus_sha256,
            load("legacy").unwrap().catalog.identity.corpus_sha256
        );
        assert!(
            crate::experiment::dataset::parse(EXPANDED_QUESTIONS, EXPANDED_EXPECTATIONS).is_err()
        );
    }

    #[test]
    fn input_identity_cannot_mix_five_and_ten_source_versions() {
        let expanded = crate::corpus::CorpusId::ExpandedTen;
        assert!(questions_for_corpus(&[QUESTION_INPUTS[0]], false, expanded).is_err());
        let mut input: QuestionSet = serde_json::from_str(QUESTION_INPUTS[0]).unwrap();
        input.as_of = expanded.as_of().into();
        input.corpus_sha256 = expanded.sha256();
        input.sources = expanded
            .sources()
            .unwrap()
            .iter()
            .zip(expanded.documents())
            .map(
                |(source, document)| crate::experiment::dataset::CorpusSource {
                    id: source.id.clone(),
                    date: source.date.clone(),
                    sha256: crate::manifest::hash(document.as_bytes()),
                },
            )
            .collect();
        let json = serde_json::to_string(&input).unwrap();
        let catalog = questions_for_corpus(&[&json], false, expanded).unwrap();
        assert_eq!(catalog.identity.corpus_sha256, expanded.sha256());
        assert!(questions_from_inputs(&[&json], false).is_err());
        input.sources.pop();
        let missing_source = serde_json::to_string(&input).unwrap();
        assert!(questions_for_corpus(&[&missing_source], false, expanded).is_err());
    }

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

#[cfg(test)]
mod approved_input_freeze_tests {
    use super::*;
    #[test]
    fn changed_typed_input_cannot_reuse_the_same_frozen_question_context() {
        let catalog = load("independent-20261103").unwrap().catalog;
        let unchanged = load_user_inputs(USER_INPUTS, &catalog).unwrap();
        assert_eq!(
            unchanged
                .values()
                .filter(|input| **input != Default::default())
                .count(),
            5
        );
        assert_eq!(unchanged.len(), 12);
        let mut value: serde_json::Value = serde_json::from_str(USER_INPUTS).unwrap();
        let original_question = value["cases"][0]["question"].clone();
        let original_question_hash = value["questions_sha256"].clone();
        value["cases"][0]["filters"]["environment"] = serde_json::json!("outdoor");
        assert_eq!(value["cases"][0]["question"], original_question);
        assert_eq!(value["questions_sha256"], original_question_hash);
        let changed = serde_json::to_string(&value).unwrap();
        assert!(
            serde_json::from_str::<UserInputs>(&changed).is_ok(),
            "mutation remains a valid typed input; rejection must be from the freeze binding"
        );
        assert!(
            load_user_inputs(&changed, &catalog).is_err(),
            "changed hard filters must not retain the frozen dataset identity even with unchanged questions and valid schema"
        );
    }
}
