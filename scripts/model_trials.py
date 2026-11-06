"""One preregistered 60-session comparison, using the existing Rust runner.

No generations are made by ``plan`` or ``assess``. Execution requires ``run --live``.
An uncertain reserved attempt is never retried automatically.
"""

import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import time

DATASET = "strengthened-10-20261106"
MODELS = ("gpt-6.1-sol", "gpt-6-luna")
PILOT_CASES = ("RAG26-01", "RAG26-02")
MAX_ATTEMPTS = 60
MAX_SECONDS = 7200
MODEL_FAILURES = ("model_output_invalid", "model_contract_violation")


def load(path):
    return json.loads(Path(path).read_text())


def json_hash(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode()).hexdigest()


def file_hash(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def write_new(path, value):
    with open(path, "x", encoding="utf-8") as output:
        os.chmod(path, 0o600)
        json.dump(value, output, ensure_ascii=False, indent=2)
        output.write("\n")
        output.flush()
        os.fsync(output.fileno())


def save_state(path, state):
    temporary = path.with_suffix(".next")
    write_new(temporary, state)
    os.replace(temporary, path)


def schedule(case_ids):
    if len(case_ids) != 10 or len(set(case_ids)) != 10 or not set(PILOT_CASES) <= set(case_ids):
        raise ValueError("Exactly ten unique frozen questions, including the two pilot cases, required")
    pairs = [(1, case) for case in PILOT_CASES]
    pairs += [(trial, case) for trial in range(1, 4) for case in case_ids if trial != 1 or case not in PILOT_CASES]
    rows = []
    for pair_index, (trial, case) in enumerate(pairs):
        for model in (MODELS if pair_index % 2 == 0 else MODELS[::-1]):
            rows.append({"slot": len(rows) + 1, "model": model, "case_id": case, "trial": trial})
    return rows


def reserve(state, rows, now):
    attempts = state["attempts"]
    if state["stopped"] or len(attempts) >= MAX_ATTEMPTS:
        raise ValueError("Experiment stopped or all60 attempts consumed")
    if any(a["status"] not in ("completed", *MODEL_FAILURES) for a in attempts):
        raise ValueError("Uncertain or failed attempt exists; never retry automatically")
    if any(any(a.get(key) != row[key] for key in row) for a, row in zip(attempts, rows)):
        raise ValueError("Attempt history differs from preregistered order")
    if state["started_at"] is None:
        state["started_at"] = now
    if now < state["started_at"] or now - state["started_at"] >= MAX_SECONDS:
        raise ValueError("Global7200-second window expired or clock moved backwards")
    if attempts and now < attempts[-1]["reserved_at"]:
        raise ValueError("Clock moved before last reservation")
    attempts.append({**rows[len(attempts)], "status": "reserved", "reserved_at": now})
    return attempts[-1]


def validate_pilot_approval(approval, prereg_hash, attempts):
    if (approval.get("approved") is not True
            or approval.get("preregistration_sha256") != prereg_hash
            or len(attempts) < 4
            or any(a["status"] not in ("completed", *MODEL_FAILURES) for a in attempts[:4])
            or approval.get("pilot_attempts_sha256") != json_hash(attempts[:4])):
        raise ValueError("Independent pilot approval is absent or bound to different evidence")


def model_env(prereg, model):
    return {**os.environ, "COA_CODEX_MODEL": model, "COA_CODEX_BIN": prereg["codex_binary"]}


def verified_model_failure(record, returncode):
    live = record["live"]
    diagnostic = live.get("safe_diagnostic") or {}
    category = live.get("error")
    events = set(diagnostic.get("event_types", []))
    items = set(diagnostic.get("item_types", []))
    if (returncode != 1 or live["status"] != "failed" or live.get("run") is not None
            or category not in MODEL_FAILURES or diagnostic.get("category") != category
            or diagnostic.get("stage") != "agent_event_validation" or diagnostic.get("exit_code") != 0
            or diagnostic.get("json_event_lines_valid") is not True or diagnostic.get("provider_error_code") is not None
            or {"error", "turn.failed"} & events or "error" in items):
        return False
    reason, tool = diagnostic.get("validation_reason"), diagnostic.get("tool_event_category")
    if category == "model_output_invalid":
        return reason == "invalid_model_answer" and "turn.completed" in events and "agent_message" in items
    if reason == "missing_required_search":
        return (tool == "none" and "turn.completed" in events and "agent_message" in items
                and items <= {"reasoning", "agent_message"})
    if reason in ("invalid_search_arguments", "search_limit_exceeded"):
        return tool == "search_document" and "mcp_tool_call" in items
    if reason == "outside_contract_tool":
        return ((tool == "other_mcp_tool" and "mcp_tool_call" in items)
                or tool in ("command_execution", "file_change", "web_search") and tool in items)
    return False


def command_json(command, env):
    result = subprocess.run(command, env=env, capture_output=True, check=True, timeout=30)
    return json.loads(result.stdout)


def descendant_processes(parent):
    """PID/start-time identities only: never collect process arguments/secrets."""
    output = subprocess.check_output(["ps", "-axo", "pid=,ppid=,pgid=,lstart="], text=True)
    processes = {}
    for line in output.splitlines():
        fields = line.split(None, 3)
        if len(fields) == 4:
            processes[int(fields[0])] = (int(fields[1]), int(fields[2]), fields[3])
    owned = {parent} if parent in processes else set()
    while True:
        expanded = owned | {pid for pid, value in processes.items() if value[0] in owned}
        if expanded == owned:
            return {pid: processes[pid] for pid in owned}
        owned = expanded


def run_cancellable(command, env, timeout):
    process = subprocess.Popen(command, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
    try:
        stdout, stderr = process.communicate(timeout=timeout)
        return subprocess.CompletedProcess(command, process.returncode, stdout, stderr)
    except (subprocess.TimeoutExpired, KeyboardInterrupt):
        owned = descendant_processes(process.pid)
        # The Rust parent handles ctrl_c and drops its owned Codex process group.
        process.send_signal(signal.SIGINT)
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
        # A conservative fallback covers already-reparented owned descendants.
        current = subprocess.check_output(["ps", "-axo", "pid=,ppid=,pgid=,lstart="], text=True)
        identities = {}
        for line in current.splitlines():
            fields = line.split(None, 3)
            if len(fields) == 4:
                identities[int(fields[0])] = (int(fields[1]), int(fields[2]), fields[3])
        for pid, identity in owned.items():
            actual = identities.get(pid)
            if actual and actual[1:] == identity[1:]:
                try:
                    if pid == identity[1]:
                        os.killpg(pid, signal.SIGKILL)
                    else:
                        os.kill(pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
        try:
            process.communicate(timeout=5)
        finally:
            process.stdout.close()
            process.stderr.close()
        raise


def make_plan(args):
    binary = Path(args.binary).resolve(strict=True)
    codex = Path(args.codex_binary).resolve(strict=True)
    host = codex.with_name("codex-code-mode-host")
    version = subprocess.check_output([str(codex), "--version"], timeout=15, text=True).strip()
    if "0.166.0" not in version:
        raise ValueError("Both models must use the preregistered0.166.0 runtime")
    plans = {}
    for model in MODELS:
        env = {**os.environ, "COA_CODEX_MODEL": model, "COA_CODEX_BIN": str(codex)}
        plan = command_json([str(binary), "lab", "agent-plan", "--dataset", DATASET, "--methods", "dense"], env)
        if (plan["requested_model"] != model or plan["planned_sessions"] != 10
                or plan["max_search_calls_per_session"] != 4 or plan["max_session_seconds"] != 120):
            raise ValueError("Native input-only plan differs from the intended experiment")
        if any(name in json.dumps(plan) for name in ("evidence_alternatives", "evaluation_only_canary", "required_unknowns")):
            raise ValueError("Protected reference labels in runtime plan")
        plans[model] = plan
    left, right = (plans[m] for m in MODELS)
    if left["runs"] != right["runs"] or left["selection_sha256"] != right["selection_sha256"]:
        raise ValueError("Models differ in something other than model/protocol identity")
    rows = schedule([run["case_id"] for run in left["runs"]])
    directory = Path(args.directory).resolve()
    directory.mkdir(mode=0o700)
    for subdirectory in ("runs", "ledgers", "cohorts"):
        (directory / subdirectory).mkdir(mode=0o700)
    baseline_path = Path(args.retrieval_baseline).resolve(strict=True)
    baseline_records = [json.loads(line) for line in baseline_path.read_text().splitlines()]
    identities = [r["live"]["run"]["manifest"]["corpus_identity"] for r in baseline_records if r["live"]["config"]["retrieval"]["method"] == "dense" and r["live"].get("run")]
    if not identities or any(identity != identities[0] for identity in identities):
        raise ValueError("Existing same-corpus dense retrieval identity is absent or inconsistent")
    previous_stop_path = Path(args.previous_stop_manifest).resolve(strict=True)
    previous_stop = load(previous_stop_path)
    if previous_stop["state"] != "STOPPED_NO_RETRY" or previous_stop["attempts"] != 6:
        raise ValueError("Expected the preserved six-attempt aborted experiment")
    for entry in previous_stop["files"]:
        if file_hash(previous_stop_path.parent / entry["path"]) != entry["sha256"]:
            raise ValueError("An aborted-experiment artifact changed")
    pins = {str(path): file_hash(path) for path in (binary, codex, host, Path(__file__).resolve(), Path(args.freeze).resolve(strict=True), Path(args.analysis_plan).resolve(strict=True), baseline_path, previous_stop_path)}
    prereg = {
        "schema_version": 1, "created_at_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "binary": str(binary), "codex_binary": str(codex), "runtime_version": version,
        "pins": pins, "dataset": DATASET, "models": list(MODELS), "search_method": "dense",
        "retrieval_corpus_identity": identities[0],
        "search_method_reason": "Existing demo default; fixed before new outcomes, not selected by performance",
        "model_cache": str(Path(args.model_cache).resolve(strict=True)), "reasoning_effort": "low",
        "plans": plans, "schedule": rows, "max_generation_attempts": MAX_ATTEMPTS,
        "max_global_seconds": MAX_SECONDS, "max_search_attempts_per_session": 4,
        "max_native_seconds_per_session": 120, "ai_judge_calls": 0,
        "previous_aborted_experiment": {"manifest_path": str(previous_stop_path), "manifest_sha256": file_hash(previous_stop_path), "attempts": 6, "excluded_from_new_comparison": True},
        "total_authorized_attempts_including_aborted": 66,
        "pilot_slots": [1, 2, 3, 4], "pilot_cases": list(PILOT_CASES),
        "pilot_reason": "Integration verification includes the simple time/location case and the case of the prior unclassified tool failure; all ten questions remain in the full comparison without score-based selection",
        "ordering": "Fixed paired question/trial order; model order alternates for each pair; pilot four first",
        "repetition_identity": "Separate model/trial cohort files; one pinned30-attempt ledger per model, one external60-attempt global reservation",
        "failure_policy": "Completed sessions, including rejected tool attempts, are valid model outcomes. A safely evidenced model_output_invalid or model_contract_violation remains a failed native attempt with unknown source coverage. Specific proven model actions need not imply a complete final answer. Broad errors, missing records, trace/pin mismatch, provider/auth/network errors stop; no retries or fallback.",
        "limitations": ["Development questions on synthetic data; no blind holdout or answer-correctness judging", "Equal reasoning labels do not imply equal compute", "This question set is not comparable to the previous35/40set"],
    }
    write_new(directory / "PREREGISTRATION.json", prereg)
    write_new(directory / "STATE.json", {"preregistration_sha256": file_hash(directory / "PREREGISTRATION.json"), "started_at": None, "attempts": [], "stopped": False})
    print(json.dumps({"directory": str(directory), "preregistration_sha256": file_hash(directory / "PREREGISTRATION.json"), "sessions": len(rows)}))


def checked_prereg(directory):
    prereg = load(directory / "PREREGISTRATION.json")
    state = load(directory / "STATE.json")
    if file_hash(directory / "PREREGISTRATION.json") != state["preregistration_sha256"]:
        raise ValueError("Preregistration changed")
    for path, expected in prereg["pins"].items():
        if file_hash(path) != expected:
            raise ValueError(f"Pinned executable/reference/analysis file changed: {path}")
    for attempt in state["attempts"]:
        if attempt["status"] in ("completed", "model_output_invalid") and file_hash(attempt["record_path"]) != attempt["record_sha256"]:
            raise ValueError("Previously saved attempt bytes changed")
    if prereg["schedule"] != schedule([r["case_id"] for r in prereg["plans"][MODELS[0]]["runs"]]):
        raise ValueError("Schedule no longer matches the native plan")
    return prereg, state


def run_phase(args):
    if not args.live:
        raise ValueError("Actual generations require explicit --live")
    directory = Path(args.directory).resolve(strict=True)
    with open(directory / "execution.lock", "a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        prereg, state = checked_prereg(directory)
        if args.phase == "remaining":
            validate_pilot_approval(load(directory / "PILOT_APPROVED.json"), state["preregistration_sha256"], state["attempts"])
        limit = 4 if args.phase == "pilot" else MAX_ATTEMPTS
        while len(state["attempts"]) < limit:
            prereg, state = checked_prereg(directory)
            attempt = reserve(state, prereg["schedule"], time.time())
            slug = "sol" if attempt["model"] == MODELS[0] else "luna"
            stem = f'{attempt["slot"]:02}-{slug}-trial{attempt["trial"]}-{attempt["case_id"]}'
            output = directory / "runs" / f"{stem}.jsonl"
            attempt["record_path"] = str(output)
            save_state(directory / "STATE.json", state)
            command = [prereg["binary"], "lab", "agent-run", "--live", "--dataset", DATASET,
                       "--cases", attempt["case_id"], "--methods", "dense", "--output", str(output),
                       "--ledger", str(directory / "ledgers" / f"{slug}.json"), "--model-cache", prereg["model_cache"],
                       "--max-generations", "30", "--max-seconds", str(MAX_SECONDS)]
            try:
                remaining = MAX_SECONDS - (time.time() - state["started_at"])
                result = run_cancellable(command, model_env(prereg, attempt["model"]), max(1, min(135, remaining)))
                for name, value in (("stdout", result.stdout), ("stderr", result.stderr)):
                    path = directory / "runs" / f"{stem}.{name}.txt"
                    with open(path, "xb") as handle:
                        os.chmod(path, 0o600)
                        handle.write(value)
                records = [json.loads(line) for line in output.read_text().splitlines()]
                if len(records) != 1:
                    raise ValueError("Native execution did not save exactly one attempt")
                record = records[0]
                if record["case_id"] != attempt["case_id"] or record["requested_model"] != attempt["model"]:
                    raise ValueError("Native saved attempt differs from reserved slot")
                if record["live"].get("run"):
                    identity = record["live"]["run"]["manifest"]["corpus_identity"]
                    if identity != prereg["retrieval_corpus_identity"]:
                        raise ValueError("Embedding/source/vector identity changed")
                elif record["live"].get("error") in MODEL_FAILURES:
                    trace = record["live"]["audit_trace"]
                    if file_hash(trace["path"]) != trace["sha256"]:
                        raise ValueError("Failed-answer trace bytes changed")
                    observed = [json.loads(line)["full_response"]["manifest"]["corpus_identity"] for line in Path(trace["path"]).read_text().splitlines()]
                    if ((not observed and record["live"]["error"] == "model_output_invalid")
                            or any(value != prereg["retrieval_corpus_identity"] for value in observed)):
                        raise ValueError("Failed-answer retrieval identity is absent or changed")
                attempt["record_sha256"] = file_hash(output)
                attempt["run_id"] = record["live"]["run_id"]
                attempt["native_generation_index"] = record["live"]["generation_session_index"]
                attempt["native_budget_id"] = record["live"]["budget_id"]
                attempt["status"] = record["live"]["status"]
                if verified_model_failure(record, result.returncode):
                    attempt["status"] = record["live"]["error"]
                elif result.returncode != 0 or attempt["status"] != "completed":
                    raise ValueError("Technical or unclassified native failure; stop without retry")
                attempt["finished_at"] = time.time()
                save_state(directory / "STATE.json", state)
                print(json.dumps({key: attempt[key] for key in ("slot", "model", "case_id", "trial", "status")}), flush=True)
            except Exception as error:
                attempt["status"] = "technical_failure_or_uncertain"
                attempt["failure_type"] = type(error).__name__
                state["stopped"] = True
                save_state(directory / "STATE.json", state)
                raise


def cohort_complete(state):
    return (len(state["attempts"]) == MAX_ATTEMPTS and not state["stopped"]
            and all(a["status"] in ("completed", *MODEL_FAILURES) for a in state["attempts"]))


def assess_cohorts(args):
    directory = Path(args.directory).resolve(strict=True)
    prereg, state = checked_prereg(directory)
    if not cohort_complete(state):
        raise ValueError("A full non-stopped60-attempt cohort is required; partial evidence remains saved")
    index = []
    for model in MODELS:
        for trial in range(1, 4):
            attempts = [a for a in state["attempts"] if a["model"] == model and a["trial"] == trial]
            if len(attempts) != 10 or len({a["case_id"] for a in attempts}) != 10:
                raise ValueError("Incomplete or duplicate model/trial cohort")
            stem = ("sol" if model == MODELS[0] else "luna") + f"-trial{trial}"
            records_path = directory / "cohorts" / f"{stem}.jsonl"
            with open(records_path, "xb") as output:
                os.chmod(records_path, 0o600)
                for attempt in attempts:
                    path = Path(attempt["record_path"])
                    if file_hash(path) != attempt["record_sha256"]:
                        raise ValueError("Native attempt bytes changed")
                    output.write(path.read_bytes())
            assessment_path = records_path.with_suffix(".assessment.jsonl")
            subprocess.run([prereg["binary"], "lab", "agent-assess", "--input", str(records_path), "--output", str(assessment_path)], check=True)
            report_path = records_path.with_suffix(".report.json")
            subprocess.run([prereg["binary"], "lab", "agent-report", "--input", str(records_path), "--assessments", str(assessment_path), "--output", str(report_path)], env=model_env(prereg, model), check=True)
            for attempt in attempts:
                index.append({**attempt, "cohort_records_path": str(records_path), "assessment_path": str(assessment_path), "report_path": str(report_path)})
    write_new(directory / "ANALYSIS_INDEX.json", {"schema_version": 1, "preregistration_sha256": state["preregistration_sha256"], "attempts": index, "ai_judge_calls": 0})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    plan = commands.add_parser("plan")
    for flag in ("binary", "codex-binary", "freeze", "analysis-plan", "retrieval-baseline", "previous-stop-manifest", "model-cache", "directory"):
        plan.add_argument("--" + flag, required=True)
    run = commands.add_parser("run")
    run.add_argument("--directory", required=True)
    run.add_argument("--phase", choices=("pilot", "remaining"), required=True)
    run.add_argument("--live", action="store_true")
    assess = commands.add_parser("assess")
    assess.add_argument("--directory", required=True)
    args = parser.parse_args()
    {"plan": make_plan, "run": run_phase, "assess": assess_cohorts}[args.command](args)


if __name__ == "__main__":
    main()
