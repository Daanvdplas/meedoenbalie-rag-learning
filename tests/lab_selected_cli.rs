//! Actual CLI/MCP processes; these tests never invoke a model provider.
use meedoen_retrieval::{retrieval::{RetrievalConfig,SearchMethod},retrieval_lab::{lab_mcp::SessionConfig,types::RunConfig},storage::StorageProfile};
use serde_json::{Value,json};
use std::{io::Write,path::PathBuf,process::{Command,Stdio}};

struct Temporary(PathBuf);
impl Drop for Temporary {fn drop(&mut self){let _=std::fs::remove_dir_all(&self.0);}}
fn temporary()->Temporary {
    static NEXT:std::sync::atomic::AtomicU64=std::sync::atomic::AtomicU64::new(0);
    loop {
        let path=std::env::temp_dir().join(format!("coa-lab-mcp-cli-{}-{}",std::process::id(),NEXT.fetch_add(1,std::sync::atomic::Ordering::Relaxed)));
        match std::fs::create_dir(&path) {Ok(())=>return Temporary(path),Err(error)if error.kind()==std::io::ErrorKind::AlreadyExists=>{},Err(error)=>panic!("{error}")}
    }
}

fn protocol(profile:StorageProfile) {
    let base=temporary();
    let session=SessionConfig {schema_version:1,config:RunConfig {profile,retrieval:RetrievalConfig {method:SearchMethod::Bm25,..Default::default()}},filters:Default::default(),model_cache:base.0.join("NO_MODEL"),sqlite_dir:base.0.join("sqlite")};
    let mut child=Command::new(env!("CARGO_BIN_EXE_meedoen-retrieval")).args(["lab","mcp","--session",&serde_json::to_string(&session).unwrap()])
        .env("COA_CODEX_BIN","/missing/NO-CODEX").stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    let mut input=child.stdin.take().unwrap();
    writeln!(input,"{}",json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}})).unwrap();
    writeln!(input,"{}",json!({"jsonrpc":"2.0","id":2,"method":"tools/list"})).unwrap();
    writeln!(input,"{}",json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"search_document","arguments":{"query":"dinsdag binnen","top_k":6}}})).unwrap();
    for id in 4..9 {writeln!(input,"{}",json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":"search_document","arguments":{"query":"dinsdag binnen","top_k":3}}})).unwrap();}
    drop(input);let result=child.wait_with_output().unwrap();
    assert!(result.status.success(),"{}",String::from_utf8_lossy(&result.stderr));
    let replies:Vec<Value>=String::from_utf8(result.stdout).unwrap().lines().map(|line|serde_json::from_str(line).unwrap()).collect();
    assert_eq!(replies.len(),8);assert_eq!(replies[1]["result"]["tools"].as_array().unwrap().len(),1);
    assert_eq!(replies[1]["result"]["tools"][0]["inputSchema"]["properties"]["top_k"]["enum"],json!([3]));
    assert_eq!(replies[2]["error"]["code"],-32602);
    for reply in &replies[3..7] {
        assert_eq!(reply["result"]["isError"],false);
        let evidence:Value=serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(evidence["manifest"]["config"]["profile"],serde_json::to_value(profile).unwrap());
        assert_eq!(evidence["manifest"]["score_kind"],"bm25");
        assert_eq!(evidence["outcome"]["work"]["dense_vectors_scored"],0);
        assert_eq!(evidence["manifest"]["corpus_identity"]["embedding"],Value::Null);
        assert!(!evidence["hits"].as_array().unwrap().is_empty());
    }
    assert_eq!(replies[7]["result"]["isError"],true);
    assert!(replies[7]["result"]["content"][0]["text"].as_str().unwrap().contains("four per Codex run"));
    assert!(!session.model_cache.exists());
}

#[test] fn selected_document_mcp_is_real_bounded_and_keyword_only(){protocol(StorageProfile::DocumentsActivities);}
#[cfg(feature="sqlite")]
#[test] fn selected_sqlite_mcp_uses_the_same_real_protocol(){protocol(StorageProfile::SqliteActivities);}

#[test]
fn live_cli_requires_opt_in_and_valid_single_treatment_without_creating_artifacts() {
    let base=temporary();
    let output=base.0.join("answer.jsonl");let ledger=base.0.join("budget.json");
    let args=["lab","ask","--methods","bm25","--profiles","documents_sections","--question","Wanneer?","--output",output.to_str().unwrap(),"--ledger",ledger.to_str().unwrap(),"--max-generations","1","--max-seconds","1200"];
    let result=Command::new(env!("CARGO_BIN_EXE_meedoen-retrieval")).args(args).env("COA_CODEX_BIN","/missing/NO-CODEX").output().unwrap();
    assert!(!result.status.success());assert!(String::from_utf8_lossy(&result.stderr).contains("--live"));
    assert!(!output.exists());assert!(!ledger.exists());
    let mut invalid=args.to_vec();invalid.push("--live");invalid[3]="bm25,dense";
    let result=Command::new(env!("CARGO_BIN_EXE_meedoen-retrieval")).args(invalid).env("COA_CODEX_BIN","/missing/NO-CODEX").output().unwrap();
    assert!(!result.status.success());assert!(!output.exists());assert!(!ledger.exists());
}
