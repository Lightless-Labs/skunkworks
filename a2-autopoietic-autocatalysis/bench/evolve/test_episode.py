from __future__ import annotations

import argparse
import hashlib
import json
import os
import subprocess
import tempfile
import unittest
from contextlib import nullcontext
from pathlib import Path
from unittest import mock

from bench.evolve import episode


def git(cwd: Path, *args: str) -> str:
    return subprocess.run(["git", "-c", "commit.gpgsign=false", *args], cwd=cwd,
                          check=True, capture_output=True, text=True, timeout=60).stdout


class EpisodeTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.repo_tmp = tempfile.TemporaryDirectory()
        cls.pristine = Path(cls.repo_tmp.name) / "pristine"
        cls.pristine.mkdir()
        (cls.pristine / "answer.txt").write_text("base\n")
        git(cls.pristine, "init", "-q")
        git(cls.pristine, "add", "-A")
        git(cls.pristine, "-c", "user.name=test", "-c", "user.email=test@localhost", "commit",
            "--no-verify", "-qm", "base")

    @classmethod
    def tearDownClass(cls):
        cls.repo_tmp.cleanup()

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.addCleanup(self._make_writable)
        self.base = self.root / "base"
        import shutil
        shutil.copytree(self.pristine, self.base)
        self.run_dir = self.root / "episode"
        self.run_dir.mkdir()
        self.private = self.root / "private"
        (self.private / "opaque-secret").mkdir(parents=True)
        (self.private / "opaque-secret" / "gold.patch").write_text("gold sentinel\n")
        self.cfg = argparse.Namespace(timeout=1500, a2ctl="unused", provider="stub", max_tokens=2000000,
                                      endpoint=[], proxy_port=0, codex_home=self.root / "codex-home",
                                      base_snapshot=self.base, problem_statement="repair it")

    def _make_writable(self):
        if self.root.exists():
            for path in [self.root, *self.root.rglob("*")]:
                try:
                    path.chmod(0o755 if path.is_dir() else 0o644)
                except OSError:
                    pass

    def _patch(self, old: str, new: str) -> str:
        target = self.root / "patch-work"
        if target.exists():
            import shutil
            shutil.rmtree(target)
        import shutil
        shutil.copytree(self.base, target)
        (target / "answer.txt").write_text(new)
        return git(target, "diff")

    @staticmethod
    def _line_patch(old: str, new: str) -> str:
        return ("diff --git a/answer.txt b/answer.txt\n--- a/answer.txt\n+++ b/answer.txt\n"
                "@@ -1 +1 @@\n-" + old + "+" + new)

    def _setup_fakes(self, action_sequence, patch_diffs):
        calls = []
        rows = []
        def policy(_genome, state, _snapshot):
            calls.append(state)
            action = action_sequence[len(calls) - 1]
            return action if isinstance(action, str) else json.dumps(action), "", 0
        def a2ctl(_cfg, opaque, workspace, payload, emit, log, sealed_root):
            self.assertEqual(payload["verification_commands"], [])
            self.assertEqual(payload["task_id"].split(":", 1)[0], "opaque-secret")
            self.assertEqual(opaque, "opaque-secret")
            self.assertEqual(sealed_root, self.root / "e1-home" / "sealed")
            patch_diff = patch_diffs[len(rows)]
            record = {"patch_diff": patch_diff, "tokens_in": 7, "tokens_out": 3, "wall_secs": 2}
            Path(emit).write_text(json.dumps(record) + "\n")
            rows.append(record)
            return 0
        return calls, rows, policy, a2ctl

    def _run(self, actions, verify, patch_diffs=None, fake_git=False):
        calls, rows, policy, a2ctl = self._setup_fakes(actions, patch_diffs or [a["agent"]["prompt"] for a in actions if "agent" in a])
        markers = ["gold sentinel", "hidden::test_secret"]
        extra = []
        if fake_git:
            extra = [mock.patch.object(episode, "_new_workspace", side_effect=lambda _base, dest: self.base),
                     mock.patch.object(episode, "_git", return_value=subprocess.CompletedProcess([], 0, "root\n", "")),
                     mock.patch.object(episode, "_commit_base"),
                     mock.patch.object(episode, "_candidate_diff", side_effect=lambda _w, _root, patch: patch)]
        with mock.patch.object(episode, "run_policy", side_effect=policy), \
             mock.patch.object(episode, "_run_agent_call", side_effect=a2ctl), \
             mock.patch.object(episode.tasks, "work_dir", return_value=self.root), \
             mock.patch.object(episode.tasks, "private_dir", side_effect=lambda opaque=None: self.private / opaque if opaque else self.private), \
             mock.patch.object(episode.tasks, "load_manifest", return_value={"problem_statement": "repair it",
                                                                            "hidden_fail_to_pass": [markers[1]]}), \
             mock.patch.object(episode.tasks, "e1_home", return_value=self.root / "e1-home"), \
             mock.patch.object(episode.e1run, "private_markers", return_value=markers), \
             mock.patch.object(episode.hardened, "verify_structured", side_effect=verify), \
             mock.patch.object(episode.hardened, "grade_hardened", return_value={"resolved": True}), \
             extra[0] if fake_git else nullcontext(), \
             extra[1] if fake_git else nullcontext(), \
             extra[2] if fake_git else nullcontext(), \
             extra[3] if fake_git else nullcontext():
            return episode.run_episode(self.cfg, "opaque-secret", self.root / "genome", self.run_dir, "nonce-1"), calls, rows

    def test_fresh_fail_retry_pass_submit(self):
        first_patch = self._patch("base\n", "first\n")
        second_patch = self._patch("base\n", "passing\n")
        acts = [
            {"agent": {"base": "fresh", "prompt": first_patch, "effort": "high"}},
            {"agent": {"base": "fresh", "prompt": second_patch, "effort": "high"}},
            {"submit": {"candidate": 1}},
        ]
        row, states, a2rows = self._run(acts, lambda _opaque, diff: {"passed": "passing" in diff,
                                                                    "tests": [{"outcome": "pass" if "passing" in diff else "fail"}]})
        self.assertEqual(row["steps"], 3, row)
        self.assertEqual(row["agent_calls"], 2)
        self.assertEqual(row["grade"], {"resolved": True})
        self.assertEqual(row["submitted_candidate"], 1)
        self.assertEqual([len(s["candidates"]) for s in states], [0, 1, 2])
        self.assertEqual((row["tokens_in"], row["tokens_out"]), (14, 6))
        self.assertEqual(len(a2rows), 2)

    def test_candidate_base_composes_cumulative_diff(self):
        patch = self._patch("base\n", "passing\n")
        second_patch = self._line_patch("passing\n", "composed\n")
        actions = [
            {"agent": {"base": "fresh", "prompt": "first", "effort": "medium"}},
            {"agent": {"base": 0, "prompt": "second", "effort": "low"}},
            {"submit": {"candidate": 1}},
        ]
        row, states, _ = self._run(actions, lambda _opaque, diff: {"passed": "composed" in diff,
                                                                   "tests": [{"outcome": "pass"}]}, [patch, second_patch])
        self.assertTrue(row["grade"]["resolved"])
        self.assertEqual(states[1]["candidates"][0]["index"], 0)
        self.assertEqual(states[2]["candidates"][1]["base"], 0)

    def test_invalid_action_submits_last_and_marks_policy_error(self):
        patch = "candidate-diff"
        actions = [{"agent": {"base": "fresh", "prompt": patch, "effort": "high"}}, "garbage output"]
        row, _states, _ = self._run(actions, lambda _opaque, diff: {"passed": False, "tests": []}, fake_git=True)
        self.assertTrue(row["policy_error"])
        self.assertEqual(row["submitted_candidate"], 0)

    def test_agent_call_cap_forces_submit_last(self):
        patch = "candidate-diff"
        action = {"agent": {"base": "fresh", "prompt": patch, "effort": "high"}}
        row, _states, _ = self._run([action] * 4, lambda _opaque, diff: {"passed": False, "tests": []}, fake_git=True)
        self.assertTrue(row["policy_error"])
        self.assertEqual(row["agent_calls"], 3)
        self.assertEqual(row["steps"], 4)
        self.assertEqual(row["submitted_candidate"], 2)

    def test_state_has_no_private_identifiers_or_paths(self):
        patch = "candidate-diff"
        actions = [{"agent": {"base": "fresh", "prompt": patch, "effort": "high"}},
                   {"submit": {"candidate": 0}}]
        _row, states, _rows = self._run(actions, lambda _opaque, _diff: {"passed": False, "tests": []}, fake_git=True)
        serialized = json.dumps(states[1])
        self.assertFalse(episode.e1run._marker_in("opaque-secret", serialized))
        self.assertFalse(episode.e1run._marker_in("hidden::test_secret", serialized))
        self.assertNotIn(str(self.root / "private"), serialized)



class DenyReadTests(unittest.TestCase):
    def test_cfg_deny_read_overrides_the_default_list(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            a, b = Path(tmp) / "a", Path(tmp) / "b"; a.mkdir()
            cfg = argparse.Namespace(deny_read=[a, b])
            self.assertEqual(episode._deny_args(cfg, Path(tmp) / "sealed"), ["--deny-read", str(a)])
            self.assertEqual(episode._deny_args(argparse.Namespace(), Path(tmp)), episode.deny_read_args(Path(tmp)))

    def test_agents_are_denied_secrets_sealed_projects_and_ssh(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            sealed = Path(tmp) / "sealed"; sealed.mkdir()
            args = episode.deny_read_args(sealed)
        denied = args[1::2]
        self.assertEqual(args[0::2], ["--deny-read"] * len(denied))
        self.assertIn(str(sealed), denied)
        for path in (episode.tasks.private_dir(), Path.home() / "Projects", Path.home() / ".ssh"):
            if path.exists():
                self.assertIn(str(path), denied)


if __name__ == "__main__":
    unittest.main()
