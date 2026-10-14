"""Unit tests for bench/e1/common.py. Run: python3 -m unittest bench/e1/test_common.py"""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import common  # noqa: E402


class SplitTests(unittest.TestCase):
    def test_split_is_deterministic_and_partitions(self) -> None:
        ids = [f"test/units/test_x.py::test_{i}" for i in range(5)]
        visible, hidden = common.split_fail_to_pass(ids)
        self.assertEqual((visible, hidden), common.split_fail_to_pass(list(reversed(ids))))
        self.assertEqual(sorted(visible + hidden), sorted(ids))
        self.assertFalse(set(visible) & set(hidden))
        self.assertEqual(len(visible), 2)
        self.assertEqual(len(hidden), 3)

    def test_split_of_two_gives_one_each(self) -> None:
        visible, hidden = common.split_fail_to_pass(["a::x", "a::y"])
        self.assertEqual(len(visible), 1)
        self.assertEqual(len(hidden), 1)

    def test_split_rejects_fewer_than_two(self) -> None:
        with self.assertRaises(ValueError):
            common.split_fail_to_pass(["a::x"])


class TestPathTests(unittest.TestCase):
    def test_official_test_path_rule(self) -> None:
        for path in [
            "test/units/utils/test_vars.py",
            "lib/ansible/tests/thing.py",
            "conftest.py",
            "pkg/foo_test.go",
            "src/app.spec.ts",
            "src/app.test.js",
            "test_top.py",
            "web/__tests__/a.js",
            "x/fixtures/data.json",
            "x/__snapshots__/a.snap",
            "x/mocks/m.py",
        ]:
            self.assertTrue(common.is_test_path(path), path)
        for path in ["lib/ansible/utils/vars.py", "docs/testing.rst", "lib/contest.py", "latest_notes.py"]:
            self.assertFalse(common.is_test_path(path), path)


class PythonVersionTests(unittest.TestCase):
    def test_python_by_base_year(self) -> None:
        self.assertEqual(common.python_for_base_date("2019-12-01"), "3.9")
        self.assertEqual(common.python_for_base_date("2021-04-16"), "3.9")
        self.assertEqual(common.python_for_base_date("2022-07-01"), "3.10")
        self.assertEqual(common.python_for_base_date("2023-09-18"), "3.11")
        self.assertEqual(common.python_for_base_date("2025-04-28"), "3.11")

    def test_exclude_newer_is_next_day(self) -> None:
        self.assertEqual(common.exclude_newer_for_base_date("2023-09-18"), "2023-09-19")
        self.assertEqual(common.exclude_newer_for_base_date("2024-12-31"), "2025-01-01")


class OpaqueIdTests(unittest.TestCase):
    def test_opaque_id_does_not_contain_instance_hashes(self) -> None:
        iid = "instance_ansible__ansible-0ea40e09d1b35bcb69ff4d9cecf3d0defa4b36e8-v30a923fb5c164d6cd18280c02422f75e611e8fb2"
        opaque = common.opaque_task_id(iid)
        self.assertRegex(opaque, r"^e1-[0-9a-f]{12}$")
        self.assertNotIn("0ea40e09", opaque)
        self.assertEqual(opaque, common.opaque_task_id(iid))


class InstructionTests(unittest.TestCase):
    def test_instruction_includes_requirements_and_interface_only_when_present(self) -> None:
        row = {"problem_statement": "Fix X.", "requirements": "Must Y.", "interface": ""}
        text = common.instruction_text(row)
        self.assertIn("Fix X.", text)
        self.assertIn("Must Y.", text)
        self.assertNotIn("Interface", text)


class IssueOnlyTests(unittest.TestCase):
    def test_issue_only_drops_requirements_and_interface(self) -> None:
        row = {"problem_statement": "  Fix X.  ", "requirements": "Must Y.", "interface": "def f(): ..."}
        text = common.instruction_text(row, issue_only=True)
        self.assertEqual(text, "Fix X.")

    def test_agent_instruction_selects_mode_from_manifest(self) -> None:
        manifest = {"instruction": "Fix X.\n\nRequirements:\nMust Y.", "problem_statement": "Fix X."}
        self.assertEqual(common.agent_instruction(manifest, "issue-only"), "Fix X.")
        self.assertEqual(common.agent_instruction(manifest, "full"), manifest["instruction"])
        with self.assertRaises(ValueError):
            common.agent_instruction(manifest, "other")
        with self.assertRaises(KeyError):
            common.agent_instruction({"instruction": "x"}, "issue-only")


class ResolutionTests(unittest.TestCase):
    def test_resolution_requires_all_required_tests(self) -> None:
        self.assertTrue(common.resolved({"a", "b"}, {"a", "b", "c"}))
        self.assertFalse(common.resolved({"a", "b"}, {"a"}))
        self.assertFalse(common.resolved(set(), {"a"}))

    def test_junit_classname_maps_to_node_id(self) -> None:
        existing = {"test/units/utils/test_vars.py"}
        node = common.junit_case_to_node_id(
            "test.units.utils.test_vars.TestVariableUtils", "test_combine_vars_replace", existing.__contains__
        )
        self.assertEqual(node, "test/units/utils/test_vars.py::TestVariableUtils::test_combine_vars_replace")
        node = common.junit_case_to_node_id("test.units.utils.test_vars", "test_fn[param-1]", existing.__contains__)
        self.assertEqual(node, "test/units/utils/test_vars.py::test_fn[param-1]")
        self.assertIsNone(common.junit_case_to_node_id("nowhere.mod", "t", existing.__contains__))


class RequirementFilesTests(unittest.TestCase):
    def test_base_plus_first_existing_unit_requirements(self) -> None:
        import tasks

        profile = tasks.REPO_PROFILES["ansible/ansible"]
        present = {"requirements.txt", "test/lib/ansible_test/_data/requirements/units.txt", "test/runner/requirements/units.txt"}
        self.assertEqual(
            tasks.requirement_files(profile, present.__contains__),
            ["requirements.txt", "test/lib/ansible_test/_data/requirements/units.txt"],
        )
        self.assertEqual(tasks.requirement_files(profile, {"requirements.txt"}.__contains__), ["requirements.txt"])


class SplitSampleTests(unittest.TestCase):
    def test_split_sample_is_seeded_and_disjoint(self) -> None:
        import tasks

        summary = [{"opaque_id": f"e1-{i:012x}", "passed": i % 4 != 0} for i in range(40)]
        a = tasks.split_sample(summary, 11, 5, 20)
        self.assertEqual(a, tasks.split_sample(list(reversed(summary)), 11, 5, 20))
        self.assertEqual(len(a["calibration"]), 5)
        self.assertEqual(len(a["sample"]), 20)
        self.assertEqual(a["gate_passed"], 30)
        everything = a["calibration"] + a["sample"] + a["reserve"]
        self.assertEqual(len(everything), len(set(everything)))
        self.assertTrue(all(int(x[3:], 16) % 4 != 0 for x in everything))


if __name__ == "__main__":
    unittest.main()
