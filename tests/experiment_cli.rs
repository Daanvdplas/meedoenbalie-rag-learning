use std::{path::PathBuf, process::Command};

#[test]
fn lists_explicit_experiment_arms_without_starting_a_model() {
    let output = Command::new(env!("CARGO_BIN_EXE_meedoen-retrieval"))
        .args(["experiment", "arms"])
        .env("COA_CODEX_BIN", "/not-a-runtime/PRIVATE-CANARY")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["arms"].as_array().unwrap().len(), 4);
    assert_eq!(value["schema_version"], 1);
}

#[test]
fn live_execution_requires_explicit_opt_in_before_runtime_or_file_access() {
    let path: PathBuf =
        std::env::temp_dir().join(format!("coa-no-implicit-live-{}.jsonl", std::process::id()));
    let output = Command::new(env!("CARGO_BIN_EXE_meedoen-retrieval"))
        .args(["experiment", "run", "--output"])
        .arg(&path)
        .env("COA_CODEX_BIN", "/not-a-runtime/PRIVATE-CANARY")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("requires --live"));
    assert!(!path.exists());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("PRIVATE-CANARY"));
}

#[test]
fn validates_bundled_data_without_authentication() {
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_meedoen-retrieval"))
        .args(["experiment", "validate"])
        .env("COA_CODEX_BIN", "/missing/PRIVATE-CANARY")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(String::from_utf8_lossy(&result.stdout).contains("12"));
}

#[test]
fn live_run_checks_selection_before_runtime_or_creating_artifacts() {
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_meedoen-retrieval"))
        .args([
            "experiment",
            "run",
            "--live",
            "--cases",
            "NOT-A-CASE",
            "--arms",
            "full_context",
            "--output",
            "/missing/output.jsonl",
            "--ledger",
            "/missing/budget.json",
            "--max-cases",
            "1",
            "--max-generations",
            "1",
            "--max-judgements",
            "0",
            "--max-seconds",
            "120",
        ])
        .env("COA_CODEX_BIN", "/missing/PRIVATE-CANARY")
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("Unknown experiment case"));
    assert!(!String::from_utf8_lossy(&result.stderr).contains("PRIVATE-CANARY"));
}

#[test]
fn regrading_rejects_missing_artifact_before_runtime_or_generation() {
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_meedoen-retrieval"))
        .args([
            "experiment",
            "grade",
            "--live",
            "--input",
            "/missing/PRIVATE-CANARY.jsonl",
            "--output",
            "/missing/out.jsonl",
            "--ledger",
            "/missing/budget.json",
            "--max-judgements",
            "4",
            "--max-seconds",
            "600",
        ])
        .env("COA_CODEX_BIN", "/missing/PRIVATE-CANARY")
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("Cannot read experiment artifact"));
    assert!(!String::from_utf8_lossy(&result.stderr).contains("PRIVATE-CANARY"));
}

#[test]
fn renders_a_local_report_without_authentication_or_model_calls() {
    let output = std::env::temp_dir().join(format!(
        "coa-cli-report-{}-{}.html",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_meedoen-retrieval"))
        .args([
            "experiment",
            "report",
            "--input",
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/evals/coa-assistent/fixtures/report-example.jsonl"
            ),
            "--output",
        ])
        .arg(&output)
        .env("COA_CODEX_BIN", "/missing/PRIVATE-CANARY")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let html = std::fs::read_to_string(&output).unwrap();
    assert!(html.contains("<!doctype html>"));
    assert!(!html.contains("PRIVATE-CANARY"));
    std::fs::remove_file(output).unwrap();
}
