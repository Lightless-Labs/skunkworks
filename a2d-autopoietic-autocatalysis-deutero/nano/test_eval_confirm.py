"""python3 -I nano/test_eval_confirm.py"""
import argparse
import pathlib
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
sys.argv = sys.argv[:1]
import eval as ev  # noqa: E402


class SealedConfirmScore(unittest.TestCase):
    def test_refuses_once_the_decision_is_sealed(self):
        # The sealed decision lives in the holdout dir; a re-run would re-pool and write
        # decision.json back into the repo (and print it), so it must stop first.
        with tempfile.TemporaryDirectory() as d:
            (pathlib.Path(d) / "sealed-sha256.txt").write_text("x  decision.json\n")
            with mock.patch.object(ev, "CONFIRM2_DIR", pathlib.Path(d)), \
                    mock.patch.object(ev, "git", side_effect=AssertionError("checked git")), \
                    mock.patch.object(ev.subprocess, "run", side_effect=AssertionError("ran a2d")):
                with self.assertRaises(SystemExit) as e:
                    ev.cmd_confirm_score(argparse.Namespace(jobs=6))
            self.assertIn("sealed", str(e.exception.code))
            self.assertFalse((pathlib.Path(d) / "decision.json").exists())


if __name__ == "__main__":
    unittest.main()
