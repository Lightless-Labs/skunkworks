"""python3 -I nano/test_confirm3.py"""
import json
import pathlib
import sys
import tempfile
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import confirm3 as c3  # noqa: E402

TASKS = [f"f{i:02d}-l1" for i in range(20)]
ARMS = ["D", "B", "X:frozen"]


def comparison(comp, lift, lo, hi, families=20, dx=3, dy=1):
    return {"treatment": "X:frozen", "comparator": comp, "n": 40, "families": families, "lift": lift,
            "ci95": [lo, hi], "discordant_treatment_only": dx, "discordant_comparator_only": dy,
            "class": "IntegrityReview"}


def summary(claim="SUPPORTED", d=(0.15, 0.02, 0.28), b=(0.12, 0.01, 0.25), hits=(), excluded=0.0):
    return {"claim": claim, "hidden_name_hits": list(hits), "excluded_share": excluded, "max_excluded_share": 0.05,
            "min_families": 10, "aborted": False,
            "comparisons": [comparison("D", *d), comparison("B", *b)]}


class Fixture:
    """One replicate's out dir: a ledger plus invocation prompt/transcript files."""

    def __init__(self, root):
        self.out = pathlib.Path(root)
        self.recs = [{"kind": "registration", "arms": ARMS, "tasks": TASKS, "replicates": 1}]

    def tuple(self, arm, task, passed=True, void=None, tools=0, transcript="v0.162.0", calls=2):
        d = self.out / "invocations" / arm.replace(":", "-") / task / "r0"
        d.mkdir(parents=True, exist_ok=True)
        for i in range(calls):
            prompt = d / f"inv{i:02d}-lift-coder.prompt.txt"
            prompt.write_text("p")
            if transcript and i == calls - 1:
                (d / f"inv{i:02d}-lift-coder.transcript.txt").write_text(
                    f"Reading additional input from stdin...\nOpenAI Codex {transcript}\n--------\n")
            elif i < calls - 1:
                (d / f"inv{i:02d}-lift-coder.transcript.txt").write_text("OpenAI Codex v0.162.0\n")
            # The ledger records the run's absolute path; scoring resolves it under this out dir.
            self.recs.append({"kind": "invocation", "arm": arm, "task": task, "replicate": 0, "index": i,
                              "prompt_path": f"/elsewhere/run/invocations/{arm.replace(':', '-')}/{task}/r0/{prompt.name}"})
        self.recs.append({"kind": "submission", "arm": arm, "task": task, "replicate": 0, "invocations": calls,
                          "tool_calls": tools, "void_reason": void})
        if not void:
            self.recs.append({"kind": "score", "arm": arm, "task": task, "replicate": 0, "family": task[:3],
                              "result": {"all_passed": passed}})

    def write(self):
        (self.out / "ledger.jsonl").write_text("".join(json.dumps(r) + "\n" for r in self.recs))
        return self.out


class Classify(unittest.TestCase):
    # Mirrors crates/a2d-lift/src/stats.rs classify / classify_guarded.
    def test_matches_the_controller_rules(self):
        cls = lambda lift, lo, hi, fam=20, dx=1, dy=0: c3.classify(comparison("D", lift, lo, hi, fam, dx, dy), 10)
        self.assertEqual(cls(0.0, -0.05, 0.0999), "RuledOut")
        self.assertEqual(cls(8 / 80, 0.01, 0.2), "Supported")          # 0.09999999999999998 is on the boundary
        self.assertEqual(cls(0.2, 0.0, 0.3), "Inconclusive")            # ci_low must be > 0
        self.assertEqual(cls(0.2, 0.05, 0.3, fam=9), "Inconclusive")     # family floor
        self.assertEqual(cls(0.2, 0.05, 0.3, dx=0, dy=0), "Inconclusive")


class Tuples(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()

    def tearDown(self):
        self.tmp.cleanup()

    def replicate(self, name, special=()):
        f = Fixture(pathlib.Path(self.tmp.name) / name)
        for arm in ARMS:
            for t in TASKS:
                kw = dict(special).get((arm, t), {})
                f.tuple(arm, t, **{"passed": arm == "X:frozen" or t < "f10", **kw})
        return f.write()

    def test_voids_per_tuple(self):
        out = self.replicate("r1", {("X:frozen", "f00-l1"): {"void": "provider"},
                                    ("X:frozen", "f01-l1"): {"tools": 1},
                                    ("X:frozen", "f02-l1"): {"transcript": None},       # failed call: no transcript
                                    ("X:frozen", "f03-l1"): {"transcript": "v0.163.0"},
                                    ("D", "f04-l1"): {"transcript": "v0.162.01"}})
        t, stats = c3.replicate_tuples(out, 1, "0.162.0")
        void = sorted((a, k) for (_, a, k), v in t.items() if v["void"])
        self.assertEqual(void, [("D", "f04-l1"), ("X:frozen", "f00-l1"), ("X:frozen", "f01-l1"),
                                ("X:frozen", "f02-l1"), ("X:frozen", "f03-l1")])
        self.assertEqual(len(t), 60)
        self.assertEqual(stats, {"invocations": 120, "tool_calls": 1})

    def test_missing_score_is_void(self):
        f = Fixture(pathlib.Path(self.tmp.name) / "r1")
        for arm in ARMS:
            for t in TASKS[:-1]:
                f.tuple(arm, t)
        t, _ = c3.replicate_tuples(f.write(), 1, "0.162.0")
        self.assertTrue(all(t[(1, a, TASKS[-1])]["void"] for a in ARMS))

    def test_imputation_worst_and_best(self):
        t1, _ = c3.replicate_tuples(self.replicate("r1", {("X:frozen", "f00-l1"): {"void": "x"},
                                                          ("D", "f15-l1"): {"void": "x"}}), 1, "0.162.0")
        # X passes everything, D passes f00..f09. Worst: X loses f00, D gains f15 -> (19 - 11) / 20.
        self.assertAlmostEqual(c3.imputed_lift(t1, "D", "worst"), 8 / 20)
        # Best: X keeps f00, D loses f15 (it failed anyway) -> (20 - 10) / 20.
        self.assertAlmostEqual(c3.imputed_lift(t1, "D", "best"), 10 / 20)


class Decide(unittest.TestCase):
    def tuples(self, x_void=0, d_void=0, x_pass=20, d_pass=10, b_pass=10):
        t = {}
        for i, task in enumerate(TASKS):
            t[(1, "X:frozen", task)] = {"pass": i < x_pass, "void": i < x_void}
            t[(1, "D", task)] = {"pass": i < d_pass, "void": TASKS.index(task) >= 20 - d_void}
            t[(1, "B", task)] = {"pass": i < b_pass, "void": False}
        return t

    def decide(self, s, t, rate=(0, 100)):
        return c3.decide(s, t, {1: {"tool_calls": rate[0], "invocations": rate[1]}})

    def test_supported_needs_the_worst_case_point_lift(self):
        self.assertEqual(self.decide(summary(), self.tuples())["overall"], "SUPPORTED")
        # 3 void X tuples (5% of 60, at the gate): worst-case X 17/20 vs D 10/20 -> +35 pp, still SUPPORTED.
        self.assertEqual(self.decide(summary(), self.tuples(x_void=3))["overall"], "SUPPORTED")
        # X 12/20 with 3 voids: worst case 9/20 vs 10/20 -> below +10 pp.
        d = self.decide(summary(), self.tuples(x_void=3, x_pass=12))
        self.assertEqual(d["overall"], "INCONCLUSIVE (worst-case void imputation)")
        self.assertEqual(d["worst_case_void_lifts"]["D"], -0.05)

    def test_tool_events_alone_do_not_make_the_run_integrity_review(self):
        # The controller says IntegrityReview on any tool call; §0.4 voids the tuple instead.
        s = summary(claim="IntegrityReview")
        self.assertEqual(self.decide(s, self.tuples(x_void=1), rate=(1, 1000))["overall"], "SUPPORTED")

    def test_run_level_integrity_and_validity(self):
        self.assertEqual(self.decide(summary(hits=["h"]), self.tuples())["overall"], "IntegrityReview")
        self.assertEqual(self.decide(summary(), self.tuples(), rate=(6, 1000))["overall"], "IntegrityReview")
        self.assertEqual(self.decide(summary(), self.tuples(), rate=(5, 1000))["overall"], "SUPPORTED")
        # 3 of 60 void tuples is 5.0% (valid); 4 of 60 is 6.7% > 5%.
        self.assertEqual(self.decide(summary(), self.tuples(x_void=3))["overall"], "SUPPORTED")
        self.assertEqual(self.decide(summary(), self.tuples(x_void=3, d_void=1))["overall"], "Invalid")
        self.assertEqual(self.decide(summary(excluded=0.06), self.tuples())["overall"], "Invalid")

    def test_ruled_out_needs_the_best_case_point_lift(self):
        s = summary(claim="RULED OUT", d=(0.0, -0.05, 0.08))
        self.assertEqual(self.decide(s, self.tuples(x_pass=10))["overall"], "RULED OUT")
        # 3 void X tuples that failed: best case counts them as passes -> 13/20 vs 10/20 = +15 pp.
        x = self.tuples(x_pass=10)
        for task in TASKS[17:]:
            x[(1, "X:frozen", task)] = {"pass": False, "void": True}
        self.assertEqual(self.decide(s, x)["overall"], "INCONCLUSIVE (best-case void imputation)")

    def test_output_is_counts_only(self):
        d = self.decide(summary(), self.tuples())
        self.assertNotIn("f00", json.dumps(d))


if __name__ == "__main__":
    unittest.main()
