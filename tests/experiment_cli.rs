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
