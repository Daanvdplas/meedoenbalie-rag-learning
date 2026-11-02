use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use super::{
    assessment::{AssessmentRecord, validate_bindings},
    records::validate_record,
    types::StoredRecord,
};

#[derive(Serialize)]
struct ReportRow<'a> {
    record: &'a super::types::RetrievalRecord,
    record_sha256: &'a str,
    assessments: Vec<&'a AssessmentRecord>,
}

#[derive(Serialize)]
struct ReportData<'a> {
    schema_version: u32,
    artifact_id: String,
    record_hash_policy: &'static str,
    assessor_version: &'static str,
    shared_preparations: Vec<BatchPreparation<'a>>,
    rows: Vec<ReportRow<'a>>,
}

#[derive(Serialize)]
struct BatchPreparation<'a> {
    batch_id: &'a str,
    summary: &'a super::types::SharedPreparation,
}

pub fn render_html(
    records: &[StoredRecord],
    assessments: &[AssessmentRecord],
) -> Result<String, String> {
    validate_bindings(records, assessments)?;
    let mut ids = BTreeSet::new();
    for stored in records {
        validate_record(&stored.record)?;
        if !ids.insert(stored.record.run_id.as_str())
            || stored.record_sha256.len() != 64
            || !stored
                .record_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err("Report requires distinct run IDs and exact stored-record hashes".into());
        }
    }
    let identity: Vec<_> = records
        .iter()
        .map(|stored| (&stored.record.run_id, &stored.record_sha256))
        .collect();
    let identity = serde_json::to_vec(&identity).map_err(|error| error.to_string())?;
    let mut shared = BTreeMap::new();
    for stored in records {
        if let Some(summary) = &stored.record.shared_preparation {
            let key = (
                stored.record.batch_id.as_str(),
                summary.preparation_id.as_str(),
            );
            if let Some(previous) = shared.insert(key, summary)
                && serde_json::to_value(previous).map_err(|error| error.to_string())?
                    != serde_json::to_value(summary).map_err(|error| error.to_string())?
            {
                return Err("Copies of a shared batch preparation disagree".into());
            }
        }
    }
    let data = ReportData {
        schema_version: 1,
        artifact_id: crate::manifest::hash(&identity),
        record_hash_policy: super::types::RECORD_HASH_POLICY,
        assessor_version: super::assessment::ASSESSOR_VERSION,
        shared_preparations: shared
            .into_iter()
            .map(|((batch_id, _), summary)| BatchPreparation { batch_id, summary })
            .collect(),
        rows: records
            .iter()
            .map(|stored| ReportRow {
                record: &stored.record,
                record_sha256: &stored.record_sha256,
                assessments: assessments
                    .iter()
                    .filter(|assessment| assessment.run_id == stored.record.run_id)
                    .collect(),
            })
            .collect(),
    };
    let data = serde_json::to_string(&data)
        .map_err(|error| error.to_string())?
        .replace('&', "\\u0026")
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029");
    Ok(include_str!("report.html")
        .replace("__REPORT_STYLE__", include_str!("report.css"))
        .replace("__REPORT_SCRIPT__", include_str!("report.js"))
        .replace("__REPORT_DATA__", &data))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::retrieval_lab::{
        assessment::{AssessmentKind, assess},
        records::fixture_record,
    };

    fn stored() -> StoredRecord {
        let record = fixture_record();
        let record_sha256 = crate::manifest::hash(&serde_json::to_vec(&record).unwrap());
        StoredRecord {
            record,
            record_sha256,
        }
    }

    #[test]
    fn empty_report_is_an_offline_empty_state() {
        let html = render_html(&[], &[]).unwrap();
        assert!(html.contains("Nog geen uitvoeringen"));
        assert!(html.contains("lang=\"nl\""));
        assert!(!html.contains("src=\"http"));
        assert!(!html.contains("href=\"http"));
    }

    #[test]
    fn source_text_and_question_cannot_escape_the_embedded_data() {
        let mut input = stored();
        let injection = "</script><script>window.reportInjected=true</script>&\u{2028}";
        input.record.question = format!("Wat betekent {injection}?");
        input.record.outcome.as_mut().unwrap().query = input.record.question.clone();
        input.record.outcome.as_mut().unwrap().hits[0].section.text = injection.into();
        input.record_sha256 = crate::manifest::hash(&serde_json::to_vec(&input.record).unwrap());
        let html = render_html(&[input], &[]).unwrap();
        assert!(!html.contains(injection));
        assert!(html.contains("\\u003c/script\\u003e"));
        let json = html
            .split("id=\"report-data\" type=\"application/json\">")
            .nth(1)
            .unwrap()
            .split("</script>")
            .next()
            .unwrap();
        let data: serde_json::Value = serde_json::from_str(json).unwrap();
        assert_eq!(
            data["rows"][0]["record"]["outcome"]["hits"][0]["section"]["text"],
            injection
        );
    }

    #[test]
    fn report_keeps_immutable_bindings_and_ungraded_states() {
        let input = stored();
        let kinds = [
            AssessmentKind::Deterministic,
            AssessmentKind::Manual,
            AssessmentKind::Ai,
        ];
        let assessments = assess(std::slice::from_ref(&input), &kinds).unwrap();
        let html = render_html(std::slice::from_ref(&input), &assessments).unwrap();
        assert!(html.contains(&input.record_sha256));
        assert!(html.contains("\"status\":\"unreviewed\""));
        assert!(html.contains("\"status\":\"paused\""));
        assert!(html.contains("\"id\":\"answer_correctness\",\"value\":null"));
        let mut stale = assessments;
        stale[0].retrieval_sha256 = "a".repeat(64);
        assert!(render_html(&[input], &stale).is_err());
    }

    #[test]
    fn shared_preparation_is_reported_once_and_conflicting_copies_are_rejected() {
        use crate::retrieval_lab::types::SharedPreparation;
        let mut first = stored();
        first.record.shared_preparation = Some(SharedPreparation {
            preparation_id: "shared-fixture".into(),
            source_parse_ms: 1.0,
            model_identity_ms: Some(2.0),
            model_load_ms: Some(3.0),
            document_inference_ms: Some(4.0),
            query_inference_ms: Some(5.0),
            document_inputs_encoded: 30,
            query_inputs_encoded: 18,
            scoring_path: "same Rust scorer".into(),
            interpretation: "one shared preparation".into(),
        });
        first.record_sha256 = crate::manifest::hash(&serde_json::to_vec(&first.record).unwrap());
        let mut second = first.clone();
        second.record.run_id.push_str("-second");
        second.record_sha256 = crate::manifest::hash(&serde_json::to_vec(&second.record).unwrap());
        let html = render_html(&[first.clone(), second.clone()], &[]).unwrap();
        let json = html
            .split("id=\"report-data\" type=\"application/json\">")
            .nth(1)
            .unwrap()
            .split("</script>")
            .next()
            .unwrap();
        let data: serde_json::Value = serde_json::from_str(json).unwrap();
        assert_eq!(data["shared_preparations"].as_array().unwrap().len(), 1);
        assert_eq!(
            data["shared_preparations"][0]["summary"]["document_inputs_encoded"],
            30
        );
        second
            .record
            .shared_preparation
            .as_mut()
            .unwrap()
            .document_inputs_encoded = 60;
        second.record_sha256 = crate::manifest::hash(&serde_json::to_vec(&second.record).unwrap());
        assert!(render_html(&[first, second], &[]).is_err());
    }
}
