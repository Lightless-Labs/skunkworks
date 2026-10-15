from __future__ import annotations

import os
import shutil
import subprocess
import sys
import tempfile
import unittest

from bench.evolve.test_seatbelt import skip_without_sandbox_exec
from pathlib import Path
from unittest import mock

from bench.evolve import hardened

# pytest stalls when its rootdir sits in macOS's per-user /var/folders temp, even unsandboxed;
# production trees live under ~/.a2-e1/private/tmp, so the fixtures use a sibling root.
TEST_TMP = Path.home() / ".a2-e1" / "test-tmp"
TEST_TMP.mkdir(parents=True, exist_ok=True)


class AuditTests(unittest.TestCase):
    def test_shadow_modules_configs_and_plugin_are_rejected(self) -> None:
        roots = [".", "lib"]
        protected = {"pytest", "sitecustomize", "evil_plugin"}
        cases = [
            "diff --git a/pytest.py b/pytest.py\nnew file mode 100644\n",
            "diff --git a/lib/sitecustomize.py b/lib/sitecustomize.py\nnew file mode 100644\n",
            "diff --git a/setup.cfg b/setup.cfg\n--- a/setup.cfg\n+++ b/setup.cfg\n+[tool:pytest]\n+addopts = -p evil_plugin\n"
            "diff --git a/lib/evil_plugin.py b/lib/evil_plugin.py\nnew file mode 100644\n",
        ]
        for diff in cases:
            with self.subTest(diff=diff[:40]):
                self.assertTrue(hardened.audit_diff(diff, roots, protected))

    def test_legitimate_lib_edit_has_no_flags(self) -> None:
        diff = "diff --git a/lib/fix.py b/lib/fix.py\n--- a/lib/fix.py\n+++ b/lib/fix.py\n+VALUE = 1\n"
        self.assertEqual(hardened.audit_diff(diff, ["." , "lib"], {"pytest"}), [])

    def test_content_audit_skips_tests_but_flags_production_code(self) -> None:
        diff = ("diff --git a/pkg/fix.py b/pkg/fix.py\n+++ b/pkg/fix.py\n+import pytest\n"
                "diff --git a/tests/test_fix.py b/tests/test_fix.py\n+++ b/tests/test_fix.py\n+import pytest\n")
        reasons = hardened.audit_diff(diff, ["."], {"pytest"})
        self.assertTrue(any("pytest" in reason for reason in reasons))
        self.assertEqual(len(reasons), 1)

    def test_junit_duplicate_or_missing_required_id_is_not_passed(self) -> None:
        node = "tests/test_x.py::test_x"
        result = hardened.enforce_required_ids({node: ["pass", "pass"]}, [node])
        self.assertEqual(result["passed"], set())
        self.assertIn(node, result["duplicates"])
        missing = hardened.enforce_required_ids({}, [node])
        self.assertEqual(missing["passed"], set())
        self.assertIn(node, missing["missing"])


class StripTests(unittest.TestCase):
    def test_strips_hidden_body_and_keeps_shared_and_p2p(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tree = Path(tmp)
            source = tree / "tests" / "test_demo.py"
            source.parent.mkdir()
            source.write_text(
                "import pytest\n\n"
                "def test_hidden():\n    SECRET_BODY_SENTINEL = 'hidden'\n\n"
                "@pytest.mark.parametrize('v', [1, 2])\n"
                "def test_shared(v):\n    assert v\n\n"
                "def test_p2p():\n    assert True\n"
            )
            result = hardened.strip_hidden_tests(tree, ["tests/test_demo.py::test_shared[1]"],
                                                ["tests/test_demo.py::test_hidden", "tests/test_demo.py::test_shared[2]"],
                                                ["tests/test_demo.py::test_p2p"])
            body = source.read_text()
        self.assertNotIn("SECRET_BODY_SENTINEL", body)
        self.assertIn("def test_shared", body)
        self.assertIn("def test_p2p", body)
        self.assertIn("tests/test_demo.py::test_shared[2]", result["inseparable"])
        self.assertIn("tests/test_demo.py::test_hidden", result["stripped"])

    def test_verify_json_never_contains_hidden_ids_or_private_paths(self) -> None:
        manifest = {"repo": "synthetic/repo", "visible_fail_to_pass": ["tests/test_x.py::test_visible"],
                    "hidden_fail_to_pass": ["tests/test_x.py::test_SECRET_hidden"],
                    "pass_to_pass": ["tests/test_x.py::test_p2p"]}
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp); private_home = root / ".a2-e1"; (root / "base").mkdir()
            (root / "private").mkdir(); (root / "scratch").mkdir()
            (root / "private" / "test.patch").write_text("")
            fake_tree = root / "tree"

            def build(_base, tree, diff=None):
                Path(tree).mkdir(parents=True)
                (Path(tree) / "tests").mkdir()
                (Path(tree) / "tests" / "test_x.py").write_text("def test_visible(): pass\n")
                return []

            fake_result = hardened.HardenedResult(1, passed={"tests/test_x.py::test_p2p"},
                failed={"tests/test_x.py::test_visible"},
                messages={"tests/test_x.py::test_visible": str(private_home / "private/opaque/gold.patch")})
            with mock.patch.object(hardened.tasks, "load_manifest", return_value=manifest), \
                 mock.patch.dict(hardened.tasks.REPO_PROFILES, {"synthetic/repo": {"pythonpath": []}}), \
                 mock.patch.object(hardened.tasks, "work_dir", return_value=root / "work"), \
                 mock.patch.object(hardened.tasks, "private_dir", return_value=root / "private"), \
                 mock.patch.object(hardened.tasks, "private_tmp", return_value=root / "scratch"), \
                 mock.patch.object(hardened.tasks, "e1_home", return_value=private_home), \
                 mock.patch.object(hardened.grading, "build_candidate_tree", side_effect=build), \
                 mock.patch.object(hardened.grading, "apply_patch"), \
                 mock.patch.object(hardened, "audit_diff", return_value=[]), \
                 mock.patch.object(hardened, "strip_hidden_tests", return_value={"stripped": [], "inseparable": []}), \
                 mock.patch.object(hardened, "frozen_pytest_config", return_value=root / "frozen.ini"), \
                 mock.patch.object(hardened, "run_pytest_hardened", return_value=fake_result):
                output = hardened.verify_structured("opaque", "")
        rendered = repr(output)
        self.assertNotIn("test_SECRET_hidden", rendered)
        self.assertNotIn(str(private_home), rendered)
        self.assertIn("<private>", rendered)


class VenvTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.temp = tempfile.TemporaryDirectory(prefix="evolve-venv-", dir=TEST_TMP)
        cls.venv = Path(cls.temp.name) / "venv"
        env = {**os.environ, "UV_CACHE_DIR": str(Path(cls.temp.name) / "uv-cache")}
        try:
            subprocess.run(["uv", "venv", "--python", sys.executable, str(cls.venv)], check=True,
                           capture_output=True, text=True, timeout=120, env=env)
            subprocess.run(["uv", "pip", "install", "--python", str(cls.venv / "bin" / "python"),
                            "pytest", "pytest-mock"], check=True, capture_output=True, text=True, timeout=180, env=env)
        except (OSError, subprocess.SubprocessError) as exc:
            detail = getattr(exc, "stderr", None)
            if isinstance(detail, bytes): detail = detail.decode(errors="replace")
            cls.skip_reason = "uv could not create/install the pytest venv: " + (str(detail).strip() or str(exc))
            cls.venv = None

    @classmethod
    def tearDownClass(cls) -> None:
        cls.temp.cleanup()

    def setUp(self) -> None:
        if self.venv is None:
            self.skipTest(self.skip_reason)
        skip_without_sandbox_exec(self)

    def test_protected_module_names_include_installed_distribution(self) -> None:
        names = hardened.protected_module_names(self.venv)
        self.assertTrue({"pytest", "_pytest", "pluggy", "pytest_mock"} <= names)
        self.assertIn("pytest_mock", hardened.venv_pytest_plugins(self.venv))

    def test_frozen_config_uses_pristine_config(self) -> None:
        with tempfile.TemporaryDirectory(dir=TEST_TMP) as tmp:
            base, out = Path(tmp) / "base", Path(tmp) / "out"
            base.mkdir(); out.mkdir()
            (base / "pytest.ini").write_text("[pytest]\naddopts = -q\n")
            frozen = hardened.frozen_pytest_config(base, out)
            self.assertEqual(frozen.read_text(), "[pytest]\naddopts = -q\n")

    def test_frozen_config_stops_candidate_plugin_and_reports_failure(self) -> None:
        with tempfile.TemporaryDirectory(dir=TEST_TMP) as tmp:
            base, tree, out = Path(tmp) / "base", Path(tmp) / "tree", Path(tmp) / "out"
            for path in (base, tree, out): path.mkdir()
            (base / "setup.cfg").write_text("[tool:pytest]\n")
            (tree / "setup.cfg").write_text("[tool:pytest]\naddopts = -p evil_plugin\n")
            (tree / "lib").mkdir()
            (tree / "lib" / "evil_plugin.py").write_text(
                "import pytest\n"
                "@pytest.hookimpl(hookwrapper=True, tryfirst=True)\n"
                "def pytest_runtest_makereport(item, call):\n"
                "    outcome = yield\n"
                "    report = outcome.get_result()\n"
                "    if call.when == 'call' and report.failed:\n"
                "        report.outcome = 'passed'; report.longrepr = None\n"
            )
            (tree / "tests").mkdir()
            (tree / "tests" / "test_bad.py").write_text("def test_bad():\n    assert False\n")
            frozen = hardened.frozen_pytest_config(base, out)
            result = hardened.run_pytest_hardened(tree, self.venv, ["tests/test_bad.py::test_bad"],
                                                   ["lib"], frozen, [], 30, Path(tmp) / "home")
            self.assertNotIn("tests/test_bad.py::test_bad", result.passed)
            self.assertIn("tests/test_bad.py::test_bad", result.failed, result.output[-2000:])

    def _runner_tree(self, tmp: Path) -> tuple[Path, Path, Path]:
        root = tmp; tree = root / "tree"; home = root / "fake-home"
        private = home / ".a2-e1" / "private" / "e1-x"
        (tree / "tests").mkdir(parents=True); private.mkdir(parents=True)
        (private / "gold.patch").write_text("SECRET-GOLD")
        (private / "manifest.json").write_text('{"hidden": "SECRET-ID"}')
        (root / "out").mkdir()
        return tree, home, private

    def test_code_under_test_cannot_read_write_or_connect(self) -> None:
        with tempfile.TemporaryDirectory(dir=TEST_TMP) as tmp:
            tree, home, private = self._runner_tree(Path(tmp))
            (tree / "tests" / "test_probe.py").write_text(
                "import socket\nfrom pathlib import Path\nimport pytest\nP = Path(%r)\n"
                "def test_read_gold():\n    with pytest.raises(OSError):\n        P.joinpath('gold.patch').read_text()\n"
                "def test_read_manifest():\n    with pytest.raises(OSError):\n        P.joinpath('manifest.json').read_text()\n"
                "def test_write_private():\n    with pytest.raises(OSError):\n        P.joinpath('w').write_text('x')\n"
                "def test_network():\n    with pytest.raises(OSError):\n        socket.create_connection(('1.1.1.1', 443), 2)\n"
                "def test_read_outside_home_still_works():\n    assert Path('/etc/hosts').read_text()\n" % str(private))
            ids = [f"tests/test_probe.py::{n}" for n in ("test_read_gold", "test_read_manifest", "test_write_private",
                                                         "test_network", "test_read_outside_home_still_works")]
            ini = hardened.frozen_pytest_config(tree, Path(tmp) / "out")
            result = hardened.run_pytest_hardened(tree, self.venv, ids, [], ini, [], 60, home)
            self.assertEqual(result.passed, set(ids), result.output[-3000:])
            self.assertFalse((private / "w").exists())

    def test_real_home_denied_with_uv_managed_python(self) -> None:
        """Production shape: the real home is denied and the venv's Python is uv-managed under it."""
        with tempfile.TemporaryDirectory(dir=TEST_TMP) as tmp:
            venv = Path(tmp) / "venv"
            made = subprocess.run(["uv", "venv", "-q", "--managed-python", "--python", "3.11", str(venv)],
                                  capture_output=True, text=True)
            if made.returncode != 0:
                self.skipTest("no uv-managed Python 3.11: " + made.stderr[-200:])
            subprocess.run(["uv", "pip", "install", "-q", "--python", str(venv / "bin" / "python"), "pytest"],
                           check=True, capture_output=True, text=True)
            tree = Path(tmp) / "tree"; (tree / "tests").mkdir(parents=True); (Path(tmp) / "out").mkdir()
            (tree / "tests" / "test_ok.py").write_text(
                "from pathlib import Path\nimport pytest\n"
                "def test_ok():\n    assert True\n"
                "def test_codex_auth_unreadable():\n    with pytest.raises(OSError):\n"
                "        (Path.home() / '.codex' / 'auth.json').read_bytes()\n")
            ids = ["tests/test_ok.py::test_ok", "tests/test_ok.py::test_codex_auth_unreadable"]
            ini = hardened.frozen_pytest_config(tree, Path(tmp) / "out")
            result = hardened.run_pytest_hardened(tree, venv, ids, [], ini, [], 120, Path.home())
            self.assertEqual(result.passed, set(ids), result.output[-2000:])

    def test_shadow_pytest_in_tree_is_not_imported(self) -> None:
        with tempfile.TemporaryDirectory(dir=TEST_TMP) as tmp:
            tree, home, _ = self._runner_tree(Path(tmp))
            (tree / "pytest.py").write_text("raise SystemExit(0)\n")
            (tree / "tests" / "test_bad.py").write_text("def test_bad():\n    assert False\n")
            ini = hardened.frozen_pytest_config(tree, Path(tmp) / "out")
            result = hardened.run_pytest_hardened(tree, self.venv, ["tests/test_bad.py::test_bad"], ["."], ini, [], 60, home)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("tests/test_bad.py::test_bad", result.failed, result.output[-2000:])

if __name__ == "__main__":
    unittest.main()
