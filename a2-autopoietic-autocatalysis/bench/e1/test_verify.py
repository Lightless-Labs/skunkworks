"""Unit tests for bench/e1/verify.py report rendering. Run: python3 -m unittest bench/e1/test_verify.py"""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import verify  # noqa: E402


class RenderReportTests(unittest.TestCase):
    def test_all_visible_pass_is_ok_and_short(self) -> None:
        ok, report = verify.render_report(["t::a"], ["t::p"], {"t::a", "t::p", "t::hidden"}, "noise")
        self.assertTrue(ok)
        self.assertIn("2/2", report)
        self.assertNotIn("noise", report)
        self.assertNotIn("hidden", report)

    def test_failures_list_only_visible_and_pass_to_pass(self) -> None:
        ok, report = verify.render_report(["t::a", "t::b"], ["t::p"], {"t::a"}, "E   assert 1 == 2")
        self.assertFalse(ok)
        self.assertIn("1/3", report)
        self.assertIn("t::b", report)
        self.assertIn("t::p", report)
        self.assertIn("assert 1 == 2", report)

    def test_output_tail_is_bounded(self) -> None:
        _, report = verify.render_report(["t::a"], [], set(), "x" * (verify.MAX_OUTPUT_CHARS * 3))
        self.assertLess(len(report), verify.MAX_OUTPUT_CHARS + 500)


if __name__ == "__main__":
    unittest.main()
