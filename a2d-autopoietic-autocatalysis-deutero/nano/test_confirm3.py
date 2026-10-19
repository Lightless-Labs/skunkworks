"""python3 -I nano/test_confirm3.py"""
import argparse
import io
import json
import pathlib
import sys
import tempfile
import unittest
from unittest import mock

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

    def test_json_transcripts_carry_no_banner(self):
        # codex 0.162 --json prints no version banner (probe 2026-10-19); the version is pinned by
        # bracketing each replicate, so a --json transcript only has to exist.
        f = Fixture(pathlib.Path(self.tmp.name) / "r1")
        for arm in ARMS:
            for t in TASKS:
                f.tuple(arm, t)
        out = f.write()
        for tr in out.rglob("*.transcript.txt"):
            tr.write_text('{"type":"thread.started"}\n{"type":"turn.completed"}\n\n--- codex stderr ---\nnoise\n')
        t, _ = c3.replicate_tuples(out, 1, "0.162.0")
        self.assertFalse(any(v["void"] for v in t.values()))

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



class Gates(unittest.TestCase):
    def test_spend_day(self):
        d = __import__("datetime").date
        g = c3.spend_gate
        self.assertEqual(g(d(2026, 10, 23), d(2026, 10, 20), True, 1, None), "before 2026-10-24")
        self.assertEqual(g(d(2026, 10, 24), d(2026, 10, 24), True, 1, None), "registration day")
        self.assertEqual(g(d(2026, 10, 24), d(2026, 10, 23), False, 1, None), "confirmation-2 decision missing")
        self.assertIsNone(g(d(2026, 10, 24), d(2026, 10, 23), True, 1, None))
        self.assertEqual(g(d(2026, 10, 24), d(2026, 10, 23), True, 2, d(2026, 10, 24)), "replicate 1's day")
        self.assertIsNone(g(d(2026, 10, 25), d(2026, 10, 23), True, 2, d(2026, 10, 24)))

    def test_latency(self):
        self.assertTrue(c3.latency_go([10.0, 50.0, 20.0]))
        self.assertFalse(c3.latency_go([10.0, float("inf"), 60.0]))


class Generate(unittest.TestCase):
    """The replicate runner's order: gates (deferrals write nothing), attempt record before
    the first model call, then the codex bracket."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        root = pathlib.Path(self.tmp.name)
        self.dir, self.runs = root / "repo-runs", root / "holdout-runs"
        self.dir.mkdir()
        self.reg = {"tasks": TASKS, "arms": ARMS, "k": 5, "model": "codex/gpt-6-luna", "effort": "low",
                    "seeds": {"1": 9191, "2": 9292}, "claim": "X:frozen>D,X:frozen>B", "lanes": {},
                    "suite_hash": "h" * 64, "codex_version": "codex-cli 0.162.0", "codex_files": {"codex": "s"}}
        self.calls = []
        fp = {"realpath": "/x/codex", "files": {"codex": "s"}, "version": "codex-cli 0.162.0"}
        patches = {"registration": lambda sha: self.reg, "commit_day": lambda sha: c3.datetime.date(2026, 10, 23),
                   "today": lambda: c3.datetime.date(2026, 10, 24), "drained": lambda: True,
                   "codex_fingerprint": lambda: dict(fp), "luna_today": lambda: 0,
                   "latency_probe": lambda model, effort: [5.0, 6.0, 7.0], "DIR": self.dir,
                   "out_dir": lambda rep, relaunch=False: self.runs / f"r{rep}{'-relaunch1' if relaunch else ''}",
                   "C2_SEALED": root / "sealed.txt"}
        (root / "sealed.txt").write_text("x\n")
        self.patchers = [mock.patch.object(c3, k, v) for k, v in patches.items()]
        self.patchers.append(mock.patch.object(c3.subprocess, "run", side_effect=self.fake_run))
        for pt in self.patchers:
            pt.start()

    def tearDown(self):
        for pt in self.patchers:
            pt.stop()
        self.tmp.cleanup()

    def fake_run(self, cmd, **kw):
        self.calls.append(cmd)
        self.assertTrue((self.dir / "attempt-r1.json").exists(), "attempt record precedes the run")
        out = pathlib.Path(cmd[cmd.index("--out") + 1])
        out.mkdir(parents=True)
        (out / "ledger.jsonl").write_text(json.dumps({"kind": "registration"}) + "\n" + json.dumps(
            {"kind": "generation_complete", "aborted": False, "ts_ms": 1_792_800_000_000}) + "\n")
        return mock.Mock(returncode=0)

    def generate(self, rep=1, relaunch=False):
        out = io.StringIO()
        with mock.patch("sys.stdout", out):
            c3.cmd_generate(argparse.Namespace(rep=rep, registered_at="abc", relaunch=relaunch, jobs=8))
        return [json.loads(x) for x in out.getvalue().splitlines() if x.strip()]

    def test_generates_once_with_attempt_record_and_bracket(self):
        self.assertEqual(self.generate()[-1]["decision"], "generated (scoring deferred)")
        self.assertEqual(len(self.calls), 1)
        cmd = self.calls[0]
        self.assertEqual(cmd[cmd.index("--seed") + 1], "9191")
        self.assertIn("--defer-scoring", cmd)
        self.assertTrue(json.loads((self.dir / "bracket-r1.json").read_text())["match"])
        self.assertEqual(self.generate()[-1]["decision"], "already generated")
        self.assertEqual(len(self.calls), 1)

    def test_deferrals_write_nothing(self):
        for name, value in (("today", lambda: c3.datetime.date(2026, 10, 23)), ("luna_today", lambda: 2800),
                            ("latency_probe", lambda model, effort: [90.0, 90.0, 5.0])):
            with mock.patch.object(c3, name, value):
                self.assertTrue(self.generate()[-1]["decision"].startswith("deferred"), name)
            self.assertFalse((self.dir / "attempt-r1.json").exists(), name)
        self.assertEqual(self.calls, [])

    def test_refusals(self):
        with mock.patch.object(c3, "drained", lambda: False), self.assertRaises(SystemExit):
            self.generate()
        bad = {"realpath": "/x/codex", "files": {"codex": "other"}, "version": "codex-cli 0.162.0"}
        with mock.patch.object(c3, "codex_fingerprint", lambda: bad), self.assertRaises(SystemExit):
            self.generate()
        self.assertEqual(self.calls, [])

    def test_a_failed_attempt_is_never_rerun_but_may_be_relaunched_once(self):
        (self.dir / "attempt-r1.json").write_text("{}\n")       # attempted, never completed
        with self.assertRaises(SystemExit):
            self.generate()
        self.assertEqual(self.generate(relaunch=True)[-1]["decision"], "generated (scoring deferred)")
        self.assertIn("r1-relaunch1", self.calls[0][self.calls[0].index("--out") + 1])
        self.assertTrue((self.dir / "attempt-r1-relaunch1.json").exists())
        self.assertTrue((self.dir / "bracket-r1-relaunch1.json").exists())
        self.assertEqual(self.generate(relaunch=True)[-1]["decision"], "already generated")
        self.assertEqual(self.generate()[-1]["decision"], "already generated")
        self.assertEqual(len(self.calls), 1)

    def test_relaunch_needs_a_failed_first_attempt(self):
        with self.assertRaises(SystemExit):
            self.generate(relaunch=True)
        self.assertEqual(self.calls, [])


if __name__ == "__main__":
    unittest.main()
