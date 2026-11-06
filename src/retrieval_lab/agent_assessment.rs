//! Post-hoc source measurements of guarded agent runs; never consulted by the agent.
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use serde::{Deserialize, Serialize};

use super::{
    agent_run::{self, StoredAgentRecord},
    reference::ExpectationsContext,
};
use crate::experiment::dataset::{CaseCriteria, SectionRef};
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceCoverage {
    pub expected_source_options: Vec<Vec<SectionRef>>,
    pub expected_sections: Vec<SectionRef>,
    pub offered_sections: Vec<SectionRef>,
    pub found_expected_sections: Vec<SectionRef>,
    pub missing_expected_sections: Vec<SectionRef>,
    pub complete: Option<bool>,
    pub coverage: Option<f64>,
    pub first_complete_call_index: Option<usize>,
}

pub const ASSESSOR_VERSION: &str = "agentic-source-assessment-v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentAssessmentStatus {
    Scored,
    NotApplicable,
    ExecutionFailed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentCallAssessment {
    pub index: usize,
    pub query: String,
    pub top_k: usize,
    pub status: String,
    pub error: Option<String>,
    pub offered_sections: Vec<SectionRef>,
    pub newly_offered_sections: Vec<SectionRef>,
    pub cumulative_source_count: usize,
    pub complete_expected_sources: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentAssessment {
    pub schema_version: u32,
    pub assessor_version: String,
    pub run_id: String,
    pub record_sha256: String,
    pub live_output_sha256: String,
    pub dataset_id: String,
    pub case_id: String,
    pub protocol_id: String,
    pub selection_sha256: String,
    pub questions_sha256: String,
    pub query_inputs_sha256: Option<String>,
    pub expectations_sha256: String,
    pub dataset_corpus_sha256: String,
    pub source_sha256: String,
    pub trace_sha256: Option<String>,
    pub runtime_configuration_sha256: Option<String>,
    pub expected_runtime_configuration_sha256: Option<String>,
    pub runtime_configuration_mismatch: bool,
    pub status: AgentAssessmentStatus,
    pub actual_search_calls: Option<usize>,
    pub successful_search_calls: Option<usize>,
    pub elapsed_ms: u64,
    pub source_coverage: SourceCoverage,
    pub calls: Vec<AgentCallAssessment>,
    pub answer_quality_status: String,
    pub ai_judge_calls: usize,
}

fn json_hash(value: &impl Serialize) -> Result<String, String> {
    serde_json::to_vec(value)
        .map(|bytes| crate::manifest::hash(&bytes))
        .map_err(|_| "Cannot encode agent measurement identity".into())
}

fn canonical_sections(corpus: crate::corpus::CorpusId) -> Result<Vec<crate::Section>, String> {
    Ok(corpus
        .sources()?
        .into_iter()
        .flat_map(|source| source.sections)
        .collect())
}

/// Called after generation. No reference text or coverage decision enters the agent loop.
pub fn assess(records: &[StoredAgentRecord]) -> Result<Vec<AgentAssessment>, String> {
    if records.is_empty() {
        return Err("No agent records to assess".into());
    }
    let mut sessions = BTreeSet::new();
    let mut generations = BTreeSet::new();
    let mut treatments = BTreeSet::new();
    let mut expected_runtime: Option<Value> = None;
    for stored in records {
        let record = &stored.record;
        agent_run::validate_record_identity(record)?;
        if record.live.budget_id != records[0].record.live.budget_id
            || !generations.insert(record.live.generation_session_index)
        {
            return Err(
                "Agent source measurements mix budget ledgers or repeat generation indices".into(),
            );
        }
        if stored.record_sha256.len() != 64
            || !stored
                .record_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
            || !sessions.insert(record.live.run_id.clone())
            || !treatments.insert((
                record.dataset_id.clone(),
                record.case_id.clone(),
                record.live.config_id.clone(),
            ))
        {
            return Err("Invalid or duplicate agent source measurement identity".into());
        }
        if record.protocol_id != records[0].record.protocol_id
            || record.batch_id != records[0].record.batch_id
        {
            return Err("Agent source measurements mix protocols or batch identities".into());
        }
        if record.live.status == "completed" {
            let runtime = record
                .live
                .runtime_configuration
                .as_ref()
                .ok_or("Completed agent run has no runtime identity")?;
            if expected_runtime
                .as_ref()
                .is_some_and(|expected| expected != runtime)
            {
                return Err(
                    "Completed agent runs use different runtime/model/binary identities".into(),
                );
            }
            expected_runtime = Some(runtime.clone());
        }
    }
    let expected_runtime_hash = expected_runtime.as_ref().map(json_hash).transpose()?;
    let selected_corpus = super::dataset::load(&records[0].record.dataset_id)?.corpus();
    if records.iter().any(|record| {
        super::dataset::load(&record.record.dataset_id)
            .map(|dataset| dataset.corpus() != selected_corpus)
            .unwrap_or(true)
    }) {
        return Err("Agent source measurements mix source corpus versions".into());
    }
    let sections = canonical_sections(selected_corpus)?;
    let corpus = crate::retrieval::PreparedCorpus::new(sections, None, None)?;
    records
        .iter()
        .map(|stored| {
            let record = &stored.record;
            let live = &record.live;
            let context = ExpectationsContext::load(&record.dataset_id)?;
            let criteria = context
                .criteria
                .get(&record.case_id)
                .ok_or("Agent run has no protected reference case")?;
            let mut calls = Vec::new();
            let mut received = Vec::new();
            let mut trace_sha256 = live.audit_trace.as_ref().map(|trace| trace.sha256.clone());
            let completed = live.status == "completed";
            let actual_search_calls = live.run.as_ref().map(|run| run.tool_calls.len());
            let mut successful_search_calls = None;
            if completed {
                let run = live
                    .run
                    .as_ref()
                    .ok_or("Completed agent run has no runtime transcript")?;
                let trace = live
                    .audit_trace
                    .as_ref()
                    .ok_or("Completed agent run has no source trace")?;
                let transport = super::mcp_trace::TransportConfig {
                    payload_mode: live.payload_mode,
                    session_id: trace.session_id.clone(),
                    trace_path: trace.path.clone(),
                };
                let (full, actual_trace) = super::mcp_trace::bind_run(run, &transport)?;
                if serde_json::to_value(&actual_trace).map_err(|e| e.to_string())?
                    != serde_json::to_value(trace).map_err(|e| e.to_string())?
                {
                    return Err("Agent source trace summary changed".into());
                }
                // The full-response guard does not load a model. Paths are validation-only
                // absolute locations; the original transport has already been bound above.
                let settings = super::lab_mcp::SessionConfig {
                    corpus: context.dataset.corpus(),
                    schema_version: 1,
                    config: live.config.clone(),
                    filters: live.filters.clone(),
                    model_cache: std::env::temp_dir(),
                    sqlite_dir: std::env::temp_dir(),
                    transport: None,
                };
                super::lab_mcp::validate_run(&full, &settings)?;
                trace_sha256 = Some(actual_trace.sha256);
                for (index, call) in full.tool_calls.iter().enumerate() {
                    let offered = if call.status == "completed" {
                        let hits: Vec<crate::retrieval::RetrievalHit> = serde_json::from_value(
                            call.result
                                .as_ref()
                                .ok_or("Successful search has no offered evidence")?["hits"]
                                .clone(),
                        )
                        .map_err(|_| "Invalid offered agent source sections")?;
                        hits.into_iter()
                            .map(|hit| SectionRef {
                                source_id: hit.section.source_id,
                                section_id: hit.section.id,
                            })
                            .collect::<Vec<_>>()
                    } else {
                        Vec::new()
                    };
                    let previous: BTreeSet<_> = received.iter().flatten().cloned().collect();
                    let newly_offered_sections = offered
                        .iter()
                        .filter(|key| !previous.contains(*key))
                        .cloned()
                        .collect::<BTreeSet<_>>()
                        .into_iter()
                        .collect();
                    received.push(offered.clone());
                    let prefix = source_coverage(criteria, &received);
                    calls.push(AgentCallAssessment {
                        index: index + 1,
                        query: call.query.clone(),
                        top_k: call.top_k,
                        status: call.status.clone(),
                        error: call.error.clone(),
                        offered_sections: offered,
                        newly_offered_sections,
                        cumulative_source_count: prefix.offered_sections.len(),
                        complete_expected_sources: prefix.complete,
                    });
                }
                successful_search_calls = Some(
                    full.tool_calls
                        .iter()
                        .filter(|call| call.status == "completed")
                        .count(),
                );
            }
            let mut coverage = source_coverage(criteria, &received);
            let status = if !completed {
                coverage.complete = None;
                coverage.coverage = None;
                coverage.first_complete_call_index = None;
                AgentAssessmentStatus::ExecutionFailed
            } else if coverage.complete.is_none() {
                AgentAssessmentStatus::NotApplicable
            } else {
                AgentAssessmentStatus::Scored
            };
            let observed_hash = live
                .runtime_configuration
                .as_ref()
                .map(json_hash)
                .transpose()?;
            let runtime_configuration_mismatch =
                live.runtime_configuration.as_ref().is_some_and(|runtime| {
                    runtime["model"] != record.requested_model
                        || expected_runtime
                            .as_ref()
                            .is_some_and(|expected| expected != runtime)
                });
            Ok(AgentAssessment {
                schema_version: 1,
                assessor_version: ASSESSOR_VERSION.into(),
                run_id: live.run_id.clone(),
                record_sha256: stored.record_sha256.clone(),
                live_output_sha256: record.live_output_sha256.clone(),
                dataset_id: record.dataset_id.clone(),
                case_id: record.case_id.clone(),
                protocol_id: record.protocol_id.clone(),
                selection_sha256: record.selection_sha256.clone(),
                questions_sha256: record.dataset.questions_sha256.clone(),
                query_inputs_sha256: record.query_inputs_sha256.clone(),
                expectations_sha256: context.expectations_sha256.clone(),
                dataset_corpus_sha256: record.dataset.corpus_sha256.clone(),
                source_sha256: corpus.identity.source_sha256.clone(),
                trace_sha256,
                runtime_configuration_sha256: observed_hash,
                expected_runtime_configuration_sha256: expected_runtime_hash.clone(),
                runtime_configuration_mismatch,
                status,
                actual_search_calls,
                successful_search_calls,
                elapsed_ms: live.elapsed_ms,
                source_coverage: coverage,
                calls,
                answer_quality_status: "not_assessed".into(),
                ai_judge_calls: 0,
            })
        })
        .collect()
}

pub fn read_assessments(
    records: &[StoredAgentRecord],
    path: &Path,
) -> Result<Vec<AgentAssessment>, String> {
    let bytes = std::fs::read(path).map_err(|_| "Cannot read agent source assessments")?;
    if bytes.len() > super::records::MAX_ARTIFACT_BYTES {
        return Err("Agent assessment file exceeds size boundary".into());
    }
    let mut imported = BTreeMap::new();
    let lines = bytes.split(|byte| *byte == b'\n').collect::<Vec<_>>();
    for (index, line) in lines.iter().enumerate() {
        if line.is_empty() && index + 1 == lines.len() {
            continue;
        }
        let value: AgentAssessment =
            serde_json::from_slice(line).map_err(|_| "Invalid agent source assessment line")?;
        if imported.insert(value.run_id.clone(), value).is_some() {
            return Err("Duplicate imported agent source assessment".into());
        }
    }
    let expected = assess(records)?;
    if imported.len() != expected.len()
        || expected
            .iter()
            .any(|value| imported.get(&value.run_id) != Some(value))
    {
        return Err("Imported agent source scores or immutable bindings changed".into());
    }
    Ok(expected)
}

fn key(section: &SectionRef) -> String {
    format!("{}:{}", section.source_id, section.section_id)
}

/// A local source-comparison export; model-generated answer quality stays unassessed.
pub fn report(
    records: &[StoredAgentRecord],
    assessments: &[AgentAssessment],
) -> Result<Value, String> {
    let expected = assess(records)?;
    if assessments != expected {
        return Err("Agent report requires freshly recomputed bound assessments".into());
    }
    let first = &records[0].record;
    let plan = agent_run::plan_for_dataset(
        super::dataset::ten_source_dataset_id(&first.dataset_id),
        &[],
        &super::catalog::METHODS,
        &first.requested_model,
    )?;
    let mut questions = Vec::new();
    let mut seen = BTreeSet::new();
    for input in &plan.runs {
        if !seen.insert((input.dataset_id.clone(), input.case_id.clone())) {
            continue;
        }
        let context = ExpectationsContext::load(&input.dataset_id)?;
        let criteria = &context.criteria[&input.case_id];
        let source_plans = source_coverage(criteria, &[]).expected_source_options;
        questions.push(json!({"key":format!("{}/{}",input.dataset_id,input.case_id),"dataset":input.dataset_id,"case_id":input.case_id,"question":input.question,"kind":context.dataset.catalog.cases.iter().find(|case|case.id==input.case_id).ok_or("Missing agent question metadata")?.kind,"criteria":criteria.facts,"required_unknowns":criteria.required_unknowns,"expected_source_plans":source_plans.iter().map(|plan|plan.iter().map(key).collect::<Vec<_>>()).collect::<Vec<_>>() }));
    }
    let sources:BTreeMap<_,_> = canonical_sections(super::dataset::load(&first.dataset_id)?.corpus())?.into_iter().map(|section| {
        let id=format!("{}:{}",section.source_id,section.id);
        (id,json!({"source_id":section.source_id,"source_title":section.source_title,"source_date":section.source_date,"id":section.id,"heading":section.heading,"text":section.text}))
    }).collect();
    let mut runs = Vec::new();
    for (stored, measured) in records.iter().zip(assessments) {
        let record = &stored.record;
        let context = ExpectationsContext::load(&record.dataset_id)?;
        let criteria = &context.criteria[&record.case_id];
        let offered: BTreeSet<_> = measured
            .source_coverage
            .offered_sections
            .iter()
            .cloned()
            .collect();
        let scored = measured.status == AgentAssessmentStatus::Scored;
        let facts: Vec<_> = criteria
            .facts
            .iter()
            .map(|fact| {
                let applicable = scored && !fact.evidence_alternatives.is_empty();
                let covered = applicable.then(|| {
                    fact.evidence_alternatives.iter().any(|alternative| {
                        alternative.iter().all(|section| offered.contains(section))
                    })
                });
                json!({"id":fact.id,"covered":covered,"unscored":!applicable})
            })
            .collect();
        let covered = facts.iter().filter(|fact| fact["covered"] == true).count();
        let target = facts
            .iter()
            .filter(|fact| fact["unscored"] == false)
            .count();
        let case_key = format!("{}/{}", record.dataset_id, record.case_id);
        let method =
            serde_json::to_value(record.live.config.retrieval.method).map_err(|e| e.to_string())?;
        let method = method.as_str().ok_or("Invalid agent method name")?;
        let profile = "documents_sections";
        let scenario = "agentic_baseline";
        let call_rows:Vec<_>=measured.calls.iter().map(|call|json!({"index":call.index,"query":call.query,"status":call.status,"error":call.error,"top_k":call.top_k,"hits":call.offered_sections.iter().map(|section|json!({"key":key(section),"inclusion":"seed"})).collect::<Vec<_>>(),"new_source_keys":call.newly_offered_sections.iter().map(key).collect::<Vec<_>>(),"cumulative_source_count":call.cumulative_source_count,"complete_expected_sources":call.complete_expected_sources})).collect();
        runs.push(json!({
            "key":format!("{case_key}|{profile}|{scenario}|{method}"),"case_key":case_key,"profile":profile,"scenario":scenario,"method":method,"execution_kind":agent_run::EXECUTION_KIND,"status":record.live.status,"error":record.live.error,"covered":covered,"total":target,"value":(target>0).then(||covered as f64/target as f64),"facts":facts,"hits":measured.source_coverage.offered_sections.iter().map(|section|json!({"key":key(section),"inclusion":"seed"})).collect::<Vec<_>>(),
            "expected_pieces":{"covered":scored.then_some(measured.source_coverage.found_expected_sections.len()),"total":measured.source_coverage.expected_sections.len(),"selected_keys":measured.source_coverage.expected_sections.iter().map(key).collect::<Vec<_>>(),"found_keys":measured.source_coverage.found_expected_sections.iter().map(key).collect::<Vec<_>>(),"missing_keys":measured.source_coverage.missing_expected_sections.iter().map(key).collect::<Vec<_>>()},
            "search_actions":measured.actual_search_calls,"successful_search_actions":measured.successful_search_calls,"total_wait_ms":measured.elapsed_ms,"first_complete_call_index":measured.source_coverage.first_complete_call_index,"calls":call_rows,"run_id":record.live.run_id,"record_sha256":stored.record_sha256,"live_output_sha256":record.live_output_sha256,"config":record.live.config,"source_file":"agentic-evidence.jsonl","assessment":measured,
        }));
    }
    Ok(
        json!({"schema_version":2,"execution_kind":agent_run::EXECUTION_KIND,"display_metric":"unique_expected_source_sections_v1","assessor_version":ASSESSOR_VERSION,"protocol_id":first.protocol_id,"batch_id":first.batch_id,"selection_sha256":first.selection_sha256,"requested_model":first.requested_model,"runtime_configuration_sha256":assessments.iter().find_map(|row|row.expected_runtime_configuration_sha256.as_ref()),"search_actions_definition":"Every recorded search attempt in the full runtime transcript, including failed/empty attempts; null when a failed session has no complete transcript","wait_definition":"Saved LiveRecord elapsed_ms from submitting the question and initializing the bounded session to final completion or technical failure; not retrieval-only CPU time","answer_quality_status":"not_assessed","ai_judge_calls":0,"source_manifest_sha256":first.dataset.corpus_sha256,"questions":questions,"sources":sources,"runs":runs}),
    )
}

fn source_coverage(criteria: &CaseCriteria, calls: &[Vec<SectionRef>]) -> SourceCoverage {
    let mut plans = BTreeSet::from([BTreeSet::new()]);
    let mut has_targets = false;
    for fact in &criteria.facts {
        if fact.evidence_alternatives.is_empty() {
            continue;
        }
        has_targets = true;
        // Keep every valid route, including longer alternatives: a model may
        // receive a different valid proof than the smallest possible one.
        plans = plans
            .iter()
            .flat_map(|previous| {
                fact.evidence_alternatives.iter().map(move |alternative| {
                    previous
                        .iter()
                        .chain(alternative)
                        .cloned()
                        .collect::<BTreeSet<_>>()
                })
            })
            .collect();
    }
    if !has_targets {
        plans.clear();
    }
    let mut offered = BTreeSet::new();
    let mut first_complete_call_index = None;
    for (index, sections) in calls.iter().enumerate() {
        offered.extend(sections.iter().cloned());
        if first_complete_call_index.is_none() && plans.iter().any(|plan| plan.is_subset(&offered))
        {
            first_complete_call_index = Some(index + 1);
        }
    }
    let selected = plans
        .iter()
        .max_by(|left, right| {
            let left_found = left.intersection(&offered).count();
            let right_found = right.intersection(&offered).count();
            (left_found * right.len())
                .cmp(&(right_found * left.len()))
                .then_with(|| right.len().cmp(&left.len()))
                .then_with(|| right.cmp(left))
        })
        .cloned()
        .unwrap_or_default();
    let found: Vec<_> = selected.intersection(&offered).cloned().collect();
    let missing: Vec<_> = selected.difference(&offered).cloned().collect();
    SourceCoverage {
        expected_source_options: plans
            .into_iter()
            .map(|plan| plan.into_iter().collect())
            .collect(),
        expected_sections: selected.iter().cloned().collect(),
        offered_sections: offered.into_iter().collect(),
        complete: has_targets.then_some(missing.is_empty()),
        coverage: has_targets.then(|| found.len() as f64 / selected.len() as f64),
        found_expected_sections: found,
        missing_expected_sections: missing,
        first_complete_call_index,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::retrieval_lab::reference::ExpectationsContext;

    fn section(source_id: &str, section_id: &str) -> SectionRef {
        SectionRef {
            source_id: source_id.into(),
            section_id: section_id.into(),
        }
    }

    #[test]
    fn sources_from_separate_searches_jointly_complete_a_question() {
        let context = ExpectationsContext::load("legacy").unwrap();
        let when = section("ACT-FIETS-002", "S2");
        let where_ = section("ACT-FIETS-002", "S3");
        let result = source_coverage(
            &context.criteria["T02"],
            &[vec![when.clone()], vec![where_.clone()]],
        );
        assert_eq!(result.complete, Some(true));
        assert_eq!(result.found_expected_sections, vec![when, where_]);
        assert_eq!(result.first_complete_call_index, Some(2));
        assert_eq!(result.coverage, Some(1.0));
    }

    #[test]
    fn repeated_hits_and_extra_searches_do_not_change_the_first_complete_step() {
        let context = ExpectationsContext::load("legacy").unwrap();
        let when = section("ACT-FIETS-002", "S2");
        let where_ = section("ACT-FIETS-002", "S3");
        let result = source_coverage(
            &context.criteria["T02"],
            &[
                vec![when.clone(), when.clone()],
                vec![],
                vec![where_.clone()],
                vec![where_],
            ],
        );
        assert_eq!(result.complete, Some(true));
        assert_eq!(result.offered_sections.len(), 2);
        assert_eq!(result.first_complete_call_index, Some(3));
    }

    #[test]
    fn either_accepted_nature_source_completes_the_walking_question() {
        let context = ExpectationsContext::load("legacy").unwrap();
        for alternative in ["S1", "S3"] {
            let natural = section("ACT-WANDEL-005", alternative);
            let when = section("ACT-WANDEL-005", "S2");
            let result = source_coverage(&context.criteria["P06"], &[vec![natural], vec![when]]);
            assert_eq!(result.complete, Some(true));
            assert_eq!(result.expected_sections.len(), 2);
            assert_eq!(result.first_complete_call_index, Some(2));
        }
    }

    #[test]
    fn two_alternative_nature_sources_cannot_replace_the_missing_time() {
        let context = ExpectationsContext::load("legacy").unwrap();
        let result = source_coverage(
            &context.criteria["P06"],
            &[vec![
                section("ACT-WANDEL-005", "S1"),
                section("ACT-WANDEL-005", "S3"),
            ]],
        );
        assert_eq!(result.complete, Some(false));
        assert_eq!(result.expected_sections.len(), 2);
        assert_eq!(result.found_expected_sections.len(), 1);
        assert_eq!(
            result.missing_expected_sections,
            vec![section("ACT-WANDEL-005", "S2")]
        );
    }

    #[test]
    fn absent_source_targets_are_unassessed_instead_of_perfect() {
        let context = ExpectationsContext::load("legacy").unwrap();
        let result = source_coverage(
            &context.criteria["P05"],
            &[vec![section("ACT-TUIN-001", "S1")]],
        );
        assert_eq!(result.complete, None);
        assert_eq!(result.coverage, None);
        assert_eq!(result.first_complete_call_index, None);
        assert!(result.expected_source_options.is_empty());
    }

    #[test]
    fn a_valid_longer_alternative_is_not_lost_to_minimum_set_pruning() {
        let context = ExpectationsContext::load("legacy").unwrap();
        let mut criteria = context.criteria["T02"].clone();
        criteria.facts.truncate(1);
        criteria.facts[0].evidence_alternatives = vec![
            vec![section("ACT-FIETS-002", "S1")],
            vec![
                section("ACT-FIETS-002", "S2"),
                section("ACT-FIETS-002", "S3"),
            ],
        ];
        let result = source_coverage(
            &criteria,
            &[vec![
                section("ACT-FIETS-002", "S2"),
                section("ACT-FIETS-002", "S3"),
            ]],
        );
        assert_eq!(result.complete, Some(true));
        assert_eq!(result.expected_source_options.len(), 2);
        assert_eq!(result.expected_sections.len(), 2);
        assert_eq!(result.first_complete_call_index, Some(1));
    }

    #[test]
    fn source_ownership_is_part_of_the_match() {
        let context = ExpectationsContext::load("legacy").unwrap();
        let result = source_coverage(
            &context.criteria["T02"],
            &[vec![
                section("ACT-TUIN-001", "S2"),
                section("ACT-FIETS-002", "S3"),
            ]],
        );
        assert_eq!(result.complete, Some(false));
        assert_eq!(result.found_expected_sections.len(), 1);
        assert_eq!(
            result.missing_expected_sections,
            vec![section("ACT-FIETS-002", "S2")]
        );
    }

    struct Fixture {
        directory: std::path::PathBuf,
        stored: StoredAgentRecord,
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.directory);
        }
    }

    fn fixture() -> Fixture {
        fixture_for("T02")
    }

    fn fixture_for(case_id: &str) -> Fixture {
        use crate::{
            codex::{CodexRun, GroundedAnswer, ToolCall},
            retrieval::SearchMethod,
            retrieval_lab::{
                agent_run,
                lab_mcp::{SearchSession, SessionConfig},
                live::LiveRecord,
                mcp_trace::{self, PayloadMode, TransportConfig},
            },
        };
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let directory = std::env::temp_dir().join(format!(
            "coa-agent-assessment-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir(&directory).unwrap();
        let trace_path = directory.join("live.mcp-trace.jsonl");
        std::fs::write(&trace_path, b"").unwrap();
        let plan = agent_run::plan(&[case_id.into()], &[SearchMethod::Bm25], "test-model").unwrap();
        let selected = &plan.runs[0];
        let transport = TransportConfig {
            payload_mode: PayloadMode::Compact,
            session_id: directory
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            trace_path,
        };
        let settings = SessionConfig {
            corpus: crate::corpus::CorpusId::BaselineFive,
            schema_version: 2,
            config: selected.config.clone(),
            filters: Default::default(),
            model_cache: directory.join("models"),
            sqlite_dir: directory.join("sqlite"),
            transport: Some(transport.clone()),
        };
        let mut session = SearchSession::new(settings.clone()).unwrap();
        let mut calls = Vec::new();
        for query in [
            "fietsen dinsdag 14.00 16.00",
            "Esdoornstraat Werkbank binnen",
        ] {
            let result = session.search(query, 3).unwrap();
            calls.push(ToolCall {
                server: "coa".into(),
                tool: "search_document".into(),
                query: query.into(),
                top_k: 3,
                status: "completed".into(),
                result: Some(result),
                error: None,
            });
        }
        // A failed search is real work, but has no successful sidecar record.
        calls.insert(
            1,
            ToolCall {
                server: "coa".into(),
                tool: "search_document".into(),
                query: "mislukte zoekactie".into(),
                top_k: 3,
                status: "failed".into(),
                result: None,
                error: Some("fixture".into()),
            },
        );
        let first = calls[0].result.as_ref().unwrap().clone();
        let run = CodexRun {
            id: "fixture-codex-run".into(),
            status: "completed".into(),
            question: selected.question.clone(),
            answer: GroundedAnswer {
                text: "Onbekend".into(),
                citations: vec![],
                unknowns: vec!["Beschikbaarheid onbekend".into()],
            },
            tool_calls: calls,
            sources: first["sources"].as_array().unwrap().clone(),
            as_of: crate::AS_OF.into(),
            manifest: first["manifest"].clone(),
            client_version: "fixture-runtime".into(),
            model: "test-model".into(),
            elapsed_ms: 1200,
            usage: None,
        };
        crate::retrieval_lab::lab_mcp::validate_run(&run, &settings).unwrap();
        let trace = mcp_trace::summary(&transport).unwrap();
        let config_id = selected.config_id.clone();
        let runtime = serde_json::json!({"model":"test-model","binaries":{"mcp":crate::manifest::hash(b"fixture-mcp"),"codex":crate::manifest::hash(b"fixture-codex"),"code_mode_host":crate::manifest::hash(b"fixture-host")},"prompt_version":"fixture-v1","client_version":"fixture-runtime","reasoning_effort":"low","tool_policy_version":1,"output_schema_version":2});
        let live = LiveRecord {schema_version:2,execution_kind:"codex_selected_mcp_v2".into(),run_id:directory.file_name().unwrap().to_string_lossy().into_owned(),question:selected.question.clone(),config_id:config_id.clone(),config:selected.config.clone(),filters:Default::default(),status:"completed".into(),budget_id:"fixture-budget".into(),generation_session_index:1,ai_judge_calls:0,answer_quality_status:"not_assessed".into(),source_guard_passed:Some(true),runtime_configuration:Some(runtime),run:Some(run),error:None,elapsed_ms:1300,payload_mode:PayloadMode::Compact,transport_treatment_id:Some(crate::manifest::hash(&serde_json::to_vec(&serde_json::json!({"schema_version":1,"config_id":config_id,"filters":crate::retrieval::QueryFilters::default(),"payload_mode":"compact"})).unwrap())),audit_trace:Some(trace),trace_integrity_status:Some("validated".into())};
        let live_output_path = directory.join("live.jsonl");
        let mut bytes = serde_json::to_vec(&live).unwrap();
        bytes.push(b'\n');
        std::fs::write(&live_output_path, &bytes).unwrap();
        let record = agent_run::AgentRunRecord {
            schema_version: 1,
            execution_kind: agent_run::EXECUTION_KIND.into(),
            batch_id: "fixture-batch".into(),
            dataset_id: selected.dataset_id.clone(),
            dataset: selected.dataset.clone(),
            query_inputs_sha256: selected.query_inputs_sha256.clone(),
            case_id: selected.case_id.clone(),
            question: selected.question.clone(),
            selection_sha256: plan.selection_sha256,
            protocol_id: plan.protocol_id,
            requested_model: "test-model".into(),
            live_output_path,
            live_output_sha256: crate::manifest::hash(&bytes),
            live,
        };
        let record_sha256 = crate::manifest::hash(&serde_json::to_vec(&record).unwrap());
        Fixture {
            directory,
            stored: StoredAgentRecord {
                record,
                record_sha256,
            },
        }
    }

    #[test]
    fn live_assessment_counts_every_attempt_and_cumulative_model_visible_sources() {
        let fixture = fixture();
        let result = assess(std::slice::from_ref(&fixture.stored)).unwrap();
        let measured = &result[0];
        assert_eq!(measured.status, AgentAssessmentStatus::Scored);
        assert_eq!(measured.actual_search_calls, Some(3));
        assert_eq!(measured.successful_search_calls, Some(2));
        assert_eq!(measured.elapsed_ms, 1300);
        assert_eq!(measured.source_coverage.complete, Some(true));
        assert_eq!(measured.source_coverage.first_complete_call_index, Some(3));
        assert_eq!(measured.calls[1].index, 2);
        assert!(measured.calls[1].offered_sections.is_empty());
        assert_eq!(measured.calls[2].complete_expected_sources, Some(true));
        assert_eq!(measured.answer_quality_status, "not_assessed");
        assert_eq!(measured.ai_judge_calls, 0);
    }

    #[test]
    fn imported_assessment_is_recomputed_instead_of_trusting_a_changed_score() {
        let fixture = fixture();
        let expected = assess(std::slice::from_ref(&fixture.stored)).unwrap();
        let path = fixture.directory.join("assessment.jsonl");
        let mut bytes = serde_json::to_vec(&expected[0]).unwrap();
        bytes.push(b'\n');
        std::fs::write(&path, &bytes).unwrap();
        assert_eq!(
            read_assessments(std::slice::from_ref(&fixture.stored), &path).unwrap(),
            expected
        );
        let mut forged = expected[0].clone();
        forged.actual_search_calls = Some(1);
        std::fs::write(&path, serde_json::to_vec(&forged).unwrap()).unwrap();
        assert!(read_assessments(std::slice::from_ref(&fixture.stored), &path).is_err());
    }

    #[test]
    fn trace_tampering_cannot_become_a_valid_source_measurement() {
        let fixture = fixture();
        assert!(assess(std::slice::from_ref(&fixture.stored)).is_ok());
        let trace = &fixture
            .stored
            .record
            .live
            .audit_trace
            .as_ref()
            .unwrap()
            .path;
        let original = std::fs::read(trace).unwrap();
        let mut record: serde_json::Value =
            serde_json::from_slice(original.split(|byte| *byte == b'\n').next().unwrap()).unwrap();
        record["full_response"]["hits"][0]["section"]["text"] = serde_json::json!("forged");
        std::fs::write(trace, serde_json::to_vec(&record).unwrap()).unwrap();
        assert!(assess(std::slice::from_ref(&fixture.stored)).is_err());
    }

    fn rewrite_fixture_live(fixture: &mut Fixture) {
        let mut bytes = serde_json::to_vec(&fixture.stored.record.live).unwrap();
        bytes.push(b'\n');
        std::fs::write(&fixture.stored.record.live_output_path, &bytes).unwrap();
        fixture.stored.record.live_output_sha256 = crate::manifest::hash(&bytes);
        fixture.stored.record_sha256 =
            crate::manifest::hash(&serde_json::to_vec(&fixture.stored.record).unwrap());
    }

    #[test]
    fn a_timeout_without_a_runtime_transcript_does_not_invent_zero_attempts_or_success() {
        let mut fixture = fixture();
        fixture.stored.record.live.status = "failed".into();
        fixture.stored.record.live.error = Some("timeout".into());
        fixture.stored.record.live.run = None;
        fixture.stored.record.live.source_guard_passed = None;
        fixture.stored.record.live.trace_integrity_status = Some("partial_not_validated".into());
        rewrite_fixture_live(&mut fixture);
        let measured = assess(std::slice::from_ref(&fixture.stored))
            .unwrap()
            .remove(0);
        assert_eq!(measured.status, AgentAssessmentStatus::ExecutionFailed);
        assert_eq!(measured.actual_search_calls, None);
        assert_eq!(measured.source_coverage.complete, None);
        assert!(measured.source_coverage.offered_sections.is_empty());
    }

    #[test]
    fn a_foreign_dataset_cannot_borrow_the_legacy_expectations() {
        let mut fixture = fixture();
        assert!(assess(std::slice::from_ref(&fixture.stored)).is_ok());
        fixture.stored.record.dataset_id = "independent-20261103".into();
        assert!(assess(std::slice::from_ref(&fixture.stored)).is_err());
    }

    #[test]
    fn report_exposes_real_total_work_and_rejects_forged_measurements() {
        let fixture = fixture();
        let measured = assess(std::slice::from_ref(&fixture.stored)).unwrap();
        let value = report(std::slice::from_ref(&fixture.stored), &measured).unwrap();
        assert_eq!(value["execution_kind"], "agentic_search_comparison");
        assert_eq!(value["runs"][0]["search_actions"], 3);
        assert_eq!(value["runs"][0]["total_wait_ms"], 1300);
        assert_eq!(value["runs"][0]["calls"][1]["status"], "failed");
        assert_eq!(value["runs"][0]["first_complete_call_index"], 3);
        assert_eq!(value["questions"].as_array().unwrap().len(), 10);
        assert_eq!(value["answer_quality_status"], "not_assessed");
        let mut forged = measured;
        forged[0].actual_search_calls = Some(1);
        assert!(report(std::slice::from_ref(&fixture.stored), &forged).is_err());
    }

    #[test]
    fn rows_from_a_different_runtime_binary_cannot_be_mixed_into_one_report() {
        let first = fixture_for("T02");
        let mut second = fixture_for("P06");
        second.stored.record.live.generation_session_index = 2;
        rewrite_fixture_live(&mut second);
        assert!(assess(&[first.stored.clone(), second.stored.clone()]).is_ok());
        second
            .stored
            .record
            .live
            .runtime_configuration
            .as_mut()
            .unwrap()["binaries"]["mcp"] =
            serde_json::json!(crate::manifest::hash(b"changed-binary"));
        rewrite_fixture_live(&mut second);
        let records = [first.stored.clone(), second.stored.clone()];
        assert!(assess(&records).is_err());
    }

    #[test]
    fn a_failed_model_pin_is_visible_audit_evidence_without_a_success_score() {
        let first = fixture_for("T02");
        let mut failed = fixture_for("P06");
        failed.stored.record.live.generation_session_index = 2;
        failed.stored.record.live.status = "failed".into();
        failed.stored.record.live.error = Some("runtime".into());
        failed.stored.record.live.run = None;
        failed.stored.record.live.source_guard_passed = None;
        failed.stored.record.live.trace_integrity_status = Some("partial_not_validated".into());
        failed
            .stored
            .record
            .live
            .runtime_configuration
            .as_mut()
            .unwrap()["model"] = serde_json::json!("wrong-model");
        let trace = failed.stored.record.live.audit_trace.as_mut().unwrap();
        std::fs::write(&trace.path, b"").unwrap();
        trace.sha256 = crate::manifest::hash(b"");
        trace.trace_bytes = 0;
        trace.payload_bytes = 0;
        trace.successful_searches = 0;
        rewrite_fixture_live(&mut failed);
        let rows = assess(&[first.stored.clone(), failed.stored.clone()]).unwrap();
        assert_eq!(rows[1].status, AgentAssessmentStatus::ExecutionFailed);
        assert_eq!(rows[1].actual_search_calls, None);
        assert_eq!(rows[1].source_coverage.complete, None);
        assert!(rows[1].runtime_configuration_mismatch);
        assert_ne!(
            rows[1].runtime_configuration_sha256,
            rows[1].expected_runtime_configuration_sha256
        );
    }

    #[test]
    fn records_from_different_budget_ledgers_cannot_form_one_comparison() {
        let first = fixture_for("T02");
        let mut second = fixture_for("P06");
        second.stored.record.live.generation_session_index = 2;
        rewrite_fixture_live(&mut second);
        assert!(assess(&[first.stored.clone(), second.stored.clone()]).is_ok());
        second.stored.record.live.budget_id = "foreign-budget".into();
        rewrite_fixture_live(&mut second);
        assert!(assess(&[first.stored.clone(), second.stored.clone()]).is_err());
    }

    #[test]
    fn repeated_generation_indices_cannot_hide_a_restarted_budget() {
        let first = fixture_for("T02");
        let mut second = fixture_for("P06");
        second.stored.record.live.generation_session_index = 2;
        rewrite_fixture_live(&mut second);
        assert!(assess(&[first.stored.clone(), second.stored.clone()]).is_ok());
        second.stored.record.live.generation_session_index = 1;
        rewrite_fixture_live(&mut second);
        assert!(assess(&[first.stored.clone(), second.stored.clone()]).is_err());
    }

    #[test]
    fn cli_assess_and_report_replay_saved_agent_runs_without_a_runtime() {
        let mut fixture = fixture();
        fixture.stored.record.live.elapsed_ms = 0;
        fixture.stored.record.live.run.as_mut().unwrap().elapsed_ms = 0;
        rewrite_fixture_live(&mut fixture);
        let input = fixture.directory.join("batch.jsonl");
        let mut original = serde_json::to_vec(&fixture.stored.record).unwrap();
        original.push(b'\n');
        std::fs::write(&input, &original).unwrap();
        let grades = fixture.directory.join("grades.jsonl");
        let output = fixture.directory.join("report.json");
        let paths = |value: &std::path::Path| value.to_string_lossy().into_owned();
        let assessed = crate::retrieval_lab::cli::cli(&[
            "agent-assess".into(),
            "--input".into(),
            paths(&input),
            "--output".into(),
            paths(&grades),
        ])
        .unwrap();
        assert_eq!(assessed["codex_calls"], 0);
        assert_eq!(assessed["ai_judge_calls"], 0);
        let reported = crate::retrieval_lab::cli::cli(&[
            "agent-report".into(),
            "--input".into(),
            paths(&input),
            "--assessments".into(),
            paths(&grades),
            "--output".into(),
            paths(&output),
        ])
        .unwrap();
        assert_eq!(reported["codex_calls"], 0);
        let result: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&output).unwrap()).unwrap();
        assert_eq!(result["runs"][0]["search_actions"], 3);
        assert_eq!(result["runs"][0]["total_wait_ms"], 0);
        assert_eq!(std::fs::read(&input).unwrap(), original);
        let duplicate = fixture.directory.join("duplicate.json");
        assert!(
            crate::retrieval_lab::cli::cli(&[
                "agent-report".into(),
                "--input".into(),
                format!("{},{}", paths(&input), paths(&input)),
                "--output".into(),
                paths(&duplicate),
            ])
            .is_err()
        );
        assert!(!duplicate.exists());
        let mut forged: AgentAssessment =
            serde_json::from_slice(&std::fs::read(&grades).unwrap()).unwrap();
        forged.actual_search_calls = Some(1);
        std::fs::write(&grades, serde_json::to_vec(&forged).unwrap()).unwrap();
        let forged_output = fixture.directory.join("forged-report.json");
        assert!(
            crate::retrieval_lab::cli::cli(&[
                "agent-report".into(),
                "--input".into(),
                paths(&input),
                "--assessments".into(),
                paths(&grades),
                "--output".into(),
                paths(&forged_output),
            ])
            .is_err()
        );
        assert!(!forged_output.exists());
    }
}
