use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use super::{assessment::AssessmentRecord, records::validate_record, types::StoredRecord};

#[derive(Serialize)]
struct ReportRow<'a> {
    record: &'a super::types::RetrievalRecord,
    record_sha256: &'a str,
    assessments: Vec<&'a AssessmentRecord>,
    task_metrics: super::task_metrics::TaskMetrics,
    reference_metadata: Option<&'a super::reference::ReferenceMetadata>,
    dataset_info: serde_json::Value,
    scenario: serde_json::Value,
}

#[derive(Serialize)]
struct ReportData<'a> {
    schema_version: u32,
    artifact_id: String,
    record_hash_policy: &'static str,
    assessor_version: &'static str,
    task_metrics_version: &'static str,
    shared_preparations: Vec<BatchPreparation<'a>>,
    rows: Vec<ReportRow<'a>>,
}

#[derive(Serialize)]
struct BatchPreparation<'a> {
    batch_id: &'a str,
    summary: &'a super::types::SharedPreparation,
}

fn scenario(record: &super::types::RetrievalRecord) -> serde_json::Value {
    use crate::retrieval::{QueryFilters, SelectionMode};
    let config = &record.config.retrieval;
    let standard = config.candidate_k == 30
        && config.context_token_limit == 1200
        && !config.mmr
        && config.rrf_k == 60.0
        && config.vector_weight == 0.7
        && config.mmr_lambda == 0.7;
    let (mut id, mut label) = match (config.selection, config.top_k, config.apply_filters) {
        (SelectionMode::Sections, 3, false) => ("baseline", "Secties · top 3"),
        (SelectionMode::ActivityExpansion, 3, false) => {
            ("activity_expansion", "Activiteitcontext · top 3")
        }
        (SelectionMode::Sections, 6, false) => ("sections_k6", "Secties · top 6"),
        (SelectionMode::ActivityExpansion, 3, true) => (
            "filtered_expansion",
            "Activiteitcontext · gebruikersfilters",
        ),
        _ => ("custom", "Aangepaste instellingen"),
    };
    if !standard {
        id = "custom";
        label = "Aangepaste instellingen";
    }
    let effect = if !config.apply_filters {
        "disabled"
    } else if record.request_filters == QueryFilters::default() {
        "no_filters_requested"
    } else {
        match record.outcome.as_ref() {
            None => "not_measured",
            Some(outcome)
                if outcome.filters.applied
                    && record.preparation.layout.as_ref().is_some_and(|layout| {
                        outcome.work.eligible_sections < layout.canonical_sections
                    }) =>
            {
                "reduced_candidates"
            }
            Some(_) => "requested_no_effect",
        }
    };
    serde_json::json!({"id":id,"label":label,"filter_effect":effect,"query_inputs_sha256":record.query_inputs_sha256})
}

pub fn render_html(
    records: &[StoredRecord],
    assessments: &[AssessmentRecord],
) -> Result<String, String> {
    render_html_with_context(
        records,
        assessments,
        &super::reference::ExpectationsContext::load("legacy")?,
    )
}

pub fn render_html_with_context(
    records: &[StoredRecord],
    assessments: &[AssessmentRecord],
    context: &super::reference::ExpectationsContext,
) -> Result<String, String> {
    super::assessment::validate_bindings_with_context(records, assessments, context)?;
    let computed = if records.is_empty() {
        Vec::new()
    } else {
        super::assessment::assess_with_context(
            records,
            &[super::assessment::AssessmentKind::Deterministic],
            context,
        )?
    };
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
        task_metrics_version: super::task_metrics::TASK_METRICS_VERSION,
        shared_preparations: shared
            .into_iter()
            .map(|((batch_id, _), summary)| BatchPreparation { batch_id, summary })
            .collect(),
        rows: records
            .iter()
            .zip(&computed)
            .map(|(stored, computed)| ReportRow {
                record: &stored.record,
                record_sha256: &stored.record_sha256,
                assessments: assessments
                    .iter()
                    .filter(|assessment| assessment.run_id == stored.record.run_id)
                    .chain((!assessments.iter().any(|assessment| assessment.run_id == stored.record.run_id && assessment.kind == super::assessment::AssessmentKind::Deterministic)).then_some(computed))
                    .collect(),
                reference_metadata: context.metadata.get(&stored.record.case_id),
                dataset_info: serde_json::json!({"id": context.dataset.id, "provenance": if context.dataset.id == "legacy" {"previously_seen_regression"} else {"agent_authored_and_independently_agent_reviewed_not_human"}, "limitation": context.dataset.catalog.identity.limitation, "freeze_sha256": (context.dataset.id != "legacy").then_some(super::dataset::FROZEN_SHA256)}),
                scenario: scenario(&stored.record),
                task_metrics: super::task_metrics::derive(
                    &stored.record,
                    computed.deterministic.as_ref().unwrap(),
                ),
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
        let original_question = input.record.question.clone();
        input.record.question = format!("Wat betekent {injection}?");
        input.record.outcome.as_mut().unwrap().query = input.record.question.clone();
        input.record_sha256 = crate::manifest::hash(&serde_json::to_vec(&input.record).unwrap());
        assert!(
            render_html(&[input.clone()], &[]).is_err(),
            "A changed question is rejected by the selected dataset boundary"
        );
        input.record.question = original_question;
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

    fn report_data_for(case_id: &str) -> serde_json::Value {
        let record = crate::retrieval_lab::records::fixture_record_for(case_id);
        let stored = StoredRecord {
            record_sha256: crate::manifest::hash(&serde_json::to_vec(&record).unwrap()),
            record,
        };
        // Derived task measurements must also be available without imported assessments.
        let html = render_html(&[stored], &[]).unwrap();
        serde_json::from_str(
            html.split("id=\"report-data\" type=\"application/json\">")
                .nth(1)
                .unwrap()
                .split("</script>")
                .next()
                .unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn missing_evidence_is_zero_but_targetless_unknowns_are_pending_not_passed() {
        let missing = report_data_for("T02");
        let metrics = &missing["rows"][0]["task_metrics"];
        assert_eq!(metrics["complete_goal_set"], false);
        assert_eq!(metrics["covered_goals"], 0);
        assert_eq!(metrics["target_goals"], 2);
        let unknown = report_data_for("P05");
        let metrics = &unknown["rows"][0]["task_metrics"];
        assert!(metrics["complete_goal_set"].is_null());
        assert_eq!(metrics["target_goals"], 0);
        assert_eq!(metrics["answer_review"], "not_generated");
        assert_eq!(metrics["unknown_obligations"], 1);
        assert_eq!(unknown["task_metrics_version"], "retrieval-task-metrics-v1");
    }

    #[test]
    fn evidence_capacity_exposes_an_impossible_three_section_baseline() {
        let data = report_data_for("T05");
        let metrics = &data["rows"][0]["task_metrics"];
        assert_eq!(metrics["minimum_required_sections"], 4);
        assert_eq!(metrics["section_capacity_limit"], 3);
        assert_eq!(metrics["goal_set_exceeds_capacity"], true);
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

#[cfg(test)]
mod reference_visibility_tests {
    #[test]
    fn a_report_without_imported_grades_exposes_fresh_expected_facts() {
        let record = super::super::records::fixture_record();
        let stored = super::super::types::StoredRecord {
            record_sha256: crate::manifest::hash(&serde_json::to_vec(&record).unwrap()),
            record,
        };
        let html = super::render_html(&[stored], &[]).unwrap();
        let json = html
            .split("id=\"report-data\" type=\"application/json\">")
            .nth(1)
            .unwrap()
            .split("</script>")
            .next()
            .unwrap();
        let data: serde_json::Value = serde_json::from_str(json).unwrap();
        assert!(
            data["rows"][0]["assessments"]
                .as_array()
                .unwrap()
                .iter()
                .any(|grade| grade["kind"] == "deterministic"
                    && grade["deterministic"]["expected_facts"]
                        .as_array()
                        .is_some_and(|facts| !facts.is_empty())),
            "fresh expectations must be visible without an imported grade artifact"
        );
    }
}
