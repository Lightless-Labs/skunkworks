"""Unit tests for bench/e1/run.py pure logic. Run: python3 -m unittest bench/e1/test_run.py"""

import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import run  # noqa: E402


class ArmOrderTests(unittest.TestCase):
    def test_order_is_seeded_permutation_per_task(self) -> None:
        sample = [f"e1-{i:012x}" for i in range(20)]
        first, second = run.arm_order(sample, 7), run.arm_order(sample, 7)
        self.assertEqual(first, second)
        for arms in first.values():
            self.assertEqual(sorted(arms), sorted(run.ARMS))
        self.assertGreater(len({tuple(v) for v in first.values()}), 1)
        self.assertNotEqual(first, run.arm_order(sample, 8))


class ShardTests(unittest.TestCase):
    def test_shard_spec(self) -> None:
        self.assertEqual(run.shard_spec("0/1"), (0, 1))
        self.assertEqual(run.shard_spec("2/3"), (2, 3))
        for bad in ("3/3", "-1/2", "0/0"):
            with self.assertRaises(ValueError):
                run.shard_spec(bad)

    def test_arm_order_does_not_depend_on_shard(self) -> None:
        sample = [f"e1-{i:012x}" for i in range(6)]
        full = run.arm_order(sample, 3)
        self.assertEqual(full[sample[4]], run.arm_order(sample, 3)[sample[4]])


class CodexHomeTests(unittest.TestCase):
    def test_codex_home_has_template_config_and_symlinked_auth(self) -> None:
        import tempfile

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            auth = root / "user-codex" / "auth.json"
            auth.parent.mkdir()
            auth.write_text("{}")
            home = run.ensure_codex_home(root / "home", auth)
            self.assertTrue((home / "auth.json").is_symlink())
            self.assertEqual((home / "auth.json").resolve(), auth.resolve())
            self.assertEqual((home / "config.toml").read_text(), run.CODEX_CONFIG_TEMPLATE.read_text())
            self.assertIn("apps = false", (home / "config.toml").read_text())
            (home / "config.toml").write_text("[mcp_servers.x]\n")
            run.ensure_codex_home(root / "home", auth)
            self.assertEqual((home / "config.toml").read_text(), run.CODEX_CONFIG_TEMPLATE.read_text())

    def test_codex_home_rejects_extra_ambient_files(self) -> None:
        import tempfile

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            auth = root / "auth.json"
            auth.write_text("{}")
            home = run.ensure_codex_home(root / "home", auth)
            (home / "skills" / ".system").mkdir(parents=True)
            run.ensure_codex_home(root / "home", auth)
            for ambient in (home / "skills" / "user-skill", home / "plugins", home / "AGENTS.md"):
                ambient.mkdir() if ambient.suffix == "" else ambient.write_text("ambient")
                with self.assertRaises(RuntimeError):
                    run.ensure_codex_home(root / "home", auth)
                ambient.rmdir() if ambient.is_dir() else ambient.unlink()


class FreshWorkspaceTests(unittest.TestCase):
    def test_fresh_workspace_is_independent_copy_with_git(self) -> None:
        import os
        import subprocess
        import tempfile

        import tasks

        with tempfile.TemporaryDirectory() as tmp:
            os.environ["A2_E1_HOME"] = tmp
            try:
                base = tasks.work_dir("e1-000000000000") / "repo"
                base.mkdir(parents=True)
                (base / "f.txt").write_text("base\n")
                subprocess.run(["git", "init", "-q"], cwd=base, check=True)
                dest = run.fresh_workspace("e1-000000000000", "raw")
                self.assertTrue((dest / ".git").is_dir())
                (dest / "f.txt").write_text("changed\n")
                self.assertEqual((base / "f.txt").read_text(), "base\n")
                again = run.fresh_workspace("e1-000000000000", "raw")
                self.assertEqual((again / "f.txt").read_text(), "base\n")
            finally:
                del os.environ["A2_E1_HOME"]


class LineageTests(unittest.TestCase):
    def test_fresh_lineage_removes_prior_db_and_sidecars(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            run_dir = Path(tmp)
            for suffix in ("", "-wal", "-shm"):
                (run_dir / f"e1-x.a2-full.lineage.sqlite{suffix}").write_text("stale")
            path = run.fresh_lineage(run_dir, "e1-x", "a2-full")
            self.assertEqual(path, str(run_dir / "e1-x.a2-full.lineage.sqlite"))
            self.assertEqual(list(run_dir.iterdir()), [])
        self.assertEqual(run.fresh_lineage(Path("/nonexistent"), "e1-x", "raw"), "none")


class InfraClassificationTests(unittest.TestCase):
    def test_classification(self) -> None:
        ok = {"tokens_in": 10, "tokens_out": 5}
        dead = {"tokens_in": 0, "tokens_out": 0}
        self.assertIsNone(run.classify_infra([ok], [0]))
        self.assertEqual(run.classify_infra([], [0]), "no_attempt_row")
        self.assertEqual(run.classify_infra([], [1]), "a2ctl_failed")
        self.assertEqual(run.classify_infra([ok, dead], [0, 0]), "provider_error")
        self.assertEqual(run.classify_infra([dead], [0]), "provider_error")


class InfraRetryTests(unittest.TestCase):
    def test_retries_infra_failure_once_and_records_it(self) -> None:
        calls = []

        def runner(*args):
            calls.append(args)
            return {"infra_failure": "provider_error" if len(calls) == 1 else None, "n": len(calls)}

        row = run.run_arm_with_infra_retry(None, "e1-x", "raw", None, runner=runner)
        self.assertEqual(len(calls), 2)
        self.assertIsNone(row["infra_failure"])
        self.assertEqual(row["infra_retry_of"], "provider_error")

    def test_no_retry_without_infra_failure_and_at_most_one_retry(self) -> None:
        calls = []

        def ok(*args):
            calls.append(args)
            return {"infra_failure": None}

        self.assertNotIn("infra_retry_of", run.run_arm_with_infra_retry(None, "e1-x", "raw", None, runner=ok))
        self.assertEqual(len(calls), 1)

        def always_bad(*args):
            calls.append(args)
            return {"infra_failure": "a2ctl_failed"}

        calls.clear()
        row = run.run_arm_with_infra_retry(None, "e1-x", "raw", None, runner=always_bad)
        self.assertEqual(len(calls), 2)
        self.assertEqual(row["infra_failure"], "a2ctl_failed")


class LeakTests(unittest.TestCase):
    def test_private_marker_in_payload_raises(self) -> None:
        with self.assertRaises(RuntimeError):
            run.assert_no_private_leak("see /Users/x/.a2-e1/private/e1-1", ["/Users/x/.a2-e1/private"])
        run.assert_no_private_leak("clean text", ["/Users/x/.a2-e1/private", ""])

    def test_markers_are_secret_content_not_the_private_path(self) -> None:
        markers = run.private_markers("--- a/lib/x.py\n+++ b/lib/x.py\n+fix\n", ["test/t.py::test_hidden"])
        traceback = "/Users/x/.a2-e1/private/e1-1/venv/lib/python3.9/site-packages/_pytest/main.py:1: in run"
        run.assert_no_private_leak(traceback, markers)
        with self.assertRaises(RuntimeError):
            run.assert_no_private_leak("FAILED test/t.py::test_hidden", markers)
        with self.assertRaises(RuntimeError):
            run.assert_no_private_leak("--- a/lib/x.py\n+++ b/lib/x.py\n+fix\n", markers)

    def test_hidden_id_matches_whole_node_ids_only(self) -> None:
        markers = run.private_markers("gold", ["t.py::K::test_six", "t.py::test_p"])
        run.assert_no_private_leak("- t.py::K::test_six_from_many_submodules failed", markers)
        run.assert_no_private_leak("- t.py::test_p[1] failed", markers)
        for text in ("- t.py::K::test_six failed", "t.py::K::test_six", "(t.py::test_p)"):
            with self.assertRaises(RuntimeError):
                run.assert_no_private_leak(text, markers)

    def test_payload_names_only_opaque_id_and_committed_verifier(self) -> None:
        payload = run.task_payload("e1-abc123abc123", "Fix it.", "e1-abc123abc123:raw:r1", ["https://api.z.ai"])
        command = payload["verification_commands"][0]["command"]
        self.assertTrue(command.startswith("python3 "))
        self.assertTrue(command.endswith("bench/e1/verify.py --task e1-abc123abc123"))
        self.assertNotIn(".a2-e1", json.dumps(payload))
        self.assertEqual(payload["network_policy"], {"AllowList": ["https://api.z.ai"]})
        self.assertTrue(payload["no_external_solution_search"])


class VerifierRowTests(unittest.TestCase):
    def test_verifier_passed_requires_non_empty_all_passed(self) -> None:
        self.assertFalse(run.verifier_passed({"worktree_verifications": []}))
        self.assertFalse(run.verifier_passed({"worktree_verifications": [{"passed": True}, {"passed": False}]}))
        self.assertTrue(run.verifier_passed({"worktree_verifications": [{"passed": True}]}))

    def test_verifier_output_is_bounded_tail(self) -> None:
        row = {"worktree_verifications": [{"stdout_tail": "a" * 20_000, "stderr_tail": "END"}]}
        out = run.verifier_output(row)
        self.assertTrue(out.endswith("END"))
        self.assertLessEqual(len(out), run.FEEDBACK_TAIL_CHARS)


if __name__ == "__main__":
    unittest.main()
