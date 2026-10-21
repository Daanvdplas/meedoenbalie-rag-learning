//! These processes replace the local Codex executable; they make no model calls.
#![cfg(unix)]
use serde_json::{Value, json};
use std::{os::unix::fs::PermissionsExt, path::PathBuf, process::Command};

struct Runtime(PathBuf);
impl Drop for Runtime {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn runtime(version: &str) -> Runtime {
    let directory = Runtime(std::env::temp_dir().join(format!(
        "coa-FAKE-PRIVATE-HOME-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    )));
    std::fs::create_dir(&directory.0).unwrap();
    let result = json!({"sources":[{"id":"ACT-TUIN-001","date":"2026-10-20"}],"as_of":"2026-10-20","manifest":{"source_sha256":"fake-source-hash"},"hits":[{"section":{"source_id":"ACT-TUIN-001","source_date":"2026-10-20","id":"S2","text":"Iedere dinsdag van 10.00 tot 12.00 uur."}}]});
    let answer = json!({"text":"Dinsdag 10–12.","citations":[{"source_id":"ACT-TUIN-001","source_date":"2026-10-20","section_id":"S2","quote":"Iedere dinsdag van 10.00 tot 12.00 uur."}],"unknowns":[]});
    let events = [
        json!({"type":"item.completed","item":{"type":"mcp_tool_call","server":"coa","tool":"search_document","arguments":{"query":"Wanneer tuinieren?","top_k":3},"status":"completed","result":{"content":[{"type":"text","text":result.to_string()}]}}}),
        json!({"type":"item.completed","item":{"type":"agent_message","text":answer.to_string()}}),
        json!({"type":"turn.completed"}),
    ].iter().map(Value::to_string).collect::<Vec<_>>().join("\n");
    let script = format!(
        "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then\ncat <<'COA_VERSION'\n{version}\nCOA_VERSION\nexit 0\nfi\ncat >/dev/null\nprintf 'FAKE-PRIVATE-TOKEN /Users/FAKE-PRIVATE-HOME' >&2\ncat <<'COA_EVENTS'\n{events}\nCOA_EVENTS\n"
    );
    let executable = directory.0.join("codex");
    std::fs::write(&executable, script).unwrap();
    std::fs::set_permissions(executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::write(directory.0.join("codex-code-mode-host"), b"fake host").unwrap();
    directory
}
fn ask(runtime: &Runtime) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_meedoen-retrieval"))
        .env("COA_CODEX_BIN", runtime.0.join("codex"))
        .env("COA_CODEX_MODEL", "test-model")
        .args(["ask", "Wanneer tuinieren?"])
        .output()
        .unwrap()
}
#[test]
fn cli_success_keeps_evidence_and_excludes_host_paths_and_child_diagnostics() {
    let output = ask(&runtime("codex-cli 0.163.0"));
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(!text.contains("FAKE-PRIVATE"));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("FAKE-PRIVATE"));
    let result: Value = serde_json::from_str(&text).unwrap();
    assert!(result.get("runtime_path").is_none());
    assert_eq!(result["client_version"], "codex-cli 0.163.0");
    assert_eq!(result["answer"]["citations"][0]["section_id"], "S2");
}
#[test]
fn unexpected_version_diagnostics_are_rejected_without_echoing_them() {
    let output = ask(&runtime(
        "codex-cli 0.163.0\nFAKE-PRIVATE-TOKEN /Users/FAKE-PRIVATE-HOME",
    ));
    assert!(
        !output.status.success(),
        "unexpected private metadata accepted"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stdout.contains("FAKE-PRIVATE"));
    assert!(!stderr.contains("FAKE-PRIVATE"));
    assert!(stderr.contains("Invalid Codex version"));
}
