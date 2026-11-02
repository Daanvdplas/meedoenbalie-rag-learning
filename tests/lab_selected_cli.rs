//! Actual CLI/MCP processes; these tests never invoke a model provider.
use meedoen_retrieval::{
    retrieval::{RetrievalConfig, SearchMethod},
    retrieval_lab::{lab_mcp::SessionConfig, types::RunConfig},
    storage::StorageProfile,
};
use serde_json::{Value, json};
use std::{
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
};

struct Temporary(PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn temporary() -> Temporary {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    loop {
        let path = std::env::temp_dir().join(format!(
            "coa-lab-mcp-cli-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        match std::fs::create_dir(&path) {
            Ok(()) => return Temporary(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => panic!("{error}"),
        }
    }
}

fn protocol(profile: StorageProfile) {
    let base = temporary();
    let session = SessionConfig {
        schema_version: 1,
        config: RunConfig {
            profile,
            retrieval: RetrievalConfig {
                method: SearchMethod::Bm25,
                ..Default::default()
            },
        },
        filters: Default::default(),
        model_cache: base.0.join("NO_MODEL"),
        sqlite_dir: base.0.join("sqlite"),
    };
    let mut child = Command::new(env!("CARGO_BIN_EXE_meedoen-retrieval"))
        .args([
            "lab",
            "mcp",
            "--session",
            &serde_json::to_string(&session).unwrap(),
        ])
        .env("COA_CODEX_BIN", "/missing/NO-CODEX")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    writeln!(input,"{}",json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}})).unwrap();
    writeln!(
        input,
        "{}",
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"})
    )
    .unwrap();
    writeln!(input,"{}",json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"search_document","arguments":{"query":"dinsdag binnen","top_k":6}}})).unwrap();
    for id in 4..9 {
        writeln!(input,"{}",json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":"search_document","arguments":{"query":"dinsdag binnen","top_k":3}}})).unwrap();
    }
    drop(input);
    let result = child.wait_with_output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let replies: Vec<Value> = String::from_utf8(result.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(replies.len(), 8);
    assert_eq!(replies[1]["result"]["tools"].as_array().unwrap().len(), 1);
    assert_eq!(
        replies[1]["result"]["tools"][0]["inputSchema"]["properties"]["top_k"]["enum"],
        json!([3])
    );
    assert_eq!(replies[2]["error"]["code"], -32602);
    for reply in &replies[3..7] {
        assert_eq!(reply["result"]["isError"], false);
        let evidence: Value =
            serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(
            evidence["manifest"]["config"]["profile"],
            serde_json::to_value(profile).unwrap()
        );
        assert_eq!(evidence["manifest"]["score_kind"], "bm25");
        assert_eq!(evidence["outcome"]["work"]["dense_vectors_scored"], 0);
        assert_eq!(
            evidence["manifest"]["corpus_identity"]["embedding"],
            Value::Null
        );
        assert!(!evidence["hits"].as_array().unwrap().is_empty());
    }
    assert_eq!(replies[7]["result"]["isError"], true);
    assert!(
        replies[7]["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("four per Codex run")
    );
    assert!(!session.model_cache.exists());
}

#[test]
fn selected_document_mcp_is_real_bounded_and_keyword_only() {
    protocol(StorageProfile::DocumentsActivities);
}
#[cfg(feature = "sqlite")]
#[test]
fn selected_sqlite_mcp_uses_the_same_real_protocol() {
    protocol(StorageProfile::SqliteActivities);
}

#[test]
fn live_cli_requires_opt_in_and_valid_single_treatment_without_creating_artifacts() {
    let base = temporary();
    let output = base.0.join("answer.jsonl");
    let ledger = base.0.join("budget.json");
    let args = [
        "lab",
        "ask",
        "--methods",
        "bm25",
        "--profiles",
        "documents_sections",
        "--question",
        "Wanneer?",
        "--output",
        output.to_str().unwrap(),
        "--ledger",
        ledger.to_str().unwrap(),
        "--max-generations",
        "1",
        "--max-seconds",
        "1200",
    ];
    let result = Command::new(env!("CARGO_BIN_EXE_meedoen-retrieval"))
        .args(args)
        .env("COA_CODEX_BIN", "/missing/NO-CODEX")
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("--live"));
    assert!(!output.exists());
    assert!(!ledger.exists());
    let mut invalid = args.to_vec();
    invalid.push("--live");
    invalid[3] = "bm25,dense";
    let result = Command::new(env!("CARGO_BIN_EXE_meedoen-retrieval"))
        .args(invalid)
        .env("COA_CODEX_BIN", "/missing/NO-CODEX")
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(!output.exists());
    assert!(!ledger.exists());
}

#[cfg(unix)]
#[test]
fn cancelling_a_live_cli_stops_its_child_and_keeps_the_reserved_failure() {
    use std::os::unix::fs::PermissionsExt;
    let base = temporary();
    let runtime = base.0.join("codex");
    let marker = base.0.join("CHILD_STARTED");
    let script = format!(
        "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then printf 'codex-cli 0.163.0\\n';exit 0;fi\nprintf '%s' \"$$\" > '{}'\ncat >/dev/null\nsleep 30\n",
        marker.display()
    );
    std::fs::write(&runtime, script).unwrap();
    std::fs::set_permissions(&runtime, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::write(base.0.join("codex-code-mode-host"), b"fixture host").unwrap();
    let output = base.0.join("answer.jsonl");
    let ledger = base.0.join("budget.json");
    let mut child = Command::new(env!("CARGO_BIN_EXE_meedoen-retrieval"))
        .args([
            "lab",
            "ask",
            "--live",
            "--methods",
            "bm25",
            "--profiles",
            "documents_sections",
            "--question",
            "Wanneer?",
            "--output",
            output.to_str().unwrap(),
            "--ledger",
            ledger.to_str().unwrap(),
            "--max-generations",
            "1",
            "--max-seconds",
            "60",
        ])
        .env("COA_CODEX_BIN", &runtime)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    for _ in 0..800 {
        if std::fs::read_to_string(&marker)
            .ok()
            .and_then(|pid| pid.parse::<i32>().ok())
            .is_some()
            || child.try_wait().unwrap().is_some()
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    if !marker.exists() {
        let _ = child.kill();
        let result = child.wait_with_output().unwrap();
        panic!(
            "Fixture runtime never started: {} {}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
    }
    let owned_group: i32 = std::fs::read_to_string(&marker).unwrap().parse().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(50));
    unsafe {
        libc::kill(child.id() as i32, libc::SIGINT);
    }
    let result = child.wait_with_output().unwrap();
    let mut child_stopped = false;
    for _ in 0..100 {
        child_stopped = unsafe { libc::kill(owned_group, 0) } != 0;
        if child_stopped {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    // Cleanup remains restricted to the fixture's own child-owned process group.
    if !child_stopped {
        unsafe {
            libc::kill(-owned_group, libc::SIGKILL);
        }
    }
    let saved: Value = serde_json::from_slice(&std::fs::read(&output).unwrap()).unwrap();
    assert!(!result.status.success());
    assert_eq!(saved["status"], "failed");
    assert_eq!(saved["error"], "cancelled");
    assert_eq!(saved["generation_session_index"], 1);
    assert_eq!(saved["ai_judge_calls"], 0);
    assert!(
        child_stopped,
        "Cancelling the CLI must stop the owned runtime group"
    );
}

#[cfg(unix)]
fn process_running(pid: i32) -> bool {
    let status = Command::new("/bin/ps")
        .args(["-o", "stat=", "-p", &pid.to_string()])
        .output()
        .unwrap();
    status.status.success()
        && !String::from_utf8_lossy(&status.stdout)
            .trim()
            .starts_with('Z')
}

#[cfg(unix)]
fn verify_stalled_version_probe(cancel: bool) {
    use std::{
        os::unix::{fs::PermissionsExt, process::CommandExt},
        time::{Duration, Instant},
    };
    let base = temporary();
    let runtime = base.0.join("codex");
    let marker = base.0.join("PROBE_STARTED");
    let generation = base.0.join("GENERATION_STARTED");
    let script = format!(
        "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then\n sleep 30 &\n printf '%s %s' \"$$\" \"$!\" > '{}'\n wait\n printf 'codex-cli 0.163.0\\n'\n exit 0\nfi\nprintf 'started' > '{}'\nexit 1\n",
        marker.display(),
        generation.display()
    );
    std::fs::write(&runtime, script).unwrap();
    std::fs::set_permissions(&runtime, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::write(base.0.join("codex-code-mode-host"), b"fixture host").unwrap();
    let output = base.0.join("answer.jsonl");
    let ledger = base.0.join("budget.json");
    let started = Instant::now();
    let mut child = Command::new(env!("CARGO_BIN_EXE_meedoen-retrieval"))
        .args([
            "lab",
            "ask",
            "--live",
            "--methods",
            "bm25",
            "--profiles",
            "documents_sections",
            "--question",
            "Wanneer?",
            "--output",
            output.to_str().unwrap(),
            "--ledger",
            ledger.to_str().unwrap(),
            "--max-generations",
            "1",
            "--max-seconds",
            if cancel { "60" } else { "1" },
        ])
        .env("COA_CODEX_BIN", &runtime)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()
        .unwrap();
    let owned_cli_group = child.id() as i32;
    while started.elapsed() < Duration::from_secs(2)
        && std::fs::read_to_string(&marker).ok().is_none_or(|text| {
            text.split_whitespace()
                .filter_map(|pid| pid.parse::<i32>().ok())
                .count()
                != 2
        })
        && child.try_wait().unwrap().is_none()
    {
        std::thread::sleep(Duration::from_millis(10));
    }
    let pids: Vec<i32> = std::fs::read_to_string(&marker)
        .unwrap_or_default()
        .split_whitespace()
        .map(|pid| pid.parse().unwrap())
        .collect();
    if cancel && pids.len() == 2 {
        std::thread::sleep(Duration::from_millis(30));
        unsafe {
            libc::kill(owned_cli_group, libc::SIGINT);
        }
    }
    let deadline = Instant::now() + Duration::from_secs(3);
    while child.try_wait().unwrap().is_none() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    let exceeded = child.try_wait().unwrap().is_none();
    if exceeded {
        unsafe {
            libc::kill(-owned_cli_group, libc::SIGKILL);
        }
    }
    let result = child.wait_with_output().unwrap();
    let elapsed = started.elapsed();
    std::thread::sleep(Duration::from_millis(60));
    let surviving: Vec<_> = pids
        .iter()
        .copied()
        .filter(|pid| process_running(*pid))
        .collect();
    // The watchdog cleans up only this regression's explicitly owned groups/PIDs.
    unsafe {
        libc::kill(-owned_cli_group, libc::SIGKILL);
    }
    for pid in &surviving {
        unsafe {
            libc::kill(-*pid, libc::SIGKILL);
            libc::kill(*pid, libc::SIGKILL);
        }
    }
    assert_eq!(pids.len(), 2, "The actual version probe must have started");
    assert!(
        !exceeded,
        "A stalled --version probe must be bounded/cancellable"
    );
    if !cancel {
        assert!(
            elapsed < Duration::from_millis(2500),
            "A one-second budget cannot wait for a stalled probe: {elapsed:?}"
        );
    }
    let saved: Value = serde_json::from_slice(&std::fs::read(&output).unwrap()).unwrap();
    assert!(!result.status.success());
    assert_eq!(saved["status"], "failed");
    assert_eq!(saved["error"], if cancel { "cancelled" } else { "timeout" });
    assert_eq!(saved["generation_session_index"], 1);
    assert_eq!(saved["ai_judge_calls"], 0);
    assert!(saved["run"].is_null());
    assert!(!generation.exists());
    assert!(
        surviving.is_empty(),
        "Version probe and descendants must stop: {surviving:?}"
    );
    let budget: Value = serde_json::from_slice(&std::fs::read(&ledger).unwrap()).unwrap();
    assert_eq!(budget["attempts"].as_array().unwrap().len(), 1);
}

#[cfg(unix)]
#[test]
fn stalled_version_probe_is_inside_the_global_deadline() {
    verify_stalled_version_probe(false);
}

#[cfg(unix)]
#[test]
fn cancelling_during_version_probe_preserves_failure_and_stops_probe_group() {
    verify_stalled_version_probe(true);
}
