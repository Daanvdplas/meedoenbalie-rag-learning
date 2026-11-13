//! Versioned evaluation input and protected, separately loaded expectations.
use crate::{manifest::hash, source::Source};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SectionRef {
    pub source_id: String,
    pub section_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorpusSource {
    pub id: String,
    pub date: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputCase {
    pub id: String,
    pub family: String,
    pub split: String,
    pub kind: String,
    pub question: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuestionSet {
    pub schema_version: String,
    pub dataset_version: String,
    pub as_of: String,
    pub corpus_sha256: String,
    pub sources: Vec<CorpusSource>,
    pub selection_note: String,
    pub cases: Vec<InputCase>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactCriterion {
    pub id: String,
    pub description: String,
    /// `unknown` permits no positive evidence target; it is still graded semantically.
    pub kind: String,
    /// Alternatives are OR; every reference in one alternative is required (AND).
    pub evidence_alternatives: Vec<Vec<SectionRef>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaseCriteria {
    pub case_id: String,
    pub facts: Vec<FactCriterion>,
    pub conditions: Vec<String>,
    pub forbidden_claims: Vec<String>,
    pub required_unknowns: Vec<String>,
    pub relevance_labels_complete: bool,
    pub relevant_sections: Vec<SectionRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LabelSet {
    pub schema_version: String,
    pub dataset_version: String,
    pub questions_sha256: String,
    pub evaluation_only_canary: String,
    pub review_note: String,
    pub cases: Vec<CaseCriteria>,
}

pub fn questions_hash(questions: &QuestionSet) -> Result<String, String> {
    serde_json::to_vec(questions)
        .map(|bytes| hash(&bytes))
        .map_err(|_| "Cannot serialize evaluation questions".into())
}

pub fn load(questions_path: &Path, labels_path: &Path) -> Result<(QuestionSet, LabelSet), String> {
    let questions =
        std::fs::read_to_string(questions_path).map_err(|_| "Cannot read evaluation questions")?;
    let labels =
        std::fs::read_to_string(labels_path).map_err(|_| "Cannot read evaluation expectations")?;
    parse(&questions, &labels)
}

pub fn parse(questions: &str, labels: &str) -> Result<(QuestionSet, LabelSet), String> {
    parse_for_corpus(questions, labels, crate::corpus::CorpusId::BaselineFive)
}

pub fn parse_for_corpus(
    questions: &str,
    labels: &str,
    corpus: crate::corpus::CorpusId,
) -> Result<(QuestionSet, LabelSet), String> {
    let questions: QuestionSet = serde_json::from_str(questions)
        .map_err(|_| "Evaluation questions do not match the schema")?;
    let labels: LabelSet = serde_json::from_str(labels)
        .map_err(|_| "Evaluation expectations do not match the schema")?;
    let sources = corpus.sources()?;
    validate_for_date(
        &questions,
        &labels,
        &sources,
        corpus.documents(),
        corpus.as_of(),
    )?;
    Ok((questions, labels))
}

pub fn validate_bundled() -> Result<(QuestionSet, LabelSet), String> {
    parse(
        include_str!("../../shared/datasets/legacy/questions.json"),
        include_str!("../../shared/datasets/legacy/expectations.json"),
    )
}

pub fn validate(
    questions: &QuestionSet,
    labels: &LabelSet,
    sources: &[Source],
    documents: &[&str],
) -> Result<(), String> {
    validate_for_date(questions, labels, sources, documents, crate::AS_OF)
}

fn validate_for_date(
    questions: &QuestionSet,
    labels: &LabelSet,
    sources: &[Source],
    documents: &[&str],
    as_of: &str,
) -> Result<(), String> {
    if questions.schema_version != "1" || labels.schema_version != "1" {
        return Err("Unsupported evaluation dataset schema".into());
    }
    if questions.dataset_version.trim().is_empty()
        || questions.dataset_version != labels.dataset_version
        || questions_hash(questions)? != labels.questions_sha256
    {
        return Err("Question and expectation versions or fingerprints differ".into());
    }
    crate::source::validate_date(&questions.as_of)?;
    if questions.as_of != as_of
        || sources.len() != documents.len()
        || questions.sources.len() != sources.len()
        || questions.corpus_sha256 != hash(documents.join("\n---document---\n").as_bytes())
    {
        return Err("Dataset corpus or reference date differs from the frozen corpus".into());
    }
    let mut source_ids = BTreeSet::new();
    for fingerprint in &questions.sources {
        crate::source::validate_date(&fingerprint.date)?;
        let Some((index, source)) = sources
            .iter()
            .enumerate()
            .find(|(_, source)| source.id == fingerprint.id)
        else {
            return Err("Dataset references an unknown document".into());
        };
        if !source_ids.insert(&fingerprint.id)
            || source.date != fingerprint.date
            || fingerprint.date > questions.as_of
            || hash(documents[index].as_bytes()) != fingerprint.sha256
        {
            return Err("Dataset document identity, date or fingerprint differs".into());
        }
    }
    if questions.cases.is_empty()
        || questions.selection_note.trim().is_empty()
        || labels.evaluation_only_canary.trim().is_empty()
        || labels.review_note.trim().is_empty()
    {
        return Err("Evaluation dataset or provenance is empty".into());
    }
    let mut case_ids = BTreeSet::new();
    let mut family_splits = BTreeMap::new();
    for case in &questions.cases {
        if crate::validate_query(&case.question)? != case.question {
            return Err("Question must match its normalized model input exactly".into());
        }
        if case.id.trim().is_empty()
            || case.family.trim().is_empty()
            || !case_ids.insert(case.id.as_str())
        {
            return Err("Question IDs and families must be nonempty and IDs unique".into());
        }
        if !["development", "pilot", "held_out"].contains(&case.split.as_str())
            || ![
                "multiple_facts",
                "paraphrase",
                "combined_conditions",
                "stale_information",
                "missing_information",
                "all_matches",
                "no_matching_activity",
            ]
            .contains(&case.kind.as_str())
        {
            return Err("Unknown evaluation split or question type".into());
        }
        if family_splits
            .insert(case.family.as_str(), case.split.as_str())
            .is_some_and(|previous| previous != case.split)
        {
            return Err("A question family cannot cross evaluation splits".into());
        }
    }
    let label_ids: BTreeSet<_> = labels
        .cases
        .iter()
        .map(|case| case.case_id.as_str())
        .collect();
    if label_ids.len() != labels.cases.len() || label_ids != case_ids {
        return Err("Questions and expectations must match one-to-one".into());
    }
    let sections: BTreeSet<_> = sources
        .iter()
        .flat_map(|source| &source.sections)
        .map(|section| SectionRef {
            source_id: section.source_id.clone(),
            section_id: section.id.clone(),
        })
        .collect();
    for case in &labels.cases {
        if case.facts.is_empty() {
            return Err("Each evaluation case needs explicit semantic criteria".into());
        }
        let relevant: BTreeSet<_> = case.relevant_sections.iter().cloned().collect();
        if relevant.len() != case.relevant_sections.len() || !relevant.is_subset(&sections) {
            return Err("Relevance labels contain duplicates or unknown source sections".into());
        }
        let mut fact_ids = BTreeSet::new();
        for fact in &case.facts {
            if fact.id.trim().is_empty()
                || !fact_ids.insert(&fact.id)
                || fact.description.trim().is_empty()
                || !["fact", "unknown"].contains(&fact.kind.as_str())
            {
                return Err("Fact criteria need unique IDs, descriptions and known kinds".into());
            }
            if fact.evidence_alternatives.is_empty() && fact.kind != "unknown" {
                return Err("A positive fact needs an explicit valid evidence target".into());
            }
            if fact.kind == "unknown" && case.required_unknowns.is_empty() {
                return Err("Unknown facts need explicit required unknowns".into());
            }
            let mut alternatives = BTreeSet::new();
            for alternative in &fact.evidence_alternatives {
                let refs: BTreeSet<_> = alternative.iter().cloned().collect();
                if refs.is_empty()
                    || refs.len() != alternative.len()
                    || !refs.is_subset(&sections)
                    || !refs.is_subset(&relevant)
                    || !alternatives.insert(refs)
                {
                    return Err(
                        "Evidence alternatives contain empty, duplicate or invalid references"
                            .into(),
                    );
                }
            }
        }
        for values in [
            &case.conditions,
            &case.forbidden_claims,
            &case.required_unknowns,
        ] {
            let unique: BTreeSet<_> = values.iter().collect();
            if unique.len() != values.len() || values.iter().any(|value| value.trim().is_empty()) {
                return Err("Semantic criteria cannot contain empty or duplicated text".into());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixtures() -> (QuestionSet, LabelSet, Vec<Source>) {
        let (questions, labels) = validate_bundled().unwrap();
        let sources = crate::source::parse_sources(crate::DOCUMENTS).unwrap();
        (questions, labels, sources)
    }

    fn reseal(questions: &QuestionSet, labels: &mut LabelSet) {
        labels.questions_sha256 = questions_hash(questions).unwrap();
    }

    #[test]
    fn rejects_duplicate_question_ids_even_when_labels_have_a_valid_hash() {
        let (mut questions, mut labels, sources) = fixtures();
        questions.cases[1].id = questions.cases[0].id.clone();
        reseal(&questions, &mut labels);
        assert!(validate(&questions, &labels, &sources, crate::DOCUMENTS).is_err());
    }

    #[test]
    fn questions_cannot_change_between_dataset_hash_and_normalized_model_input() {
        let (mut questions, mut labels, sources) = fixtures();
        questions.cases[0].question = format!(" {} ", questions.cases[0].question);
        labels.questions_sha256 = questions_hash(&questions).unwrap();
        assert!(validate(&questions, &labels, &sources, crate::DOCUMENTS).is_err());
    }

    #[test]
    fn bundled_pilot_covers_all_five_sources_without_claiming_unseen_quality() {
        let (questions, labels, _) = fixtures();
        assert_eq!(questions.cases.len(), 12);
        assert_eq!(labels.cases.len(), 12);
        assert_eq!(questions.sources.len(), 5);
        assert_eq!(
            questions
                .cases
                .iter()
                .filter(|case| case.split == "development")
                .count(),
            6
        );
        assert_eq!(
            questions
                .cases
                .iter()
                .filter(|case| case.split == "pilot")
                .count(),
            6
        );
        assert!(questions.cases.iter().all(|case| case.split != "held_out"));
        assert_eq!(questions_hash(&questions).unwrap(), labels.questions_sha256);
        let used: BTreeSet<_> = labels
            .cases
            .iter()
            .flat_map(|case| &case.relevant_sections)
            .map(|reference| &reference.source_id)
            .collect();
        assert_eq!(used.len(), 5);
    }

    #[test]
    fn changed_questions_or_source_bytes_require_new_fingerprints() {
        let (mut questions, mut labels, sources) = fixtures();
        questions.cases[0]
            .question
            .push_str(" Anders geformuleerd.");
        assert!(validate(&questions, &labels, &sources, crate::DOCUMENTS).is_err());
        reseal(&questions, &mut labels);
        assert!(validate(&questions, &labels, &sources, crate::DOCUMENTS).is_ok());
        questions.sources[0].sha256 = "0".repeat(64);
        reseal(&questions, &mut labels);
        assert!(validate(&questions, &labels, &sources, crate::DOCUMENTS).is_err());
    }

    #[test]
    fn question_families_and_reference_dates_cannot_drift_silently() {
        let (mut questions, mut labels, sources) = fixtures();
        questions.cases[6].family = questions.cases[0].family.clone();
        reseal(&questions, &mut labels);
        assert!(validate(&questions, &labels, &sources, crate::DOCUMENTS).is_err());
        questions.cases[6].family = "all-tuesday-activities".into();
        questions.as_of = "2026-02-30".into();
        reseal(&questions, &mut labels);
        assert!(validate(&questions, &labels, &sources, crate::DOCUMENTS).is_err());
    }

    #[test]
    fn missing_or_duplicate_expectations_are_not_partially_accepted() {
        let (questions, mut labels, sources) = fixtures();
        labels.cases.pop();
        assert!(validate(&questions, &labels, &sources, crate::DOCUMENTS).is_err());
        let (_, mut labels, _) = fixtures();
        labels.cases[1].case_id = labels.cases[0].case_id.clone();
        assert!(validate(&questions, &labels, &sources, crate::DOCUMENTS).is_err());
    }

    #[test]
    fn positive_facts_need_nonempty_real_source_alternatives() {
        let (questions, mut labels, sources) = fixtures();
        labels.cases[0].facts[0].evidence_alternatives.clear();
        assert!(validate(&questions, &labels, &sources, crate::DOCUMENTS).is_err());
        let (_, mut labels, _) = fixtures();
        labels.cases[0].facts[0].evidence_alternatives.push(vec![]);
        assert!(validate(&questions, &labels, &sources, crate::DOCUMENTS).is_err());
        let (_, mut labels, _) = fixtures();
        labels.cases[0].facts[0].evidence_alternatives[0][0].section_id = "S99".into();
        assert!(validate(&questions, &labels, &sources, crate::DOCUMENTS).is_err());
    }

    #[test]
    fn absent_evidence_has_explicit_unknown_semantics_not_a_vacuous_match() {
        let (questions, mut labels, sources) = fixtures();
        assert!(labels.cases[4].facts[0].evidence_alternatives.is_empty());
        assert_eq!(labels.cases[4].facts[0].kind, "unknown");
        labels.cases[4].required_unknowns.clear();
        assert!(validate(&questions, &labels, &sources, crate::DOCUMENTS).is_err());
    }

    #[test]
    fn question_input_cannot_contain_expectation_fields() {
        let (questions, labels, _) = fixtures();
        let value = serde_json::to_value(&questions).unwrap();
        let serialized = value.to_string();
        assert!(!serialized.contains(&labels.evaluation_only_canary));
        assert!(!serialized.contains("forbidden_claims"));
        let mut value = value;
        value["cases"][0]["facts"] = serde_json::json!(["hidden label"]);
        assert!(serde_json::from_value::<QuestionSet>(value).is_err());
        assert!(
            crate::DOCUMENTS
                .iter()
                .all(|text| !text.contains(&labels.evaluation_only_canary))
        );
    }

    #[test]
    fn human_worksheet_has_eight_unlabelled_candidates_and_separate_sets() {
        let value: serde_json::Value =
            serde_json::from_str(include_str!("../fixtures/human-review.json")).unwrap();
        let cases = value["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 8);
        assert!(cases.iter().all(|case| case["human_label"].is_null()));
        assert_eq!(
            cases
                .iter()
                .filter(|case| case["subset"] == "development")
                .count(),
            4
        );
        assert_eq!(
            cases
                .iter()
                .filter(|case| case["subset"] == "validation")
                .count(),
            4
        );
        let html = include_str!("../docs/human-review.html");
        assert!(!html.contains("checked="));
        assert!(!html.contains("https://"));
        assert!(html.contains("Kopieer beoordelingen"));
    }
}
