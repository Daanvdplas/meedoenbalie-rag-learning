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
use std::{collections::BTreeMap, path::Path};

const COMMON: &[&str] = &[
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
    catalog::plan(&methods, &profiles, &cases, &advanced)
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
            let flags = flags(&args[1..], &["--input", "--output", "--types"])?;
            let stored = read_inputs(required(&flags, "--input")?)?;
            let kinds = choices(flags.get("--types"), &[AssessmentKind::Deterministic, AssessmentKind::Manual, AssessmentKind::Ai])?;
            let assessed = assessment::assess(&stored, &kinds)?;
            assessment::validate_bindings(&stored, &assessed)?;
            records::write_jsonl_new(Path::new(required(&flags, "--output")?), &assessed)?;
            Ok(serde_json::json!({"schema_version":1,"retrieval_records":stored.len(),"assessments":assessed.len(),"retrieval_calls":0,"codex_calls":0,"ai_judge_calls":0}))
        }
        _ => Err("Usage: lab plan | run --output <new.jsonl> | assess --input <run.jsonl> --output <new.jsonl>".into()),
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
}
