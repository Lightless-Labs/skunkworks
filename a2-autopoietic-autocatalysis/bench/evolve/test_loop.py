import json
import tempfile
import unittest
from argparse import Namespace
from pathlib import Path
from unittest.mock import patch

from bench.evolve import loop


class LoopTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.private = self.root / "private"
        self.private.mkdir()
        self.repo = self.root / "repo"
        (self.repo / "genome").mkdir(parents=True)
        (self.repo / "genome" / "policy.py").write_text("champion\n")
        self.cfg = Namespace(dev_tasks=self.root / "dev.json", screen=self.root / "screen.json",
                             champion_runs=["initial"], shards=4, max_experiments=3,
                             nightly_token_cap=150000000, control_every_hours=12, dry_run=True,
                             repo_root=self.repo, dry_run_scenarios=["invalid", "discard", "keep"])
        self.cfg.dev_tasks.write_text(json.dumps(["d1", "d2"]))
        self.cfg.screen.write_text(json.dumps(["d1"]))
        self.patches = [
            patch.object(loop.tasks, "private_dir", return_value=self.private),
            patch.object(loop.lock, "verify", return_value=(True, [])),
            patch.object(loop.researcher.grading, "init_history_free_repo"),
        ]
        for item in self.patches:
            item.start()
            self.addCleanup(item.stop)

    def tearDown(self):
        self.temp.cleanup()

    def test_dry_run_invalid_discard_keep_resets_rates_and_commits_exact_paths(self):
        commands = []
        with patch.object(loop.subprocess, "run", side_effect=lambda argv, **kwargs:
                          commands.append(argv) or type("Result", (), {"returncode": 0})()), \
             patch.object(loop, "_dry_run_researcher", side_effect=loop._dry_run_researcher), \
             patch.object(loop.stats, "screen_decision", side_effect=[{"pass": False, "delta": -1, "threshold": 0},
                                                                         {"pass": True, "delta": 1, "threshold": 0}]), \
             patch.object(loop.stats, "stage_b_decision", return_value={"keep": True, "delta": 2,
                                                                          "p_value": .01, "full_delta": 1}), \
             patch.object(loop, "_dry_evaluate", wraps=loop._dry_evaluate) as eval_mock:
            result = loop.run_loop(self.cfg)
        self.assertEqual([row["decision"] for row in result["rows"]], ["invalid", "discard", "keep"])
        self.assertEqual(result["champion_rates"], {"d1": (3, 3), "d2": (1, 1)})
        self.assertEqual(eval_mock.call_count, 4)  # stage A, then stage B screen × 2 and full pool
        self.assertEqual(commands, [])
        self.assertTrue((self.repo / "genome" / "policy.py").read_text().startswith("# synthetic candidate"))

    def test_resume_state_mid_stage_b_skips_stage_a_and_researcher(self):
        state_dir = self.private / "evolve-loop"
        state_dir.mkdir()
        candidate = self.root / "resumed-candidate"
        candidate.mkdir()
        (candidate / "policy.py").write_text("candidate\n")
        state = {"exp_index": 0, "next_exp": 1, "tokens": 3, "champion_genome": str(self.repo / "genome"),
                 "champion_rates": {"d1": [1, 1]}, "champion_rows": [], "last_control": 0,
                 "current": {"exp_id": "exp-0001", "stage": "stage_b", "candidate": str(candidate),
                             "hypothesis": "resume hypothesis", "parent": "g0", "tokens": 3,
                             "stage_a_rows": [], "stage_b_screen_run_ids": ["b1", "b2"],
                             "full_run_id": "f1", "screen_tasks": ["d1"], "full_tasks": ["d1"]},
                 "log_rows": []}
        (state_dir / "state.json").write_text(json.dumps(state))
        with patch.object(loop, "_dry_run_researcher", side_effect=AssertionError("researcher reran")), \
             patch.object(loop.stats, "stage_b_decision", return_value={"keep": False, "delta": 0,
                                                                          "p_value": 1, "full_delta": 0}), \
             patch.object(loop.subprocess, "run", return_value=type("Result", (), {"returncode": 0})()):
            result = loop.run_loop(self.cfg)
        self.assertEqual(result["rows"][0]["decision"], "discard")

    def test_token_cap_stops_before_experiment(self):
        self.cfg.nightly_token_cap = 0
        result = loop.run_loop(self.cfg)
        self.assertEqual(result["rows"], [])
        self.assertEqual(result["stop_reason"], "nightly token cap exceeded")

    def test_token_window_counts_only_the_last_day(self):
        state = {"token_events": [[1000.0, 90], [1000.0 + 23 * 3600, 5], [1000.0 + 26 * 3600, 7]]}
        self.assertEqual(loop.window_tokens(state, 1000.0 + 26 * 3600), 12)
        self.assertEqual(loop.window_tokens({}, 0.0), 0)

    def test_commit_helper_passes_exact_paths(self):
        paths = ["docs/benchmark-results/evolve/log.jsonl", "docs/benchmark-results/evolve/archive/e.patch"]
        commands = []
        with patch.object(loop.subprocess, "run", side_effect=lambda argv, **kwargs:
                          commands.append(argv) or type("Result", (), {"returncode": 0})()):
            loop._commit_experiment(self.repo, "e", "discard", "hypothesis", paths)
        git_add = next(argv for argv in commands if "add" in argv)
        git_commit = next(argv for argv in commands if "commit" in argv)
        self.assertEqual(git_add[-len(paths):], paths)
        self.assertEqual(git_commit[-len(paths):], paths)
        self.assertNotIn("-A", git_add)
        self.assertNotIn(".", git_add)


if __name__ == "__main__":
    unittest.main()
