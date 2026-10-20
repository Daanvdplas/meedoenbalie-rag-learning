use meedoen_retrieval::{Index, keyword_search, manifest::hash, source::Source};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashSet;
const QUESTIONS: &str = include_str!("../fixtures/questions.json");
const EXPECTATIONS: &str = include_str!("../fixtures/expectations.json");
#[derive(Deserialize)]
struct Question {
    id: String,
    text: String,
}
#[derive(Deserialize)]
struct Expectations {
    as_of: String,
    document_id: String,
    cases: Vec<Expected>,
}
#[derive(Deserialize)]
struct Expected {
    question_id: String,
    required_source_ids: Vec<String>,
    unknown_flags: Vec<String>,
}
pub fn coverage(required: &[String], retrieved: &[String]) -> Option<f64> {
    let required: HashSet<_> = required.iter().collect();
    let retrieved: HashSet<_> = retrieved.iter().collect();
    if required.is_empty() {
        None
    } else {
        Some(required.intersection(&retrieved).count() as f64 / required.len() as f64)
    }
}
fn validate_fixtures(
    source: &Source,
    questions: &[Question],
    expected: &Expectations,
) -> Result<(), String> {
    if expected.document_id != source.id || expected.as_of != meedoen_retrieval::AS_OF {
        return Err("Evaluation source or as_of mismatch".into());
    }
    let question_ids: HashSet<_> = questions.iter().map(|q| q.id.as_str()).collect();
    let case_ids: HashSet<_> = expected
        .cases
        .iter()
        .map(|case| case.question_id.as_str())
        .collect();
    if questions.is_empty()
        || question_ids.len() != questions.len()
        || case_ids.len() != expected.cases.len()
        || question_ids != case_ids
    {
        return Err("Evaluation questions and labels must match one-to-one".into());
    }
    for question in questions {
        meedoen_retrieval::validate_query(&question.text)?;
    }
    let section_ids: HashSet<_> = source.sections.iter().map(|s| s.id.as_str()).collect();
    for case in &expected.cases {
        let ids: HashSet<_> = case
            .required_source_ids
            .iter()
            .map(String::as_str)
            .collect();
        if ids.len() != case.required_source_ids.len() || !ids.is_subset(&section_ids) {
            return Err("Duplicate or nonexistent labelled source section".into());
        }
    }
    Ok(())
}
pub fn run(index: &mut Index, top_k: usize) -> Result<Value, String> {
    meedoen_retrieval::ranking::validate_top_k(top_k)?;
    let questions: Vec<Question> = serde_json::from_str(QUESTIONS).map_err(|e| e.to_string())?;
    let expected: Expectations = serde_json::from_str(EXPECTATIONS).map_err(|e| e.to_string())?;
    validate_fixtures(index.source(), &questions, &expected)?;
    let mut cases = Vec::new();
    let mut dense_coverage = Vec::new();
    let mut keyword_coverage = Vec::new();
    for question in &questions {
        let gold = expected
            .cases
            .iter()
            .find(|case| case.question_id == question.id)
            .ok_or("Missing question labels")?;
        let dense = index.search_with_top_k(&question.text, top_k)?;
        let keyword = keyword_search(index.source(), &question.text, top_k)?;
        let dense_ids: Vec<_> = dense
            .hits
            .iter()
            .map(|hit| hit.section.id.clone())
            .collect();
        let keyword_ids: Vec<_> = keyword.iter().map(|hit| hit.section.id.clone()).collect();
        let dense_score = coverage(&gold.required_source_ids, &dense_ids);
        let keyword_score = coverage(&gold.required_source_ids, &keyword_ids);
        if let Some(score) = dense_score {
            dense_coverage.push(score);
        }
        if let Some(score) = keyword_score {
            keyword_coverage.push(score);
        }
        let missing: Vec<_> = gold
            .required_source_ids
            .iter()
            .filter(|id| !dense_ids.contains(id))
            .collect();
        cases.push(json!({
            "question_id":question.id,"question":question.text,"required_source_ids":gold.required_source_ids,
            "dense":{"hits":dense.hits,"source_section_coverage":dense_score,"missing_source_ids":missing,"scoring":"cosine similarity, not probability"},
            "keyword":{"hits":keyword,"source_section_coverage":keyword_score,"scoring":"Jaccard overlap of lowercase alphanumeric tokens; no stopword tuning"},
            "expected_unknown_flags":gold.unknown_flags,
            "unknown_handling_assessment":if gold.unknown_flags.is_empty(){"not_applicable"}else{"requires human or future answer-level review; retrieval does not infer facts"},
        }));
    }
    fn mean(values: &[f64]) -> Option<f64> {
        if values.is_empty() {
            None
        } else {
            Some(values.iter().sum::<f64>() / values.len() as f64)
        }
    }
    Ok(json!({
        "report_version":"one-document-source-evidence-v1","manifest":index.manifest(),"top_k":top_k,
        "questions_sha256":hash(QUESTIONS.as_bytes()),"labels_sha256":hash(EXPECTATIONS.as_bytes()),
        "run_unix_seconds":std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e|e.to_string())?.as_secs(),
        "summary":{"question_count":questions.len(),"cases_with_source_labels":dense_coverage.len(),
            "dense_mean_source_section_coverage":mean(&dense_coverage),"keyword_mean_source_section_coverage":mean(&keyword_coverage),
            "unknown_cases_requiring_separate_review":expected.cases.iter().filter(|case|!case.unknown_flags.is_empty()).count(),
            "limitations":"Five learning questions only; no held-out set, quality gate, no-match threshold, eligibility or generated-answer assessment. Historical evidence coverage does not establish current availability."},
        "cases":cases
    }))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn coverage_counts_distinct_source_evidence_not_repeated_hits() {
        let expected = vec!["S2".into(), "S3".into()];
        assert_eq!(
            coverage(&expected, &["S2".into(), "S2".into(), "S1".into()]),
            Some(0.5)
        );
        assert_eq!(coverage(&[], &["S1".into()]), None);
    }
    #[test]
    fn labels_cannot_silently_target_another_source_or_nonexistent_evidence() {
        let source = meedoen_retrieval::source::parse_source(meedoen_retrieval::DOCUMENT).unwrap();
        let questions: Vec<Question> = serde_json::from_str(QUESTIONS).unwrap();
        let mut expected: Expectations = serde_json::from_str(EXPECTATIONS).unwrap();
        expected.document_id = "OTHER".into();
        assert!(validate_fixtures(&source, &questions, &expected).is_err());
        expected.document_id = source.id.clone();
        expected.cases[0].required_source_ids.push("S99".into());
        assert!(validate_fixtures(&source, &questions, &expected).is_err());
    }
}
