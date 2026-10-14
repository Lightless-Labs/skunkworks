"""Tests for bench/e1/decide.py. Run: python3 -m unittest bench/e1/test_decide.py"""

from __future__ import annotations

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import decide  # noqa: E402


def row(task: str, arm: str, resolved: bool | None, infra: str | None = None) -> dict:
    return {"opaque_id": task, "arm": arm, "infra_failure": infra,
            "grade": None if resolved is None else {"resolved": resolved, "hidden_resolved": resolved}}


class McNemarTests(unittest.TestCase):
    def test_exact_one_sided_p(self) -> None:
        self.assertAlmostEqual(decide.mcnemar_one_sided_p(5, 0), 1 / 32)
        self.assertAlmostEqual(decide.mcnemar_one_sided_p(0, 5), 1.0)
        self.assertEqual(decide.mcnemar_one_sided_p(0, 0), 1.0)
        # P(X >= 6 | n=8) = (28 + 8 + 1) / 256
        self.assertAlmostEqual(decide.mcnemar_one_sided_p(6, 2), 37 / 256)


class DecisionTests(unittest.TestCase):
    def test_go_requires_effect_and_significance(self) -> None:
        self.assertEqual(decide.decision(diff_pp=20.0, p=0.05, infra_fraction=0.0), "GO")
        self.assertEqual(decide.decision(diff_pp=20.0, p=0.2, infra_fraction=0.0), "INCONCLUSIVE")
        self.assertEqual(decide.decision(diff_pp=10.0, p=0.01, infra_fraction=0.0), "NO GO")
        self.assertEqual(decide.decision(diff_pp=30.0, p=0.001, infra_fraction=0.25), "INCONCLUSIVE")


class AnalysisTests(unittest.TestCase):
    def test_paired_analysis_drops_persistent_infra_tasks_and_uses_reserves(self) -> None:
        rows = []
        for i in range(10):
            t = f"t{i}"
            rows.append(row(t, "raw", i < 2))
            rows.append(row(t, "stateless-retry", i < 4))
            rows.append(row(t, "a2-full", i < 8))
        rows = [r for r in rows if not (r["opaque_id"] == "t9" and r["arm"] == "a2-full")]
        rows.append(row("t9", "a2-full", None, infra="provider_error"))
        rows += [row("r0", a, a == "a2-full") for a in ("raw", "stateless-retry", "a2-full")]
        result = decide.analyse(rows, sample=[f"t{i}" for i in range(10)], reserve=["r0", "r1"])
        self.assertEqual(result["n_paired"], 10)
        self.assertEqual(result["dropped_infra"], ["t9"])
        self.assertEqual(result["reserves_used"], ["r0"])
        self.assertEqual(result["resolved"]["a2-full"], 9)   # t0..t7 + r0
        self.assertEqual(result["resolved"]["stateless-retry"], 4)
        self.assertEqual((result["b"], result["c"]), (5, 0))
        self.assertAlmostEqual(result["diff_pp"], 50.0)
        self.assertEqual(result["decision"], "GO")

    def test_incomplete_tasks_are_reported_not_scored(self) -> None:
        rows = [row("t0", "raw", True), row("t0", "stateless-retry", True)]
        result = decide.analyse(rows, sample=["t0"], reserve=[])
        self.assertEqual(result["incomplete"], ["t0"])
        self.assertEqual(result["n_paired"], 0)


if __name__ == "__main__":
    unittest.main()
