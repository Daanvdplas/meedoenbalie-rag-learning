use super::{
    dataset::{CaseCriteria, SectionRef},
    types::{Arm, ExecutionKind, ExecutionStatus, ExperimentRecord},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HardCheck {
    pub id: String,
    pub passed: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Metric {
    pub id: String,
    pub value: Option<f64>,
    pub numerator: Option<usize>,
    pub denominator: Option<usize>,
    pub direction: String,
    pub definition: String,
    pub not_applicable_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetricsReport {
    pub schema_version: u32,
    pub hard_checks: Vec<HardCheck>,
    pub hard_checks_passed: bool,
    pub metrics: Vec<Metric>,
}

pub fn validate_report(report: &MetricsReport) -> Result<(), String> {
    let checks = [
        "execution_completed",
        "live_trace",
        "artifact_integrity",
        "canonical_evidence",
        "retrieval_trace",
    ];
    let metric_ids = [
        "first_hop_fact_coverage",
        "first_hop_precision",
        "first_hop_section_recall",
        "all_hop_fact_coverage",
        "all_hop_precision",
        "all_hop_section_recall",
        "first_relevant_reciprocal_rank",
        "offered_fact_coverage",
        "search_calls",
        "mcp_calls",
        "elapsed_ms",
        "input_tokens",
        "output_tokens",
    ];
    if report.schema_version != 1
        || report.hard_checks.len() != checks.len()
        || report
            .hard_checks
            .iter()
            .map(|c| c.id.as_str())
            .collect::<BTreeSet<_>>()
            != checks.into_iter().collect()
        || report
            .hard_checks
            .iter()
            .any(|c| c.detail.trim().is_empty())
        || report.hard_checks_passed != report.hard_checks.iter().all(|c| c.passed)
        || report.metrics.len() != metric_ids.len()
        || report
            .metrics
            .iter()
            .map(|m| m.id.as_str())
            .collect::<BTreeSet<_>>()
            != metric_ids.into_iter().collect()
    {
        return Err("Invalid metric report schema or required measures".into());
    }
    for metric in &report.metrics {
        if metric.definition.trim().is_empty()
            || !matches!(
                metric.direction.as_str(),
                "higher_is_better" | "lower_is_better" | "diagnostic"
            )
            || metric.numerator.is_some() != metric.denominator.is_some()
            || metric.value.is_some_and(|v| !v.is_finite() || v < 0.0)
            || metric.value.is_some() == metric.not_applicable_reason.is_some()
            || metric
                .not_applicable_reason
                .as_deref()
                .is_some_and(|s| s.trim().is_empty())
        {
            return Err("Invalid metric applicability or numeric value".into());
        }
        if let (Some(n), Some(d)) = (metric.numerator, metric.denominator)
            && (n > d
                || (d == 0 && metric.value.is_some())
                || metric
                    .value
                    .is_some_and(|v| (v - n as f64 / d as f64).abs() > 1e-12))
        {
            return Err("Metric value disagrees with its numerator and denominator".into());
        }
        let is_ratio = metric.id.contains("coverage")
            || metric.id.contains("precision")
            || metric.id.contains("recall")
            || metric.id == "first_relevant_reciprocal_rank";
        if is_ratio
            && (metric
                .value
                .is_some_and(|v| v > 1.0 || metric.denominator.is_none())
                || metric.direction != "higher_is_better")
        {
            return Err("Metric ratio must use its documented zero-to-one scale".into());
        }
    }
    Ok(())
}

fn references<'a>(sections: impl Iterator<Item = &'a crate::Section>) -> BTreeSet<SectionRef> {
    sections
        .map(|s| SectionRef {
            source_id: s.source_id.clone(),
            section_id: s.id.clone(),
        })
        .collect()
}

fn ratio(id: &str, numerator: usize, denominator: usize, definition: &str) -> Metric {
    Metric {
        id: id.into(),
        value: (denominator > 0).then(|| numerator as f64 / denominator as f64),
        numerator: Some(numerator),
        denominator: Some(denominator),
        direction: "higher_is_better".into(),
        definition: definition.into(),
        not_applicable_reason: (denominator == 0)
            .then(|| "The metric denominator is zero; see its definition".into()),
    }
}

fn coverage(id: &str, sections: &BTreeSet<SectionRef>, criteria: &CaseCriteria) -> Metric {
    let facts: Vec<_> = criteria
        .facts
        .iter()
        .filter(|f| !f.evidence_alternatives.is_empty())
        .collect();
    let covered = facts
        .iter()
        .filter(|fact| {
            fact.evidence_alternatives
                .iter()
                .any(|group| group.iter().all(|s| sections.contains(s)))
        })
        .count();
    ratio(
        id,
        covered,
        facts.len(),
        "Fraction of source-supported fact criteria with at least one complete alternative evidence group; sections deduplicated by document and section",
    )
}

fn unavailable(mut metric: Metric, reason: &str) -> Metric {
    metric.value = None;
    metric.numerator = None;
    metric.denominator = None;
    metric.not_applicable_reason = Some(reason.into());
    metric
}

fn count(id: &str, value: Option<u64>, direction: &str, definition: &str) -> Metric {
    Metric {
        id: id.into(),
        value: value.map(|v| v as f64),
        numerator: None,
        denominator: None,
        direction: direction.into(),
        definition: definition.into(),
        not_applicable_reason: value
            .is_none()
            .then(|| "Provider did not expose usage".into()),
    }
}

fn exact_section(section: &crate::Section, corpus: &[crate::Section]) -> bool {
    corpus.iter().any(|s| {
        s.source_id == section.source_id
            && s.id == section.id
            && s.source_date == section.source_date
            && s.source_title == section.source_title
            && s.heading == section.heading
            && s.text == section.text
    })
}

fn trace_contract(record: &ExperimentRecord) -> Result<(), String> {
    let offered = references(record.offered_sections.iter());
    if offered.len() != record.offered_sections.len() {
        return Err("Offered context contains duplicate section identities".into());
    }
    if record.arm == Arm::FullContext {
        return if record.retrieval_steps.is_empty()
            && record.tool_calls.is_empty()
            && offered.len() == 30
        {
            Ok(())
        } else {
            Err("Full-context arm must offer the thirty source sections without retrieval".into())
        };
    }
    let retrieved = references(
        record
            .retrieval_steps
            .iter()
            .flat_map(|s| s.hits.iter().map(|h| &h.section)),
    );
    if offered != retrieved {
        return Err("Offered sections differ from the union of actual retrieval results".into());
    }
    if record.arm != Arm::AgenticDense {
        let expected_kind = if record.arm == Arm::Bm25 {
            "bm25"
        } else {
            "cosine"
        };
        return if record.tool_calls.is_empty()
            && record.retrieval_steps.len() == 1
            && record.retrieval_steps[0].origin == "preselection"
            && record.retrieval_steps[0]
                .hits
                .iter()
                .all(|h| h.score_kind == expected_kind)
        {
            Ok(())
        } else {
            Err("Single-search arm must record one local preselection and no MCP call".into())
        };
    }
    if record.tool_calls.is_empty()
        || record.tool_calls.len() > record.manifest.limits.max_search_calls
    {
        return Err("Agent must record one to four actual MCP calls".into());
    }
    let expected_sources = serde_json::to_value(&record.manifest.sources)
        .map_err(|_| "Cannot encode source identities")?;
    let mut completed = 0;
    let mut first_manifest = None;
    for call in &record.tool_calls {
        if call.server != "coa"
            || call.tool != "search_document"
            || crate::validate_query(&call.query).is_err()
            || !(1..=6).contains(&call.top_k)
        {
            return Err("Unexpected tool or invalid search arguments".into());
        }
        if call.status == "failed" && call.result.is_none() {
            continue;
        }
        if call.status != "completed" || call.error.is_some() {
            return Err("Invalid MCP completion status".into());
        }
        let result = call
            .result
            .as_ref()
            .ok_or("Successful MCP call is missing its actual result")?;
        let step = record
            .retrieval_steps
            .get(completed)
            .ok_or("Successful MCP call lacks a retrieval step")?;
        completed += 1;
        if step.origin != "mcp"
            || step.query != call.query
            || step.top_k != call.top_k
            || result["question"] != call.query
            || result["top_k"].as_u64() != Some(call.top_k as u64)
            || result["as_of"] != record.manifest.as_of
            || result["sources"] != expected_sources
            || result["manifest"]["source_sha256"] != record.manifest.corpus_sha256
            || result["manifest"]["as_of"] != record.manifest.as_of
            || result["manifest"]["sources"] != expected_sources
        {
            return Err("MCP result, step and pinned source manifest disagree".into());
        }
        if first_manifest.is_some_and(|m| m != &result["manifest"]) {
            return Err("Index manifest changed between MCP calls".into());
        }
        first_manifest = Some(&result["manifest"]);
        let hits = result["hits"]
            .as_array()
            .ok_or("MCP result has no ranked passages")?;
        if hits.len() != step.hits.len()
            || hits.iter().zip(&step.hits).any(|(raw, hit)| {
                hit.score_kind != "cosine"
                    || raw["similarity"].as_f64() != Some(hit.score)
                    || serde_json::to_value(&hit.section)
                        .is_ok_and(|section| raw["section"] != section)
            })
        {
            return Err("Retrieval step differs from actual MCP passages or scores".into());
        }
    }
    if completed == 0 || completed != record.retrieval_steps.len() {
        return Err("Agent has no completed retrieval or has invented retrieval steps".into());
    }
    Ok(())
}

pub fn evaluate(
    record: &ExperimentRecord,
    criteria: &CaseCriteria,
) -> Result<MetricsReport, String> {
    if record.case_id != criteria.case_id {
        return Err("Criteria belong to a different evaluation case".into());
    }
    let corpus: Vec<_> = crate::source::parse_sources(crate::DOCUMENTS)?
        .into_iter()
        .flat_map(|s| s.sections)
        .collect();
    let raw_all = references(
        record
            .retrieval_steps
            .iter()
            .flat_map(|s| s.hits.iter().map(|h| &h.section)),
    );
    let all = references(
        record
            .retrieval_steps
            .iter()
            .flat_map(|s| s.hits.iter().map(|h| &h.section))
            .filter(|s| exact_section(s, &corpus)),
    );
    let first_step = record.retrieval_steps.first();
    let first_failed = record.arm == Arm::AgenticDense
        && record
            .tool_calls
            .first()
            .is_some_and(|c| c.status != "completed");
    let raw_first = if first_failed {
        BTreeSet::new()
    } else {
        references(
            first_step
                .into_iter()
                .flat_map(|s| s.hits.iter().map(|h| &h.section)),
        )
    };
    let first = if first_failed {
        BTreeSet::new()
    } else {
        references(
            first_step
                .into_iter()
                .flat_map(|s| s.hits.iter().map(|h| &h.section))
                .filter(|s| exact_section(s, &corpus)),
        )
    };
    let relevant: BTreeSet<_> = criteria.relevant_sections.iter().cloned().collect();
    let mut metrics = Vec::new();
    for (prefix, sections, raw_sections) in [
        ("first_hop", &first, &raw_first),
        ("all_hop", &all, &raw_all),
    ] {
        metrics.push(coverage(
            &format!("{prefix}_fact_coverage"),
            sections,
            criteria,
        ));
        let correct = sections.intersection(&relevant).count();
        let precision = ratio(
            &format!("{prefix}_precision"),
            correct,
            raw_sections.len(),
            "Unique relevant, canonical document/section pairs divided by all unique returned pairs; requires complete section labels",
        );
        let recall = ratio(
            &format!("{prefix}_section_recall"),
            correct,
            relevant.len(),
            "Unique relevant returned document/section pairs divided by all labelled relevant pairs, including useful exclusion evidence; requires complete labels",
        );
        metrics.push(if criteria.relevance_labels_complete {
            precision
        } else {
            unavailable(precision, "Section relevance labels are incomplete")
        });
        metrics.push(if criteria.relevance_labels_complete {
            recall
        } else {
            unavailable(recall, "Section relevance labels are incomplete")
        });
    }
    let rank = if first_failed {
        None
    } else {
        first_step
            .and_then(|step| {
                step.hits.iter().position(|h| {
                    exact_section(&h.section, &corpus)
                        && relevant.contains(&SectionRef {
                            source_id: h.section.source_id.clone(),
                            section_id: h.section.id.clone(),
                        })
                })
            })
            .map(|p| p + 1)
    };
    let mut rr = ratio(
        "first_relevant_reciprocal_rank",
        usize::from(rank.is_some()),
        rank.unwrap_or(1),
        "Reciprocal original hit position of the first relevant section in the first retrieval attempt; zero if no relevant hit, undefined without complete nonempty relevance labels",
    );
    if !criteria.relevance_labels_complete || relevant.is_empty() {
        rr = unavailable(rr, "Ranking requires complete, nonempty relevance labels");
    }
    metrics.push(rr);
    if record.arm == Arm::FullContext {
        for metric in &mut metrics {
            *metric = unavailable(metric.clone(), "Full-context arm performs no retrieval");
        }
    } else if record.execution_status != ExecutionStatus::Completed
        && record.retrieval_steps.is_empty()
        && record.tool_calls.is_empty()
    {
        for metric in &mut metrics {
            *metric = unavailable(
                metric.clone(),
                "No retrieval attempt was observed before execution ended",
            );
        }
    }
    metrics.push(coverage(
        "offered_fact_coverage",
        &references(
            record
                .offered_sections
                .iter()
                .filter(|s| exact_section(s, &corpus)),
        ),
        criteria,
    ));
    metrics.push(count("search_calls", Some(if record.arm == Arm::AgenticDense { record.tool_calls.len() } else { record.retrieval_steps.len() } as u64), "diagnostic", "Actual retrieval requests, including failed MCP calls; quality has no one-call success rule"));
    metrics.push(count(
        "mcp_calls",
        Some(record.tool_calls.len() as u64),
        "diagnostic",
        "Actual MCP calls only; local preselection is not an MCP call",
    ));
    metrics.push(count("elapsed_ms", Some(record.elapsed_ms), "lower_is_better", "Recorded wall-clock milliseconds for this execution; not an aggregate concurrency estimate"));
    metrics.push(count(
        "input_tokens",
        record.usage.as_ref().map(|u| u.input_tokens),
        "diagnostic",
        "Provider-reported input tokens; unknown usage is not zero",
    ));
    metrics.push(count(
        "output_tokens",
        record.usage.as_ref().map(|u| u.output_tokens),
        "diagnostic",
        "Provider-reported output tokens; unknown usage is not zero",
    ));
    let mut without_assessments = record.clone();
    without_assessments.metrics = None;
    without_assessments.judgement = None;
    let integrity = super::runner::validate_record(&without_assessments);
    let trace = trace_contract(record);
    let hard_checks = vec![
        HardCheck { id: "execution_completed".into(), passed: record.execution_status == ExecutionStatus::Completed, detail: "Only completed executions may satisfy quality prerequisites".into() },
        HardCheck { id: "live_trace".into(), passed: record.execution_kind == ExecutionKind::Live && record.trace_complete, detail: "A complete live trace is required; fixtures are instrument tests, never measured model quality".into() },
        HardCheck { id: "artifact_integrity".into(), passed: integrity.is_ok(), detail: integrity.err().unwrap_or_else(|| "Record schema, fingerprints, status, limits and literal citations are valid".into()) },
        HardCheck { id: "canonical_evidence".into(), passed: record.offered_sections.iter().chain(record.retrieval_steps.iter().flat_map(|s| s.hits.iter().map(|h| &h.section))).all(|s| exact_section(s, &corpus)), detail: "Every section must match its pinned document, date, title, heading and exact text".into() },
        HardCheck { id: "retrieval_trace".into(), passed: trace.is_ok(), detail: trace.err().unwrap_or_else(|| "Actual tool results, arm contract and offered evidence agree".into()) },
    ];
    Ok(MetricsReport {
        schema_version: 1,
        hard_checks_passed: hard_checks.iter().all(|c| c.passed),
        hard_checks,
        metrics,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::experiment::{dataset, types::*};
    use serde_json::json;

    fn section(source: &str, id: &str) -> crate::Section {
        crate::source::parse_sources(crate::DOCUMENTS)
            .unwrap()
            .into_iter()
            .flat_map(|s| s.sections)
            .find(|s| s.source_id == source && s.id == id)
            .unwrap()
    }

    fn record(case_id: &str, hits: Vec<crate::Section>) -> (ExperimentRecord, CaseCriteria) {
        let (questions, labels) = dataset::validate_bundled().unwrap();
        let question = questions.cases.iter().find(|c| c.id == case_id).unwrap();
        let criteria = labels
            .cases
            .iter()
            .find(|c| c.case_id == case_id)
            .unwrap()
            .clone();
        let manifest = json!({"schema_version":1,"experiment_id":"test","created_at_unix_ms":1,"questions_version":questions.dataset_version,"questions_sha256":dataset::questions_hash(&questions).unwrap(),"expectations_version":labels.dataset_version,"expectations_sha256":"b".repeat(64),"corpus_sha256":questions.corpus_sha256,"sources":crate::source::parse_sources(crate::DOCUMENTS).unwrap().iter().zip(crate::DOCUMENTS).map(|(s,raw)|json!({"id":s.id,"title":s.title,"date":s.date,"sha256":crate::manifest::hash(raw.as_bytes())})).collect::<Vec<_>>(),"as_of":crate::AS_OF,"generation_prompt_version":"v1","generation_prompt_sha256":"c".repeat(64),"rubric_version":"v1","rubric_sha256":"d".repeat(64),"model":"test-model","client_version":"codex-cli test","binary_sha256":"e".repeat(64),"runtime_sha256":"f".repeat(64),"code_mode_host_sha256":"a".repeat(64),"generator_route":"OpenAI via locally authenticated Codex CLI","grader_route":"fixture","embedding_model":crate::MODEL,"embedding_files":[],"reproducibility_limitations":["fixture"],"limits":{"max_cases":12,"max_generations":48,"max_judgements":48,"max_seconds":600,"per_call_seconds":120,"concurrency":1,"max_search_calls":4,"top_k":3}});
        let value = json!({"schema_version":1,"run_id":"test","case_id":case_id,"family":question.family,"split":question.split,"kind":question.kind,"arm":"dense_once","execution_kind":"fixture","execution_status":"completed","question":question.question,"manifest":manifest,"answer":{"text":"Onbekend.","citations":[],"unknowns":["Onbekend"]},"tool_calls":[],"retrieval_steps":[{"origin":"preselection","query":question.question,"top_k":3,"hits":hits.iter().map(|s|json!({"section":s,"score":0.8,"score_kind":"cosine"})).collect::<Vec<_>>()}],"trace_complete":true,"offered_sections":hits,"elapsed_ms":12,"usage":null,"error":null,"metrics":null,"judgement":null});
        (serde_json::from_value(value).unwrap(), criteria)
    }

    fn metric<'a>(report: &'a MetricsReport, id: &str) -> &'a Metric {
        report.metrics.iter().find(|m| m.id == id).unwrap()
    }

    #[test]
    fn one_of_two_required_facts_is_half_even_when_the_section_is_repeated() {
        let time = section("ACT-TUIN-001", "S2");
        let (item, gold) = record("P01", vec![time.clone(), time]);
        let report = evaluate(&item, &gold).unwrap();
        let coverage = metric(&report, "all_hop_fact_coverage");
        assert_eq!(coverage.value, Some(0.5));
        assert_eq!(
            (coverage.numerator, coverage.denominator),
            (Some(1), Some(2))
        );
    }

    #[test]
    fn precision_is_two_thirds_and_first_relevant_rank_is_two() {
        let (item, gold) = record(
            "P01",
            vec![
                section("ACT-FIETS-002", "S2"),
                section("ACT-TUIN-001", "S2"),
                section("ACT-TUIN-001", "S3"),
            ],
        );
        let report = evaluate(&item, &gold).unwrap();
        assert_eq!(metric(&report, "all_hop_precision").value, Some(2.0 / 3.0));
        assert_eq!(
            metric(&report, "first_relevant_reciprocal_rank").value,
            Some(0.5)
        );
        assert_eq!(metric(&report, "all_hop_section_recall").value, Some(1.0));
    }

    #[test]
    fn empty_results_are_zero_with_gold_but_undefined_without_a_target() {
        let (item, gold) = record("P01", vec![]);
        let report = evaluate(&item, &gold).unwrap();
        assert_eq!(metric(&report, "all_hop_fact_coverage").value, Some(0.0));
        assert_eq!(metric(&report, "all_hop_section_recall").value, Some(0.0));
        assert_eq!(metric(&report, "all_hop_precision").value, None);
        let (item, gold) = record("P05", vec![section("ACT-TUIN-001", "S2")]);
        let report = evaluate(&item, &gold).unwrap();
        assert_eq!(metric(&report, "all_hop_fact_coverage").value, None);
        assert_eq!(metric(&report, "all_hop_section_recall").value, None);
        assert_eq!(metric(&report, "all_hop_precision").value, Some(0.0));
    }

    #[test]
    fn incomplete_labels_disable_precision_and_ranking() {
        let (item, gold) = record("P06", vec![section("ACT-WANDEL-005", "S1")]);
        let report = evaluate(&item, &gold).unwrap();
        assert_eq!(metric(&report, "all_hop_precision").value, None);
        assert_eq!(
            metric(&report, "first_relevant_reciprocal_rank").value,
            None
        );
        assert_eq!(metric(&report, "all_hop_fact_coverage").value, Some(0.5));
    }

    #[test]
    fn alternative_groups_are_or_but_each_group_requires_all_its_sections() {
        let (mut item, mut gold) = record("P01", vec![section("ACT-TUIN-001", "S2")]);
        gold.facts.truncate(1);
        let time = dataset::SectionRef {
            source_id: "ACT-TUIN-001".into(),
            section_id: "S2".into(),
        };
        let location = dataset::SectionRef {
            source_id: "ACT-TUIN-001".into(),
            section_id: "S3".into(),
        };
        gold.facts[0].evidence_alternatives = vec![
            vec![time, location],
            vec![dataset::SectionRef {
                source_id: "ACT-FIETS-002".into(),
                section_id: "S2".into(),
            }],
        ];
        assert_eq!(
            metric(&evaluate(&item, &gold).unwrap(), "all_hop_fact_coverage").value,
            Some(0.0)
        );
        item.retrieval_steps[0].hits.push(RankedSection {
            section: section("ACT-TUIN-001", "S3"),
            score: 0.7,
            score_kind: "cosine".into(),
        });
        assert_eq!(
            metric(&evaluate(&item, &gold).unwrap(), "all_hop_fact_coverage").value,
            Some(1.0)
        );
        item.retrieval_steps[0].hits = vec![RankedSection {
            section: section("ACT-FIETS-002", "S2"),
            score: 0.8,
            score_kind: "cosine".into(),
        }];
        assert_eq!(
            metric(&evaluate(&item, &gold).unwrap(), "all_hop_fact_coverage").value,
            Some(1.0)
        );
    }

    #[test]
    fn same_section_number_in_a_different_document_does_not_cover_the_fact() {
        let (item, gold) = record("P01", vec![section("ACT-FIETS-002", "S2")]);
        assert_eq!(
            metric(&evaluate(&item, &gold).unwrap(), "all_hop_fact_coverage").value,
            Some(0.0)
        );
    }

    #[test]
    fn later_hops_improve_coverage_without_inflating_duplicates_or_first_hop() {
        let (mut item, gold) = record("P01", vec![section("ACT-TUIN-001", "S2")]);
        let mut later = item.retrieval_steps[0].clone();
        later.hits.push(RankedSection {
            section: section("ACT-TUIN-001", "S3"),
            score: 0.7,
            score_kind: "cosine".into(),
        });
        item.retrieval_steps.push(later);
        let report = evaluate(&item, &gold).unwrap();
        assert_eq!(metric(&report, "first_hop_fact_coverage").value, Some(0.5));
        assert_eq!(metric(&report, "all_hop_fact_coverage").value, Some(1.0));
        assert_eq!(metric(&report, "all_hop_precision").denominator, Some(2));
    }

    #[test]
    fn full_context_has_offered_coverage_but_no_retrieval_scores() {
        let (mut item, gold) = record("P01", vec![]);
        item.arm = Arm::FullContext;
        item.retrieval_steps.clear();
        item.offered_sections = crate::source::parse_sources(crate::DOCUMENTS)
            .unwrap()
            .into_iter()
            .flat_map(|s| s.sections)
            .collect();
        let report = evaluate(&item, &gold).unwrap();
        assert_eq!(metric(&report, "all_hop_fact_coverage").value, None);
        assert_eq!(metric(&report, "offered_fact_coverage").value, Some(1.0));
        assert_eq!(metric(&report, "search_calls").value, Some(0.0));
        assert_eq!(metric(&report, "input_tokens").value, None);
    }

    #[test]
    fn failed_partial_execution_retains_observed_metrics_but_cannot_pass() {
        let (mut item, gold) = record("P01", vec![section("ACT-TUIN-001", "S2")]);
        item.execution_status = ExecutionStatus::TimedOut;
        item.answer = None;
        item.error = Some("timeout".into());
        item.trace_complete = false;
        let report = evaluate(&item, &gold).unwrap();
        assert_eq!(metric(&report, "all_hop_fact_coverage").value, Some(0.5));
        assert!(!report.hard_checks_passed);
        assert!(
            report
                .hard_checks
                .iter()
                .any(|c| c.id == "execution_completed" && !c.passed)
        );
    }

    fn live_agent() -> (ExperimentRecord, CaseCriteria) {
        let (mut item, gold) = record(
            "P01",
            vec![section("ACT-TUIN-001", "S2"), section("ACT-TUIN-001", "S3")],
        );
        item.arm = Arm::AgenticDense;
        item.execution_kind = ExecutionKind::Live;
        item.manifest.budget = Some(crate::experiment::budget::LedgerState {
            schema_version: 1,
            id: "fixture-ledger".into(),
            started_at_unix_ms: 1,
            max_generation_sessions: 40,
            max_grading_sessions: 56,
            max_seconds: 14400,
            generation_sessions_used: 1,
            grading_sessions_used: 0,
        });
        item.manifest.embedding_revision = Some("a".repeat(40));
        item.manifest.embedding_files = [
            "model_optimized.onnx",
            "tokenizer.json",
            "config.json",
            "tokenizer_config.json",
            "special_tokens_map.json",
        ]
        .into_iter()
        .map(|name| ModelFile {
            name: name.into(),
            sha256: "a".repeat(64),
        })
        .collect();
        item.manifest.generation_prompt_sha256 = crate::manifest::hash(
            crate::codex::generation_prompt(&item.question, item.manifest.limits.top_k).as_bytes(),
        );
        item.retrieval_steps[0].origin = "mcp".into();
        let sources = serde_json::to_value(&item.manifest.sources).unwrap();
        let result = json!({"question":item.question,"top_k":3,"model":crate::MODEL,"sources":sources,"as_of":crate::AS_OF,"manifest":{"source_sha256":item.manifest.corpus_sha256,"sources":sources,"as_of":crate::AS_OF},"hits":item.retrieval_steps[0].hits.iter().map(|h|json!({"section":h.section,"similarity":h.score})).collect::<Vec<_>>()});
        item.tool_calls.push(crate::codex::ToolCall {
            server: "coa".into(),
            tool: "search_document".into(),
            query: item.question.clone(),
            top_k: 3,
            status: "completed".into(),
            result: Some(result),
            error: None,
        });
        (item, gold)
    }

    #[test]
    fn only_complete_live_execution_with_matching_evidence_satisfies_hard_prerequisites() {
        let (mut item, gold) = live_agent();
        assert!(evaluate(&item, &gold).unwrap().hard_checks_passed);
        item.execution_kind = ExecutionKind::Fixture;
        assert!(!evaluate(&item, &gold).unwrap().hard_checks_passed);
        item.execution_kind = ExecutionKind::Live;
        item.trace_complete = false;
        assert!(!evaluate(&item, &gold).unwrap().hard_checks_passed);
    }

    #[test]
    fn wrong_date_or_changed_source_text_is_not_counted_as_retrieved_evidence() {
        let (mut item, gold) = live_agent();
        item.retrieval_steps[0].hits[0].section.source_date = "2026-10-19".into();
        let report = evaluate(&item, &gold).unwrap();
        assert!(!report.hard_checks_passed);
        assert_eq!(metric(&report, "all_hop_fact_coverage").value, Some(0.5));
        item.retrieval_steps[0].hits[1].section.text = "Invented location".into();
        assert_eq!(
            metric(&evaluate(&item, &gold).unwrap(), "all_hop_fact_coverage").value,
            Some(0.0)
        );
    }

    #[test]
    fn literal_citations_require_the_offered_document_section_and_date() {
        let (mut item, gold) = live_agent();
        item.answer
            .as_mut()
            .unwrap()
            .citations
            .push(crate::codex::Citation {
                source_id: "ACT-TUIN-001".into(),
                source_date: crate::AS_OF.into(),
                section_id: "S2".into(),
                quote: "Donderdag om drie uur".into(),
            });
        assert!(!evaluate(&item, &gold).unwrap().hard_checks_passed);
        item.answer.as_mut().unwrap().citations[0].quote = section("ACT-TUIN-001", "S2").text;
        assert!(evaluate(&item, &gold).unwrap().hard_checks_passed);
        item.answer.as_mut().unwrap().citations[0].source_id = "ACT-FIETS-002".into();
        assert!(!evaluate(&item, &gold).unwrap().hard_checks_passed);
    }

    #[test]
    fn tool_caps_unexpected_tools_and_manifest_drift_are_hard_failures() {
        let (mut item, gold) = live_agent();
        item.tool_calls[0].tool = "write_file".into();
        assert!(!evaluate(&item, &gold).unwrap().hard_checks_passed);
        item.tool_calls[0].tool = "search_document".into();
        item.tool_calls[0].result.as_mut().unwrap()["manifest"]["source_sha256"] =
            json!("a".repeat(64));
        assert!(!evaluate(&item, &gold).unwrap().hard_checks_passed);
        let (mut item, gold) = live_agent();
        item.tool_calls = vec![item.tool_calls[0].clone(); 5];
        assert!(!evaluate(&item, &gold).unwrap().hard_checks_passed);
    }

    #[test]
    fn invented_retrieval_steps_or_extra_offered_context_cannot_pass() {
        let (mut item, gold) = live_agent();
        item.tool_calls[0].result.as_mut().unwrap()["hits"] = json!([]);
        assert!(!evaluate(&item, &gold).unwrap().hard_checks_passed);
        let (mut item, gold) = live_agent();
        item.offered_sections.push(section("ACT-FIETS-002", "S2"));
        assert!(!evaluate(&item, &gold).unwrap().hard_checks_passed);
    }

    #[test]
    fn failed_first_search_does_not_receive_second_search_first_hop_credit() {
        let (mut item, gold) = live_agent();
        let mut failed = item.tool_calls[0].clone();
        failed.status = "failed".into();
        failed.result = None;
        failed.error = Some("Search tool failed".into());
        item.tool_calls.insert(0, failed);
        let report = evaluate(&item, &gold).unwrap();
        assert_eq!(metric(&report, "first_hop_fact_coverage").value, Some(0.0));
        assert_eq!(metric(&report, "all_hop_fact_coverage").value, Some(1.0));
        assert_eq!(metric(&report, "search_calls").value, Some(2.0));
        assert!(report.hard_checks_passed);
    }

    #[test]
    fn metric_payload_rejects_inconsistent_hard_results_scores_and_missing_measures() {
        let (item, gold) = live_agent();
        let report = evaluate(&item, &gold).unwrap();
        assert!(validate_report(&report).is_ok());
        let mut bad = report.clone();
        bad.hard_checks[0].passed = false;
        assert!(validate_report(&bad).is_err());
        bad = report.clone();
        bad.metrics[0].value = Some(0.125);
        assert!(validate_report(&bad).is_err());
        bad = report.clone();
        bad.metrics[0].value = Some(f64::NAN);
        assert!(validate_report(&bad).is_err());
        bad = report.clone();
        bad.metrics[0].direction = "maybe".into();
        assert!(validate_report(&bad).is_err());
        bad = report.clone();
        bad.metrics[0].numerator = None;
        bad.metrics[0].denominator = None;
        assert!(validate_report(&bad).is_err());
        bad = report.clone();
        bad.metrics.pop();
        assert!(validate_report(&bad).is_err());
        let mut value = serde_json::to_value(report).unwrap();
        value["invented_pass"] = json!(true);
        assert!(serde_json::from_value::<MetricsReport>(value).is_err());
    }

    #[test]
    fn negative_day_evidence_counts_toward_recall_and_duplicates_never_inflate_precision() {
        let (mut item, gold) = record(
            "P07",
            vec![
                section("ACT-TUIN-001", "S2"),
                section("ACT-FIETS-002", "S2"),
            ],
        );
        let report = evaluate(&item, &gold).unwrap();
        assert_eq!(metric(&report, "all_hop_fact_coverage").value, Some(1.0));
        assert_eq!(
            metric(&report, "all_hop_section_recall").value,
            Some(2.0 / 5.0)
        );
        let duplicate = item.retrieval_steps[0].hits[0].clone();
        item.retrieval_steps[0].hits.push(duplicate);
        assert_eq!(
            metric(&evaluate(&item, &gold).unwrap(), "all_hop_precision").denominator,
            Some(2)
        );
    }
}
