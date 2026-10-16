import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from bench.evolve import researcher


class ResearcherTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.genome = self.root / "champion"
        (self.genome / "prompts").mkdir(parents=True)
        (self.genome / "policy.py").write_text("print('policy')\n")
        (self.genome / "prompts" / "base.txt").write_text("base prompt\n")
        (self.genome / "HYPOTHESIS.txt").write_text("do not copy\n")

    def tearDown(self):
        self.temp.cleanup()

    def test_make_digests_omits_private_identifiers_and_paths(self):
        rows = [{"opaque_id": "secret-id", "instance_id": "private-instance", "resolved": True,
                 "agent_calls": 2, "verify_summaries": [{"passed": False, "failing": ["test_a"]}],
                 "diff_stat": ["src/private.py"], "gold": "secret", "path": "/private"}]
        manifests = {"secret-id": {"problem_statement": "Implement a generic feature."}}
        result = researcher.make_digests(rows, manifests, "night")
        self.assertEqual(len(result), 1)
        self.assertEqual(result[0]["key"], researcher.hashlib.sha256(b"nightsecret-id").hexdigest()[:10])
        self.assertEqual(result[0]["problem_statement"], "Implement a generic feature.")
        self.assertTrue(result[0]["resolved"])
        self.assertEqual(result[0]["verify_summaries"], [{"passed": False, "failing": ["test_a"]}])
        encoded = json.dumps(result)
        for secret in ("secret-id", "private-instance", "gold", "/private", "src/private.py"):
            self.assertNotIn(secret, encoded)

    def test_build_workspace_layout_and_log_projection(self):
        dest = self.root / "workspace"
        rows = [{"exp_id": "e1", "parent": "g0", "hypothesis": "h", "decision": "discard",
                 "reason": "r", "delta_A": 1, "delta_B": 2, "p_B": .01,
                 "full44_delta": 3, "tokens": 4, "secret": "omit"}]
        with patch.object(researcher.grading, "init_history_free_repo") as init_repo:
            result = researcher.build_workspace("e1", self.genome, rows,
                                                [{"key": "0123456789", "problem_statement": "p"}], dest)
        self.assertEqual(result, dest)
        init_repo.assert_called_once_with(dest)
        self.assertTrue((dest / "genome" / "policy.py").is_file())
        self.assertFalse((dest / "genome" / "HYPOTHESIS.txt").exists())
        self.assertTrue((dest / "program.md").is_file())
        self.assertIn('"tokens": 4', (dest / "log.jsonl").read_text())
        self.assertNotIn("secret", (dest / "log.jsonl").read_text())
        self.assertTrue((dest / "digests" / "0123456789.json").is_file())
        self.assertIn("program.md", (dest / "README.md").read_text())

    def test_ngram_violations_finds_shared_words(self):
        genome = self.root / "ngram"
        genome.mkdir()
        (genome / "policy.py").write_text("alpha beta gamma delta epsilon zeta eta theta")
        self.assertEqual(researcher.ngram_violations(genome, ["ALPHA beta gamma delta epsilon zeta eta theta"], 8),
                         ["alpha beta gamma delta epsilon zeta eta theta"])
        self.assertEqual(researcher.ngram_violations(genome, ["unrelated content only"], 8), [])

    def test_apply_candidate_rejects_non_genome_hunk(self):
        patch_text = "diff --git a/genome/policy.py b/genome/policy.py\n--- a/genome/policy.py\n+++ b/genome/policy.py\n@@ -1 +1 @@\n-old\n+new\ndiff --git a/README.md b/README.md\n--- a/README.md\n+++ b/README.md\n@@ -1 +1 @@\n-old\n+new\n"
        result = researcher.apply_candidate(self.genome, patch_text, self.root / "out")
        self.assertFalse(result["ok"])
        self.assertIn("README.md", result["reason"])

    def test_apply_candidate_extracts_hypothesis_and_applies_genome_hunks(self):
        patch_text = ("diff --git a/genome/policy.py b/genome/policy.py\n"
                      "--- a/genome/policy.py\n+++ b/genome/policy.py\n@@ -1 +1 @@\n"
                      "-print('policy')\n+print('changed')\n"
                      "diff --git a/genome/HYPOTHESIS.txt b/genome/HYPOTHESIS.txt\n"
                      "new file mode 100644\n--- /dev/null\n+++ b/genome/HYPOTHESIS.txt\n"
                      "@@ -0,0 +1 @@\n+Use review and repair.\n")
        dest = self.root / "candidate"
        result = researcher.apply_candidate(self.genome, patch_text, dest)
        self.assertTrue(result["ok"], result)
        self.assertEqual(result["hypothesis"], "Use review and repair.")
        self.assertIn("changed", (dest / "policy.py").read_text())
        self.assertFalse((dest / "HYPOTHESIS.txt").exists())

    def test_validate_candidate_unchanged_bad_action_and_good_action(self):
        candidate = self.root / "same"
        import shutil
        shutil.copytree(self.genome, candidate, ignore=lambda _p, names: [n for n in names if n == "HYPOTHESIS.txt"])
        with patch.object(researcher.episode, "run_policy", return_value=('{}', '', 0)):
            reasons = researcher.validate_candidate(candidate, self.genome, [], [{"state": {}, "snapshot": self.root}])
        self.assertTrue(any("unchanged" in reason for reason in reasons))

        changed = self.root / "changed"
        shutil.copytree(self.genome, changed)
        (changed / "policy.py").write_text("different\n")
        with patch.object(researcher.episode, "run_policy", return_value=('not-json', '', 0)):
            reasons = researcher.validate_candidate(changed, self.genome, [], [{"state": {}, "snapshot": self.root}])
        self.assertTrue(any("action" in reason.lower() for reason in reasons))
        with patch.object(researcher.episode, "run_policy", return_value=('{"submit":{"candidate":0}}', '', 0)):
            reasons = researcher.validate_candidate(changed, self.genome, [], [{"state": {}, "snapshot": self.root}])
        self.assertEqual(reasons, [])



class SummaryTests(unittest.TestCase):
    def test_failure_modes_are_counted_from_digest_fields(self):
        digests = [
            {"key": "a" * 10, "resolved": True, "verify_summaries": [{"passed": True, "failing": []}]},
            {"key": "b" * 10, "resolved": False, "verify_summaries": [{"passed": True, "failing": []}]},
            {"key": "c" * 10, "resolved": False, "verify_summaries": [{"passed": False, "failing": ["t1", "t2"]}]},
        ]
        text = researcher.summarize_digests(digests)
        self.assertIn("- resolved: 1 (33%)", text)
        self.assertIn("- visible-pass, hidden-fail: 1 (33%)", text)
        self.assertIn("- never visible-pass: 1 (33%)", text)
        self.assertIn("median of 2 visible tests", text)


if __name__ == "__main__":
    unittest.main()
