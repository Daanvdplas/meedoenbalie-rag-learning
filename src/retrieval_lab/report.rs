use std::collections::BTreeSet;

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
    rows: Vec<ReportRow<'a>>,
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
    let data = ReportData {
        schema_version: 1,
        artifact_id: crate::manifest::hash(&identity),
        record_hash_policy: super::types::RECORD_HASH_POLICY,
        assessor_version: super::assessment::ASSESSOR_VERSION,
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
}
