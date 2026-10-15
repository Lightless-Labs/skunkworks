"""Unit tests for bench/e1/tasks.py. Run: python3 -m unittest bench/e1/test_tasks.py"""

import json
import os
import subprocess
import sys
import tempfile
import unittest
from argparse import Namespace
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent))

import tasks  # noqa: E402


def git(cwd: Path, *args: str) -> str:
    return subprocess.run(["git", *args], cwd=cwd, check=True, capture_output=True, text=True).stdout


class TaskSelectionTests(unittest.TestCase):
    def test_repo_filter_limits_selected_rows(self) -> None:
        rows = [
            {"repo": repo, "instance_id": repo, "fail_to_pass": '["a", "b"]',
             "pass_to_pass": "[]", "selected_test_files_to_run": "[]", "patch": "", "test_patch": "",
             "base_commit": "deadbeef", "problem_statement": "A problem"}
            for repo in ("ansible/ansible", "qutebrowser/qutebrowser")
        ]
        with tempfile.TemporaryDirectory() as tmp, mock.patch.dict(os.environ, {"A2_E1_HOME": tmp}):
            out = Path(tmp) / "selected.json"
            with mock.patch.object(tasks, "dataset_path", return_value=Path(tmp) / "fake.parquet"), \
                    mock.patch.object(tasks, "load_rows", return_value=rows):
                tasks.select(Namespace(out=str(out), repo=["qutebrowser/qutebrowser"]))
            selected = json.loads(out.read_text())
            self.assertEqual([item["instance_id"] for item in selected], ["qutebrowser/qutebrowser"])


class RequirementInstallTests(unittest.TestCase):
    def test_ansible_install_argv_is_unchanged(self) -> None:
        captured = []

        def run(argv, **kwargs):
            captured.append(argv)
            return subprocess.CompletedProcess(argv, 1 if len(captured) == 1 else 0, "", "")

        with tempfile.TemporaryDirectory() as tmp:
            repo = Path(tmp)
            for rel in ("requirements.txt", "test/units/requirements.txt"):
                path = repo / rel
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("")
            with mock.patch.object(tasks.subprocess, "run", side_effect=run):
                mode = tasks.install_requirements(Path("/venv"), repo, tasks.REPO_PROFILES["ansible/ansible"], "2022-03-04")
        expected_base = [
            "uv", "pip", "install", "-q", "--python", "/venv/bin/python",
            "-r", str(repo / "requirements.txt"), "-r", str(repo / "test/units/requirements.txt"), "pytest", "pytest-mock", "mock",
        ]
        self.assertEqual(mode, "unpinned-fallback")
        self.assertEqual(captured, [
            [*expected_base, "--exclude-newer", tasks.common.exclude_newer_for_base_date("2022-03-04")],
            expected_base,
        ])

    def test_qutebrowser_uses_highest_pyqt_and_constraints_only_on_fallback(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            repo = Path(tmp)
            (repo / "requirements.txt").write_text("")
            (repo / "misc/requirements").mkdir(parents=True)
            reqdir = repo / "misc/requirements"
            (reqdir / "requirements-tests.txt").write_text("")
            (reqdir / "requirements-pyqt5.15.txt").write_text("")
            (reqdir / "requirements-pyqt5.10.txt").write_text("")
            captured = []
            constraints_text = []

            def run(argv, **kwargs):
                captured.append((list(argv), kwargs))
                if "-c" in argv:
                    constraints_text.append(Path(argv[argv.index("-c") + 1]).read_text())
                return subprocess.CompletedProcess(argv, 1 if len(captured) == 1 else 0, "", "")

            with mock.patch.object(tasks.subprocess, "run", side_effect=run):
                mode = tasks.install_requirements(Path("/venv"), repo, tasks.REPO_PROFILES["qutebrowser/qutebrowser"], "2021-01-01")

            self.assertEqual(mode, "unpinned-fallback")
            first, second = [call[0] for call in captured]
            self.assertIn(str(reqdir / "requirements-pyqt5.15.txt"), first)
            self.assertNotIn(str(reqdir / "requirements-pyqt5.10.txt"), first)
            self.assertNotIn("-c", first)
            self.assertIn("-c", second)
            constraints = Path(second[second.index("-c") + 1])
            self.assertEqual(constraints_text, ["PyQt5<5.16\nPyQtWebEngine<5.16\n"])
            self.assertFalse(constraints.exists())


class ProfileTests(unittest.TestCase):
    def test_profile_env(self) -> None:
        self.assertEqual(tasks.profile_env({"repo": "ansible/ansible"}), {})
        self.assertEqual(tasks.profile_env({"repo": "qutebrowser/qutebrowser"}), {"QT_QPA_PLATFORM": "offscreen"})


class SubmodulePreparationTests(unittest.TestCase):
    def test_fetches_exact_submodule_commit_without_nested_git_metadata(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            subwork = root / "subwork"
            subwork.mkdir()
            git(subwork, "init", "-q")
            git(subwork, "config", "user.email", "test@example.com")
            git(subwork, "config", "user.name", "Test")
            (subwork / "info.py").write_text("VALUE = 42\n")
            git(subwork, "add", "info.py")
            git(subwork, "commit", "-qm", "submodule")
            sub_sha = git(subwork, "rev-parse", "HEAD").strip()
            subremote = root / "sub.git"
            subprocess.run(["git", "clone", "-q", "--bare", str(subwork), str(subremote)], check=True)

            basework = root / "basework"
            basework.mkdir()
            git(basework, "init", "-q")
            git(basework, "config", "user.email", "test@example.com")
            git(basework, "config", "user.name", "Test")
            (basework / ".gitmodules").write_text('[submodule "vendor/infogami"]\n\tpath = vendor/infogami\n\turl = file://' + str(subremote) + '\n')
            git(basework, "add", ".gitmodules")
            git(basework, "update-index", "--add", "--cacheinfo", "160000," + sub_sha + ",vendor/infogami")
            git(basework, "commit", "-qm", "base")
            base_sha = git(basework, "rev-parse", "HEAD").strip()
            baseremote = root / "base.git"
            subprocess.run(["git", "clone", "-q", "--bare", str(basework), str(baseremote)], check=True)

            opaque = "task123"
            home = root / "home"
            private = home / "private" / opaque
            private.mkdir(parents=True)
            (private / "manifest.json").write_text(json.dumps({"repo": "internetarchive/openlibrary", "base_commit": base_sha}))
            work = home / "work" / opaque
            repo = work / "repo"
            original_run = subprocess.run
            submodule_calls = []

            def run(argv, **kwargs):
                argv = list(argv)
                if argv[:2] == ["git", "fetch"] and "https://github.com/internetarchive/openlibrary" in argv:
                    argv[argv.index("https://github.com/internetarchive/openlibrary")] = str(baseremote)
                if argv and argv[0] == "git" and kwargs.get("cwd") == repo / "vendor/infogami":
                    submodule_calls.append(argv)
                if argv[:2] == ["uv", "venv"]:
                    venv = Path(argv[-1])
                    (venv / "bin").mkdir(parents=True)
                    (venv / "bin/python").touch()
                    return subprocess.CompletedProcess(argv, 0, "", "")
                if argv[:3] == ["uv", "pip", "install"]:
                    return subprocess.CompletedProcess(argv, 0, "", "")
                return original_run(argv, **kwargs)

            with mock.patch.dict(os.environ, {"A2_E1_HOME": str(home)}), \
                    mock.patch.object(tasks.subprocess, "run", side_effect=run), \
                    mock.patch.object(tasks.common, "python_for_base_date", return_value="python3"):
                tasks.prepare(opaque)

            self.assertTrue((repo / "vendor/infogami/info.py").is_file())
            self.assertFalse((repo / "vendor/infogami/.git").exists())
            self.assertEqual((repo / "vendor/infogami/info.py").read_text(), "VALUE = 42\n")
            fetches = [call for call in submodule_calls if call[:2] == ["git", "fetch"]]
            self.assertEqual(len(fetches), 1)
            self.assertEqual(fetches[0][-2:], ["file://" + str(subremote), sub_sha])
            self.assertEqual([call[1] for call in submodule_calls], ["init", "fetch", "checkout"])
            self.assertEqual(git(repo, "rev-list", "--all", "--count").strip(), "1")


if __name__ == "__main__":
    unittest.main()
