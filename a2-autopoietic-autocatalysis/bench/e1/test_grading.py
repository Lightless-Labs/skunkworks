"""Unit tests for bench/e1/grading.py. Run: python3 -m unittest bench/e1/test_grading.py"""

import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import grading  # noqa: E402


def git(cwd: Path, *args: str) -> str:
    return subprocess.run(["git", *args], cwd=cwd, check=True, capture_output=True, text=True).stdout


def make_base(root: Path) -> Path:
    base = root / "base"
    (base / "lib").mkdir(parents=True)
    (base / "test").mkdir()
    (base / "lib" / "mod.py").write_text("VALUE = 1\n")
    (base / "test" / "test_mod.py").write_text("def test_value():\n    assert True\n")
    grading.init_history_free_repo(base)
    return base


class HistoryFreeRepoTests(unittest.TestCase):
    def test_single_root_commit_without_remotes(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            base = make_base(Path(tmp))
            self.assertEqual(git(base, "rev-list", "--all", "--count").strip(), "1")
            self.assertEqual(git(base, "remote").strip(), "")
            self.assertEqual(git(base, "status", "--porcelain").strip(), "")


class CandidateTreeTests(unittest.TestCase):
    def test_diff_applies_and_test_edits_are_reverted(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            base = make_base(root)
            work = root / "agent"
            subprocess.run(["git", "clone", "-q", str(base), str(work)], check=True)
            (work / "lib" / "mod.py").write_text("VALUE = 2\n")
            (work / "test" / "test_mod.py").write_text("def test_value():\n    assert False\n")
            (work / "test" / "test_new.py").write_text("def test_added():\n    pass\n")
            (work / "conftest.py").write_text("# agent conftest\n")
            git(work, "add", "-A")
            diff = git(work, "diff", "--cached")

            dest = root / "cand"
            grading.build_candidate_tree(base, dest, diff=diff)

            self.assertEqual((dest / "lib" / "mod.py").read_text(), "VALUE = 2\n")
            self.assertEqual((dest / "test" / "test_mod.py").read_text(), "def test_value():\n    assert True\n")
            self.assertFalse((dest / "test" / "test_new.py").exists())
            self.assertFalse((dest / "conftest.py").exists())

    def test_source_tree_copy_reverts_test_edits_and_deleted_tests(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            base = make_base(root)
            work = root / "agent"
            subprocess.run(["git", "clone", "-q", str(base), str(work)], check=True)
            (work / "lib" / "mod.py").write_text("VALUE = 3\n")
            (work / "test" / "test_mod.py").unlink()

            dest = root / "cand"
            grading.build_candidate_tree(base, dest, source_tree=work)

            self.assertEqual((dest / "lib" / "mod.py").read_text(), "VALUE = 3\n")
            self.assertTrue((dest / "test" / "test_mod.py").exists())
            self.assertFalse((dest / ".git").exists() and (dest / ".git").is_file())

    def test_empty_diff_is_base(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            base = make_base(root)
            dest = root / "cand"
            grading.build_candidate_tree(base, dest, diff="")
            self.assertEqual((dest / "lib" / "mod.py").read_text(), "VALUE = 1\n")

    def test_unappliable_diff_raises_candidate_error(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            base = make_base(root)
            bad = "--- a/lib/mod.py\n+++ b/lib/mod.py\n@@ -1 +1 @@\n-NOPE = 0\n+NOPE = 1\n"
            with self.assertRaises(grading.CandidateError):
                grading.build_candidate_tree(base, root / "cand", diff=bad)


class ChangedPathTests(unittest.TestCase):
    def test_paths_from_diff_headers(self) -> None:
        diff = (
            "diff --git a/lib/mod.py b/lib/mod.py\n--- a/lib/mod.py\n+++ b/lib/mod.py\n"
            "diff --git a/test/old.py b/test/new.py\nsimilarity index 90%\n"
        )
        self.assertEqual(grading.paths_in_diff(diff), {"lib/mod.py", "test/old.py", "test/new.py"})

    def test_worktree_changed_paths_include_untracked_and_deleted(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            base = make_base(root)
            work = root / "agent"
            subprocess.run(["git", "clone", "-q", str(base), str(work)], check=True)
            (work / "lib" / "mod.py").write_text("VALUE = 9\n")
            (work / "test" / "test_mod.py").unlink()
            (work / "test" / "test_extra.py").write_text("x = 1\n")
            self.assertEqual(
                grading.worktree_changed_paths(work),
                {"lib/mod.py", "test/test_mod.py", "test/test_extra.py"},
            )


class PytestRunTests(unittest.TestCase):
    def test_passed_node_ids_from_junit(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            tree = root / "tree"
            (tree / "test").mkdir(parents=True)
            (tree / "test" / "test_a.py").write_text(
                "import pytest\n"
                "class TestK:\n    def test_ok(self):\n        assert True\n"
                "def test_bad():\n    assert False\n"
                "@pytest.mark.parametrize('x', [1, 2])\ndef test_p(x):\n    assert x == 1\n"
            )
            try:
                import pytest  # noqa: F401
            except ImportError:
                self.skipTest("pytest not installed for this interpreter")
            result = grading.run_pytest(tree, Path(sys.executable).parent.parent, ["test/test_a.py"], pythonpath=[])
            self.assertEqual(
                result.passed,
                {"test/test_a.py::TestK::test_ok", "test/test_a.py::test_p[1]"},
            )
            self.assertIn("test/test_a.py::test_bad", result.failed)


if __name__ == "__main__":
    unittest.main()
