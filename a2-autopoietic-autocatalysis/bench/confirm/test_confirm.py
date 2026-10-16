from __future__ import annotations

import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from bench.confirm import confirm, verify_cmd


class VerifyCommandTests(unittest.TestCase):
    def test_renders_nonpassing_and_rejected_results_without_private_path(self):
        result = {
            "passed": False,
            "tests": [
                {"name": "test_ok", "outcome": "pass", "message": ""},
                {"name": "test_bad", "outcome": "fail", "message": "assertion failed"},
            ],
            "rejected": ["forbidden module"],
        }
        output = verify_cmd.render(result)
        self.assertIn("[fail] test_bad", output)
        self.assertIn("assertion failed", output)
        self.assertIn("rejected: forbidden module", output)
        self.assertIn("visible verification: 1/2 passed", output)
        self.assertNotIn("/private/", output)
        self.assertNotIn("test_ok", output)

    def test_exit_code_tracks_passed(self):
        with mock.patch.object(verify_cmd, "candidate_diff", return_value="diff"):
            with mock.patch.object(verify_cmd.hardened, "verify_structured", return_value={
                "passed": True, "tests": [{"outcome": "pass"}], "rejected": []
            }):
                self.assertEqual(verify_cmd.main(["--task", "opaque"]), 0)
            with mock.patch.object(verify_cmd.hardened, "verify_structured", return_value={
                "passed": False, "tests": [], "rejected": ["no"]
            }):
                self.assertEqual(verify_cmd.main(["--task", "opaque"]), 1)

    def test_diff_includes_tracked_and_untracked_without_mutating_index(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            def git_result(argv, **kwargs):
                if argv[1:3] == ["rev-list", "--max-parents=0"]:
                    return subprocess.CompletedProcess(argv, 0, "root-commit\n", "")
                if argv[1:2] == ["diff"]:
                    return subprocess.CompletedProcess(
                        argv, 0, "diff --git a/tracked.txt b/tracked.txt\n"
                        "diff --git a/new.txt b/new.txt\n", ""
                    )
                return subprocess.CompletedProcess(argv, 0, "", "")

            with mock.patch.object(verify_cmd.subprocess, "run", side_effect=git_result) as run:
                diff = verify_cmd.candidate_diff(root)
            self.assertIn("tracked.txt", diff)
            self.assertIn("new.txt", diff)
            self.assertEqual([call.args[0][1] for call in run.call_args_list],
                             ["rev-list", "read-tree", "add", "diff"])
            self.assertTrue(all("GIT_INDEX_FILE" in call.kwargs.get("env", {})
                                for call in run.call_args_list[1:]))


class ConfirmHelpersTests(unittest.TestCase):
    def test_sealed_home_assertion(self):
        with mock.patch.dict(os.environ, {"A2_E1_HOME": "/tmp/not-sealed"}):
            with self.assertRaises(AssertionError):
                confirm.assert_sealed_home()
        with mock.patch.dict(os.environ, {"A2_E1_HOME": "/tmp/.a2-e1/sealed"}):
            self.assertEqual(confirm.assert_sealed_home(), Path("/tmp/.a2-e1/sealed"))

    def test_deny_paths_include_all_existing_targets(self):
        with tempfile.TemporaryDirectory() as tmp:
            home = Path(tmp)
            e1 = home / ".a2-e1"
            sealed = e1 / "sealed"
            dev_private = e1 / "private"
            projects = home / "Projects"
            ssh = home / ".ssh"
            for path in (sealed, sealed / "private", dev_private, projects, ssh):
                path.mkdir(parents=True, exist_ok=True)
            with mock.patch.object(confirm.Path, "home", return_value=home), mock.patch.dict(
                os.environ, {"A2_E1_HOME": str(sealed)}
            ):
                paths = confirm.deny_read_paths()
            self.assertEqual(set(paths), {sealed / "private", dev_private, projects, ssh})

    def test_arm_order_is_seeded_per_task_and_matches_rng_shuffle(self):
        tasks = ["a", "b", "c"]
        actual = confirm.arm_order(tasks)
        import random
        rng = random.Random(1201)
        expected = {}
        for task in tasks:
            arms = ["champion", "a2-full", "g0"]
            rng.shuffle(arms)
            expected[task] = arms
        self.assertEqual(actual, expected)
        self.assertEqual(actual, confirm.arm_order(tasks))

    def test_a2_full_argv_and_visible_verifier_command(self):
        cfg = confirm.Config(
            a2ctl="/repo/target/debug/a2ctl", provider="codex/gpt-6-luna", max_tokens=2000000,
            timeout=1500, endpoint=["https://chatgpt.com", "https://auth.openai.com"], proxy_port=1234,
            codex_home=Path("/confirm/home"), reasoning_effort="high", instruction="issue-only",
            run_id="run", deny_read=[Path("/sealed/private"), Path("/dev/private")],
        )
        with tempfile.TemporaryDirectory() as tmp:
            workspace = Path(tmp)
            payload = {"task_id": "opaque", "problem_statement": "issue", "verification_commands": []}
            with mock.patch.object(confirm.subprocess, "run", return_value=subprocess.CompletedProcess([], 0)) as run:
                confirm.run_a2ctl(cfg, workspace, payload, "a2-full", Path(tmp) / "emit", Path(tmp) / "log", "lineage.sqlite")
            argv = run.call_args.args[0]
            for pair in (("--attempts", "3"), ("--lineage-db", "lineage.sqlite"),
                         ("--verifier-feedback", "raw"), ("--prompt-profile", "external"),
                         ("--timeout", "1500"), ("--max-tokens", "2000000"),
                         ("--provider", "codex/gpt-6-luna")):
                i = argv.index(pair[0])
                self.assertEqual(argv[i + 1], pair[1])
            self.assertEqual(argv.count("--deny-read"), 2)
            command = json.loads(run.call_args.kwargs["input"])["verification_commands"][0]["command"]
            self.assertIn("bench/confirm/verify_cmd.py --task opaque", command)
            self.assertEqual(run.call_args.kwargs["env"]["A2_CODEX_REASONING_EFFORT"], "high")

    def test_resume_skips_recorded_pair_and_infra_retries_once(self):
        with tempfile.TemporaryDirectory() as tmp:
            run_dir = Path(tmp)
            rows = [{"opaque_id": "a", "arm": "champion", "grade": {"resolved": True}}]
            (run_dir / "rows.shard0of1.jsonl").write_text(json.dumps(rows[0]) + "\n")
            self.assertEqual(confirm.read_done(run_dir), {("a", "champion")})
            calls = []
            def runner(*args):
                calls.append(args)
                return {"infra_failure": "provider_error"} if len(calls) == 1 else {"infra_failure": None}
            row = confirm.run_with_infra_retry(runner, None, "a", "g0", run_dir)
            self.assertEqual(len(calls), 2)
            self.assertEqual(row["infra_retry_of"], "provider_error")

    def test_refuses_champion_equal_to_g0(self):
        with self.assertRaises(SystemExit):
            confirm.validate_genomes(Path("/same"), Path("/same"))

    def test_refuses_distinct_paths_with_identical_genome_content(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            left, right = root / "left", root / "right"
            left.mkdir()
            right.mkdir()
            (left / "policy.py").write_text("# same\n")
            (right / "policy.py").write_text("# same\n")
            with self.assertRaises(SystemExit):
                confirm.validate_genomes(left, right)

    def test_decide_math_drops_task_with_primary_infrastructure_failure(self):
        rows = [
            {"opaque_id": "a", "arm": "champion", "grade": {"resolved": True}},
            {"opaque_id": "a", "arm": "a2-full", "grade": {"resolved": False}},
            {"opaque_id": "b", "arm": "champion", "grade": {"resolved": False}},
            {"opaque_id": "b", "arm": "a2-full", "grade": {"resolved": True}},
            {"opaque_id": "c", "arm": "champion", "grade": {"resolved": True}, "infra_failure": None},
            {"opaque_id": "c", "arm": "a2-full", "grade": None, "infra_failure": "grader_error"},
        ]
        result = confirm.analyse(rows, ["a", "b", "c"], ["champion", "a2-full"], sample_size=3)
        self.assertEqual(result["primary"]["n_paired"], 2)
        self.assertEqual(result["primary"]["b"], 1)
        self.assertEqual(result["primary"]["c"], 1)
        self.assertAlmostEqual(result["primary"]["one_sided_p"], 0.75)
        self.assertEqual(result["infra_fraction"], 1 / 3)
        self.assertFalse(result["promote"])

    def test_decide_promote_threshold_and_power_table(self):
        rows = []
        for i in range(20):
            rows.extend([
                {"opaque_id": str(i), "arm": "champion", "grade": {"resolved": i < 10}},
                {"opaque_id": str(i), "arm": "a2-full", "grade": {"resolved": False}},
                {"opaque_id": str(i), "arm": "g0", "grade": {"resolved": False}},
            ])
        result = confirm.analyse(rows, [str(i) for i in range(20)], ["champion", "a2-full", "g0"], sample_size=20)
        self.assertTrue(result["promote"])
        self.assertEqual(result["infra_fraction"], 0.0)
        self.assertIn("n=80", json.dumps(result["power_table"]))


if __name__ == "__main__":
    unittest.main()
