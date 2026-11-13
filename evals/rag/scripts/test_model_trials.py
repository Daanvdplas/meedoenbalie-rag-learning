import tempfile
import unittest
from pathlib import Path
import os
import subprocess
import sys

import model_trials


class ModelTrialTests(unittest.TestCase):
    def test_schedule_contains_each_model_question_trial_once_and_balanced_pilot(self):
        rows = model_trials.schedule([f"RAG26-{i:02}" for i in range(1, 11)])
        self.assertEqual(len(rows), 60)
        self.assertEqual(len({(r["model"], r["case_id"], r["trial"]) for r in rows}), 60)
        self.assertEqual(
            [(r["model"], r["case_id"], r["trial"]) for r in rows[:4]],
            [("gpt-6.1-sol", "RAG26-01", 1), ("gpt-6-luna", "RAG26-01", 1),
             ("gpt-6-luna", "RAG26-02", 1), ("gpt-6.1-sol", "RAG26-02", 1)],
        )

    def test_reservation_cannot_repeat_an_uncertain_attempt(self):
        rows = model_trials.schedule([f"RAG26-{i:02}" for i in range(1, 11)])
        state = {"started_at": None, "attempts": [], "stopped": False}
        model_trials.reserve(state, rows, 100)
        self.assertEqual(state["attempts"][0]["status"], "reserved")
        with self.assertRaises(ValueError):
            model_trials.reserve(state, rows, 101)

    def test_global_generation_and_time_caps_never_extend_on_resume(self):
        rows = model_trials.schedule([f"RAG26-{i:02}" for i in range(1, 11)])
        state = {"started_at": 100, "attempts": [], "stopped": False}
        for _ in range(60):
            model_trials.reserve(state, rows, 101)
            state["attempts"][-1]["status"] = "completed"
        with self.assertRaises(ValueError):
            model_trials.reserve(state, rows, 102)
        state["attempts"] = []
        with self.assertRaises(ValueError):
            model_trials.reserve(state, rows, 7300)

    def test_attempt_order_tampering_is_rejected(self):
        rows = model_trials.schedule([f"RAG26-{i:02}" for i in range(1, 11)])
        state = {"started_at": 100, "attempts": [{**rows[1], "status": "completed"}], "stopped": False}
        with self.assertRaises(ValueError):
            model_trials.reserve(state, rows, 101)

    def test_last_reserved_attempt_is_not_a_completed_cohort(self):
        state = {"attempts": [{"status": "completed"} for _ in range(60)], "stopped": False}
        self.assertTrue(model_trials.cohort_complete(state))
        state["attempts"][-1]["status"] = "reserved"
        self.assertFalse(model_trials.cohort_complete(state))

    def test_exact_tool_action_can_be_known_without_claiming_a_complete_answer(self):
        record = {"live": {"status": "failed", "run": None, "error": "model_contract_violation", "safe_diagnostic": {
            "category": "model_contract_violation", "stage": "agent_event_validation", "exit_code": 0,
            "json_event_lines_valid": True, "event_types": ["item.completed"], "item_types": ["mcp_tool_call"],
            "provider_error_code": None, "validation_reason": "outside_contract_tool", "tool_event_category": "other_mcp_tool"}}}
        self.assertTrue(model_trials.verified_model_failure(record, 1))
        self.assertFalse(model_trials.verified_model_failure(record, -9))
        record["live"]["safe_diagnostic"]["validation_reason"] = "missing_required_search"
        record["live"]["safe_diagnostic"]["tool_event_category"] = "none"
        self.assertFalse(model_trials.verified_model_failure(record, 1))
        record["live"]["error"] = "unexpected_tool"
        self.assertFalse(model_trials.verified_model_failure(record, 1))

    def test_manifest_output_creation_never_overwrites_existing_evidence(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "frozen.json"
            model_trials.write_new(path, {"fixed": True})
            with self.assertRaises(FileExistsError):
                model_trials.write_new(path, {"fixed": False})
            self.assertTrue(model_trials.load(path)["fixed"])

    def test_pilot_approval_is_bound_to_exact_first_four_attempts(self):
        attempts = [{"slot": n, "status": "completed"} for n in range(1, 5)]
        approval = {"approved": True, "preregistration_sha256": "abc", "pilot_attempts_sha256": model_trials.json_hash(attempts)}
        model_trials.validate_pilot_approval(approval, "abc", attempts)
        attempts[0]["status"] = "failed"
        with self.assertRaises(ValueError):
            model_trials.validate_pilot_approval(approval, "abc", attempts)

    def test_timeout_cleans_a_descendant_with_its_own_process_group(self):
        with tempfile.TemporaryDirectory() as directory:
            pid_file = Path(directory) / "child.pid"
            child = "import time;time.sleep(60)"
            parent = (
                "import subprocess,sys,time;from pathlib import Path;"
                "p=subprocess.Popen([sys.executable,'-c',sys.argv[2]],start_new_session=True);"
                "Path(sys.argv[1]).write_text(str(p.pid));time.sleep(60)"
            )
            with self.assertRaises(subprocess.TimeoutExpired):
                model_trials.run_cancellable([sys.executable, "-c", parent, str(pid_file), child], os.environ, 0.5)
            pid = int(pid_file.read_text())
            observed = subprocess.run(["ps", "-p", str(pid), "-o", "stat="], capture_output=True, text=True)
            self.assertTrue(observed.returncode != 0 or observed.stdout.strip().startswith("Z"))


if __name__ == "__main__":
    unittest.main()
