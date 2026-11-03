use super::{
    assessment::{self, AssessmentKind},
    catalog,
    preparation::PreparationOptions,
    records, runner,
};
use crate::{
    retrieval::{QueryFilters, RetrievalConfig, SearchMethod},
    storage::StorageProfile,
};
use std::{collections::BTreeMap, io::Write, path::Path};

const COMMON: &[&str] = &[
    "--dataset",
    "--protocol",
    "--methods",
    "--profiles",
    "--cases",
    "--top-k",
    "--candidate-k",
    "--context-limit",
    "--selection",
    "--apply-filters",
    "--mmr",
    "--rrf-k",
    "--vector-weight",
    "--mmr-lambda",
];

fn flags(args: &[String], allowed: &[&str]) -> Result<BTreeMap<String, String>, String> {
    if !args.len().is_multiple_of(2) {
        return Err("Each lab flag requires a value".into());
    }
    let mut flags = BTreeMap::new();
    for pair in args.chunks_exact(2) {
        if !allowed.contains(&pair[0].as_str())
            || pair[1].is_empty()
            || flags.insert(pair[0].clone(), pair[1].clone()).is_some()
        {
            return Err(format!("Unknown, empty or duplicate lab flag {}", pair[0]));
        }
    }
    Ok(flags)
}

fn choices<T>(value: Option<&String>, defaults: &[T]) -> Result<Vec<T>, String>
where
    T: Clone + serde::de::DeserializeOwned,
{
    match value {
        None => Ok(defaults.to_vec()),
        Some(value) => value
            .split(',')
            .map(|choice| {
                serde_json::from_value(serde_json::Value::String(choice.trim().into()))
                    .map_err(|_| format!("Unknown selection {choice}"))
            })
            .collect(),
    }
}

fn boolean(value: Option<&String>, default: bool) -> Result<bool, String> {
    match value.map(String::as_str) {
        None => Ok(default),
        Some("on" | "true") => Ok(true),
        Some("off" | "false") => Ok(false),
        _ => Err("Boolean settings accept on/off or true/false".into()),
    }
}

fn selected_plan(flags: &BTreeMap<String, String>) -> Result<super::types::ExperimentPlan, String> {
    let dataset = super::dataset::load(
        flags
            .get("--dataset")
            .map(String::as_str)
            .unwrap_or("legacy"),
    )?;
    if let Some(protocol) = flags.get("--protocol") {
        if protocol == "quality-v1" {
            if flags.keys().any(|key| {
                ![
                    "--dataset",
                    "--protocol",
                    "--output",
                    "--model-cache",
                    "--sqlite-dir",
                ]
                .contains(&key.as_str())
            }) {
                return Err("quality-v1 is frozen: do not override methods, profiles, cases or advanced settings".into());
            }
            return catalog::quality_plan(&dataset);
        }
        if protocol != "baseline" {
            return Err("Unknown protocol; select baseline or quality-v1".into());
        }
    }
    let mut advanced = RetrievalConfig::default();
    for (key, target) in [
        ("--top-k", &mut advanced.top_k),
        ("--candidate-k", &mut advanced.candidate_k),
        ("--context-limit", &mut advanced.context_token_limit),
    ] {
        if let Some(value) = flags.get(key) {
            *target = value.parse().map_err(|_| format!("Invalid {key}"))?;
        }
    }
    for (key, target) in [
        ("--rrf-k", &mut advanced.rrf_k),
        ("--vector-weight", &mut advanced.vector_weight),
        ("--mmr-lambda", &mut advanced.mmr_lambda),
    ] {
        if let Some(value) = flags.get(key) {
            *target = value.parse().map_err(|_| format!("Invalid {key}"))?;
        }
    }
    if let Some(selection) = flags.get("--selection") {
        advanced.selection = serde_json::from_value(serde_json::Value::String(selection.clone()))
            .map_err(|_| "Unknown context selection")?;
    }
    advanced.apply_filters = boolean(flags.get("--apply-filters"), false)?;
    advanced.mmr = boolean(flags.get("--mmr"), false)?;
    let methods: Vec<SearchMethod> = choices(flags.get("--methods"), &catalog::METHODS)?;
    let profiles: Vec<StorageProfile> = choices(flags.get("--profiles"), &catalog::PROFILES)?;
    let cases = flags
        .get("--cases")
        .map(|value| {
            value
                .split(',')
                .map(|value| value.trim().to_owned())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    catalog::plan_for(&dataset, &methods, &profiles, &cases, &advanced)
}

fn required<'a>(flags: &'a BTreeMap<String, String>, key: &str) -> Result<&'a str, String> {
    flags
        .get(key)
        .map(String::as_str)
        .ok_or_else(|| format!("Missing {key}"))
}

pub fn read_inputs(input: &str) -> Result<Vec<super::types::StoredRecord>, String> {
    let mut all = Vec::new();
    let mut identities = std::collections::BTreeSet::new();
    for path in input.split(',') {
        for stored in records::read_records(Path::new(path))? {
            if !identities.insert(stored.record.run_id.clone()) {
                return Err("Duplicate run across retrieval input files".into());
            }
            all.push(stored);
        }
    }
    Ok(all)
}

fn report_assessments(
    records: &[super::types::StoredRecord],
    input: Option<&String>,
    context: &super::reference::ExpectationsContext,
) -> Result<Vec<assessment::AssessmentRecord>, String> {
    let mut result = match input {
        Some(path) => assessment::read_assessments_with_context(Path::new(path), records, context)?,
        None => Vec::new(),
    };
    for kind in [
        AssessmentKind::Deterministic,
        AssessmentKind::Manual,
        AssessmentKind::Ai,
    ] {
        let missing: Vec<_> = records
            .iter()
            .filter(|record| {
                !result
                    .iter()
                    .any(|value| value.run_id == record.record.run_id && value.kind == kind)
            })
            .cloned()
            .collect();
        if !missing.is_empty() {
            result.extend(assessment::assess_with_context(&missing, &[kind], context)?);
        }
    }
    assessment::validate_bindings_with_context(records, &result, context)?;
    Ok(result)
}

pub fn cli(args: &[String]) -> Result<serde_json::Value, String> {
    match args.first().map(String::as_str) {
        Some("plan" | "catalog") => {
            let flags = flags(&args[1..], COMMON)?;
            serde_json::to_value(selected_plan(&flags)?).map_err(|error| error.to_string())
        }
        Some("run") => {
            let allowed = [COMMON, &["--output", "--model-cache", "--sqlite-dir", "--query-filters"]].concat();
            let flags = flags(&args[1..], &allowed)?;
            let output = std::path::PathBuf::from(required(&flags, "--output")?);
            let filters: QueryFilters = flags.get("--query-filters").map(|value| serde_json::from_str(value).map_err(|_| "Typed query filters do not match the supported schema")).transpose()?.unwrap_or_default();
            let options = runner::RunOptions {output: output.clone(), preparation: PreparationOptions {
                model_cache: flags.get("--model-cache").map(Into::into).unwrap_or_else(|| ".model-cache".into()),
                sqlite_dir: flags.get("--sqlite-dir").map(Into::into).unwrap_or_else(|| output.parent().unwrap_or_else(|| Path::new(".")).join("retrieval-cache")),
            }, filters};
            serde_json::to_value(runner::run(&selected_plan(&flags)?, &options)?).map_err(|error| error.to_string())
        }
        Some("assess") => {
            let flags = flags(&args[1..], &["--input", "--output", "--types", "--dataset"])?;
            let stored = read_inputs(required(&flags, "--input")?)?;
            let kinds = choices(flags.get("--types"), &[AssessmentKind::Deterministic, AssessmentKind::Manual, AssessmentKind::Ai])?;
            let context = super::reference::ExpectationsContext::load(flags.get("--dataset").map(String::as_str).unwrap_or("legacy"))?;
            let assessed = assessment::assess_with_context(&stored, &kinds, &context)?;
            assessment::validate_bindings_with_context(&stored, &assessed, &context)?;
            records::write_jsonl_new(Path::new(required(&flags, "--output")?), &assessed)?;
            Ok(serde_json::json!({"schema_version":1,"retrieval_records":stored.len(),"assessments":assessed.len(),"retrieval_calls":0,"codex_calls":0,"ai_judge_calls":0}))
        }
        Some("report") => {
            let flags = flags(&args[1..], &["--input", "--assessments", "--output", "--dataset"])?;
            let records = read_inputs(required(&flags, "--input")?)?;
            let context = super::reference::ExpectationsContext::load(flags.get("--dataset").map(String::as_str).unwrap_or("legacy"))?;
            let assessments = report_assessments(&records, flags.get("--assessments"), &context)?;
            let html = super::report::render_html_with_context(&records, &assessments, &context)?;
            let output = Path::new(required(&flags, "--output")?);
            let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(output)
                .map_err(|error| format!("Cannot create new report {}: {error}", output.display()))?;
            file.write_all(html.as_bytes()).and_then(|_| file.sync_all()).map_err(|error| error.to_string())?;
            Ok(serde_json::json!({"schema_version":1,"retrieval_records":records.len(),"assessments":assessments.len(),
                "output_sha256":crate::manifest::hash(html.as_bytes()),"retrieval_calls":0,"codex_calls":0,"ai_judge_calls":0}))
        }
        _ => Err("Usage: lab plan | run --output <new.jsonl> | assess --input <run.jsonl> --output <new.jsonl> | report --input <run.jsonl> [--assessments <reviews.jsonl>] --output <new.html>; use --dataset legacy|independent-20261103 consistently; plan/run accept --protocol baseline|quality-v1".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).into()).collect()
    }
    #[test]
    fn plan_command_expands_main_axes_without_loading_a_runtime_or_model() {
        let plan = cli(&args(&["plan"])).unwrap();
        assert_eq!(plan["planned_rows"], 288);
        assert_eq!(plan["codex_calls"], 0);
        assert_eq!(plan["ai_judge_calls"], 0);
        assert_eq!(plan["configurations"].as_array().unwrap().len(), 16);
        for bad in [
            args(&["plan", "--methods", "bm25,bm25"]),
            args(&["plan", "--cases", "NOT_REAL"]),
            args(&["plan", "--wat", "true"]),
            args(&["plan", "--top-k", "0"]),
            args(&["plan", "--profiles", "postgres"]),
        ] {
            assert!(cli(&bad).is_err());
        }
    }

    #[test]
    fn local_keyword_commands_run_and_reassess_the_same_immutable_artifact() {
        let base = std::env::temp_dir().join(format!(
            "coa-lab-cli-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&base).unwrap();
        let output = base.join("retrieval.jsonl");
        let assessment_output = base.join("assessments.jsonl");
        let values = vec![
            "run".into(),
            "--methods".into(),
            "bm25".into(),
            "--profiles".into(),
            "documents_sections".into(),
            "--cases".into(),
            "T02,P05".into(),
            "--output".into(),
            output.display().to_string(),
            "--model-cache".into(),
            base.join("MISSING-MODEL").display().to_string(),
        ];
        assert_eq!(cli(&values).unwrap()["completed"], 2);
        let bytes = std::fs::read(&output).unwrap();
        let assessment = cli(&[
            "assess".into(),
            "--input".into(),
            output.display().to_string(),
            "--types".into(),
            "deterministic,manual,ai".into(),
            "--output".into(),
            assessment_output.display().to_string(),
        ])
        .unwrap();
        assert_eq!(assessment["assessments"], 6);
        assert_eq!(assessment["retrieval_calls"], 0);
        assert_eq!(assessment["ai_judge_calls"], 0);
        assert_eq!(std::fs::read(&output).unwrap(), bytes);
        let records = super::super::records::read_records(&output).unwrap();
        let assessed =
            super::super::assessment::read_assessments(&assessment_output, &records).unwrap();
        assert_eq!(assessed.len(), 6);
        assert!(!base.join("MISSING-MODEL").exists());
        assert!(cli(&values).is_err());
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn report_imports_manual_reviews_without_retrieval_or_overwriting_artifacts() {
        let base = std::env::temp_dir().join(format!(
            "coa-lab-report-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&base).unwrap();
        let input = base.join("runs.jsonl");
        let review_path = base.join("reviews.jsonl");
        let output = base.join("report.html");
        let run = vec![
            "run".into(),
            "--methods".into(),
            "bm25".into(),
            "--profiles".into(),
            "documents_sections".into(),
            "--cases".into(),
            "T02".into(),
            "--output".into(),
            input.display().to_string(),
            "--model-cache".into(),
            base.join("MISSING-MODEL").display().to_string(),
        ];
        assert_eq!(cli(&run).unwrap()["completed"], 1);
        let source_bytes = std::fs::read(&input).unwrap();
        let stored = records::read_records(&input).unwrap();
        let mut reviews = assessment::assess(&stored, &[AssessmentKind::Manual]).unwrap();
        reviews[0].status = assessment::AssessmentStatus::Completed;
        reviews[0].manual = Some(assessment::ManualReview {
            verdict: assessment::ManualVerdict::Good,
            notes: "Explicit test review; not a human reference label.".into(),
            reviewed_at_unix_ms: 1,
        });
        records::write_jsonl_new(&review_path, &reviews).unwrap();
        let review_bytes = std::fs::read(&review_path).unwrap();
        let command = vec![
            "report".into(),
            "--input".into(),
            input.display().to_string(),
            "--assessments".into(),
            review_path.display().to_string(),
            "--output".into(),
            output.display().to_string(),
        ];
        let summary = cli(&command).unwrap();
        assert_eq!(summary["retrieval_records"], 1);
        assert_eq!(summary["assessments"], 3);
        assert_eq!(summary["retrieval_calls"], 0);
        assert_eq!(summary["codex_calls"], 0);
        assert_eq!(summary["ai_judge_calls"], 0);
        let html = std::fs::read_to_string(&output).unwrap();
        assert!(html.contains("\"verdict\":\"good\""));
        assert!(html.contains("\"status\":\"paused\""));
        assert!(html.contains("\"evidence_coverage\""));
        assert_eq!(std::fs::read(&input).unwrap(), source_bytes);
        assert_eq!(std::fs::read(&review_path).unwrap(), review_bytes);
        assert!(!base.join("MISSING-MODEL").exists());
        assert!(cli(&command).is_err());
        assert_eq!(std::fs::read_to_string(&output).unwrap(), html);
        reviews[0].retrieval_sha256 = "a".repeat(64);
        let stale = base.join("stale-reviews.jsonl");
        records::write_jsonl_new(&stale, &reviews).unwrap();
        let rejected = base.join("rejected.html");
        assert!(
            cli(&[
                "report".into(),
                "--input".into(),
                input.display().to_string(),
                "--assessments".into(),
                stale.display().to_string(),
                "--output".into(),
                rejected.display().to_string()
            ])
            .is_err()
        );
        assert!(!rejected.exists());
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn report_rejects_forged_deterministic_grades_before_creating_output() {
        let base = std::env::temp_dir().join(format!(
            "coa-lab-forged-grade-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&base).unwrap();
        let input = base.join("runs.jsonl");
        let assessment_path = base.join("assessments.jsonl");
        let output = base.join("report.html");
        records::write_jsonl_new(&input, &[records::fixture_record_for("P01")]).unwrap();
        let raw_before = std::fs::read(&input).unwrap();
        let stored = records::read_records(&input).unwrap();
        let mut assessed = assessment::assess(&stored, &[AssessmentKind::Deterministic]).unwrap();
        let metric = assessed[0]
            .deterministic
            .as_mut()
            .unwrap()
            .metrics
            .iter_mut()
            .find(|metric| metric.id == "evidence_coverage")
            .unwrap();
        metric.value = Some(0.12345);
        records::write_jsonl_new(&assessment_path, &assessed).unwrap();
        let grades_before = std::fs::read(&assessment_path).unwrap();
        let result = cli(&[
            "report".into(),
            "--input".into(),
            input.display().to_string(),
            "--assessments".into(),
            assessment_path.display().to_string(),
            "--output".into(),
            output.display().to_string(),
        ]);
        assert!(result.is_err(), "Forged deterministic grade was accepted");
        assert!(!output.exists());
        assert_eq!(std::fs::read(&input).unwrap(), raw_before);
        assert_eq!(std::fs::read(&assessment_path).unwrap(), grades_before);
        std::fs::remove_dir_all(base).unwrap();
    }
}

#[cfg(test)]
mod quality_protocol_tests {
    #[test]
    fn frozen_dataset_assess_report_and_import_bind_the_selected_reference() {
        let base = std::env::temp_dir().join(format!(
            "coa-new-reference-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&base).unwrap();
        let raw = base.join("raw.jsonl");
        let grades = base.join("grades.jsonl");
        let html = base.join("report.html");
        let call = |values: Vec<String>| super::cli(&values);
        let run = call(vec![
            "run".into(),
            "--dataset".into(),
            "independent-20261103".into(),
            "--methods".into(),
            "bm25".into(),
            "--profiles".into(),
            "documents_sections".into(),
            "--cases".into(),
            "IND26-11".into(),
            "--output".into(),
            raw.display().to_string(),
            "--model-cache".into(),
            base.join("NO_MODEL").display().to_string(),
        ])
        .unwrap();
        assert_eq!(run["completed"], 1);
        let before = std::fs::read(&raw).unwrap();
        let summary = call(vec![
            "assess".into(),
            "--dataset".into(),
            "independent-20261103".into(),
            "--input".into(),
            raw.display().to_string(),
            "--output".into(),
            grades.display().to_string(),
        ])
        .expect("assessment must use actual selected criteria");
        assert_eq!(summary["assessments"], 3);
        assert!(
            call(vec![
                "assess".into(),
                "--input".into(),
                raw.display().to_string(),
                "--output".into(),
                base.join("wrong.jsonl").display().to_string()
            ])
            .is_err()
        );
        call(vec![
            "report".into(),
            "--dataset".into(),
            "independent-20261103".into(),
            "--input".into(),
            raw.display().to_string(),
            "--assessments".into(),
            grades.display().to_string(),
            "--output".into(),
            html.display().to_string(),
        ])
        .unwrap();
        let contents = std::fs::read_to_string(html).unwrap();
        assert!(contents.contains("ask_which_activity"));
        assert!(contents.contains("retrieval-task-metrics-v1"));
        let json = contents
            .split("id=\"report-data\" type=\"application/json\">")
            .nth(1)
            .unwrap()
            .split("</script>")
            .next()
            .unwrap();
        let data: serde_json::Value = serde_json::from_str(json).unwrap();
        assert!(data["rows"][0]["task_metrics"]["complete_goal_set"].is_null());
        assert_eq!(data["rows"][0]["task_metrics"]["target_goals"], 0);
        assert_eq!(
            data["rows"][0]["task_metrics"]["answer_review"],
            "not_generated"
        );
        assert_eq!(std::fs::read(&raw).unwrap(), before);
        assert!(!base.join("NO_MODEL").exists());
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn frozen_dataset_plan_is_explicit_and_has_preregistered_336_rows() {
        let values = [
            "plan",
            "--dataset",
            "independent-20261103",
            "--protocol",
            "quality-v1",
        ]
        .map(String::from);
        let plan = super::cli(&values)
            .expect("frozen dataset must be selectable without loading gold or a model");
        assert_eq!(plan["cases"].as_array().unwrap().len(), 12);
        assert_eq!(plan["configurations"].as_array().unwrap().len(), 28);
        assert_eq!(plan["planned_rows"], 336);
        assert_eq!(
            plan["dataset"]["questions_sha256"],
            "06cbd0737c98e672c5704765b6e8ed2e22d940e681b71e0eec6c74ca73338a88"
        );
        let encoded = plan.to_string();
        for protected in [
            "expected_action",
            "fact_roles",
            "evidence_alternatives",
            "forbidden_claims",
            "supported_candidate_activity_ids",
        ] {
            assert!(!encoded.contains(protected));
        }
    }
}
