from __future__ import annotations

import importlib.util
import unittest
from pathlib import Path

POLICY_PATH = Path(__file__).with_name("policy.py")
SPEC = importlib.util.spec_from_file_location("g0_policy", POLICY_PATH)
policy = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(policy)


class G0PolicyTests(unittest.TestCase):
    def test_replay_empty_state_starts_fresh(self):
        state = {"problem_statement": "repair", "budget": {"agent_calls_left": 3}, "candidates": []}
        self.assertEqual(policy.choose_action(state, "high"),
                         {"agent": {"base": "fresh", "prompt": "repair", "effort": "high"}})

    def test_replay_failed_candidate_retries_with_e1_header(self):
        state = {"problem_statement": "repair", "budget": {"agent_calls_left": 2},
                 "candidates": [{"index": 0, "verify": {"passed": False, "tests": [
                     {"outcome": "fail", "name": "test_x", "message": "assert 1 == 2"}]}}]}
        action = policy.choose_action(state, "high")
        self.assertEqual(action["agent"]["base"], "fresh")
        self.assertIn(policy.FEEDBACK_HEADER, action["agent"]["prompt"])
        self.assertIn("assert 1 == 2", action["agent"]["prompt"])

    def test_replay_passing_last_candidate_submits_it(self):
        state = {"problem_statement": "repair", "budget": {"agent_calls_left": 1},
                 "candidates": [{"index": 0, "verify": {"passed": False}},
                                {"index": 1, "verify": {"passed": True}}]}
        self.assertEqual(policy.choose_action(state), {"submit": {"candidate": 1}})

    def test_replay_exhausted_budget_selects_most_visible_passes_latest_tie(self):
        state = {"problem_statement": "repair", "budget": {"agent_calls_left": 0},
                 "candidates": [{"index": 0, "verify": {"tests": [{"outcome": "pass"}]}},
                                {"index": 1, "verify": {"tests": [{"outcome": "pass"}]}},
                                {"index": 2, "verify": {"tests": [{"outcome": "fail"}]}}]}
        self.assertEqual(policy.choose_action(state), {"submit": {"candidate": 1}})


if __name__ == "__main__":
    unittest.main()
