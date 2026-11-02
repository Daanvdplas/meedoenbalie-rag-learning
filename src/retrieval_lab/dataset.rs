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

pub fn bundled_questions() -> Result<QuestionCatalog, String> {
    let sources = crate::source::parse_sources(crate::DOCUMENTS)?;
    let corpus_sha256 =
        crate::manifest::hash(crate::DOCUMENTS.join("\n---document---\n").as_bytes());
    let mut cases = Vec::new();
    let mut ids = BTreeSet::new();
    let mut versions = Vec::new();
    let mut sets = Vec::new();
    for input in QUESTION_INPUTS {
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
            questions_sha256: crate::manifest::hash(
                &serde_json::to_vec(&sets).map_err(|error| error.to_string())?,
            ),
            corpus_sha256,
            as_of: crate::AS_OF.into(),
            dataset_versions: versions,
            limitation: "Existing development/regression cases; previously seen comparison questions are not a new held-out benchmark".into(),
        },
    })
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
