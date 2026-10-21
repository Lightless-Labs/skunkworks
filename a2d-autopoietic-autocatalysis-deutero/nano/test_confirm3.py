"""python3 -I nano/test_confirm3.py"""
import argparse
import datetime
import io
import json
import pathlib
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import confirm3 as c3  # noqa: E402

TASKS = [f"c3-f{i:02d}-l1" for i in range(20)]
ARMS = ["D", "B", "X:frozen"]
NOTICE_LINE = json.dumps({"type": "item.completed", "item": {"id": "item_0", "type": "error", "message": c3.CODEX_HOST_OFF_NOTICE}})
JSON_TRANSCRIPT = ('{"type":"thread.started"}\n' + NOTICE_LINE + '\n{"type":"turn.started"}\n'
                   '{"type":"item.completed","item":{"type":"agent_message","text":"x"}}\n{"type":"turn.completed"}\n'
                   '\n--- codex stderr ---\nnoise\n')
IT = 2000                                   # bootstrap iterations in tests


def comparison(comp, lift, lo, hi, families=20, dx=3, dy=1, cls="Supported"):
    return {"treatment": "X:frozen", "comparator": comp, "n": 40, "families": families, "lift": lift,
            "ci95": [lo, hi], "discordant_treatment_only": dx, "discordant_comparator_only": dy, "class": cls}


def summary(claim="SUPPORTED", d=(0.15, 0.02, 0.28), b=(0.12, 0.01, 0.25), hits=(), excluded=0.0,
            integrity_ok=True, classes=("Supported", "Supported")):
    return {"claim": claim, "hidden_name_hits": list(hits), "excluded_share": excluded, "max_excluded_share": 0.05,
            "min_families": 10, "aborted": False, "integrity_ok": integrity_ok,
            "comparisons": [comparison("D", *d, cls=classes[0]), comparison("B", *b, cls=classes[1])]}


class Fixture:
    """One replicate's out dir: a ledger plus invocation prompt/transcript files."""

    def __init__(self, root, arms=ARMS, tasks=TASKS):
        self.out = pathlib.Path(root)
        self.recs = [{"kind": "registration", "arms": list(arms), "tasks": list(tasks), "replicates": 1}]

    def tuple(self, arm, task, passed=True, void=None, tools=0, transcript=JSON_TRANSCRIPT, calls=2, claimed=None,
              inv_tools=0, malformed_score=False):
        d = self.out / "invocations" / arm.replace(":", "-") / task / "r0"
        d.mkdir(parents=True, exist_ok=True)
        for i in range(calls):
            prompt = d / f"inv{i:02d}-lift-coder.prompt.txt"
            prompt.write_text("p")
            last = i == calls - 1
            if not last or transcript is not None:
                (d / f"inv{i:02d}-lift-coder.transcript.txt").write_text(JSON_TRANSCRIPT if not last else transcript)
            # The ledger records the run's absolute path; scoring resolves it under this out dir.
            self.recs.append({"kind": "invocation", "arm": arm, "task": task, "replicate": 0, "index": i,
                              "tool_calls": inv_tools if last else 0,
                              "prompt_path": f"/elsewhere/run/invocations/{arm.replace(':', '-')}/{task}/r0/{prompt.name}"})
        self.recs.append({"kind": "submission", "arm": arm, "task": task, "replicate": 0,
                          "invocations": calls if claimed is None else claimed, "tool_calls": tools, "void_reason": void})
        if not void:
            self.recs.append({"kind": "score", "arm": arm, "task": task, "replicate": 0, "family": task[:6],
                              **({} if malformed_score else {"result": {"all_passed": passed}})})

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


class Bootstrap(unittest.TestCase):
    def test_single_family_collapses_and_concentrated_lift_has_low_zero(self):
        self.assertEqual(c3.bootstrap([("a", True, False)] * 4, 500), (1.0, 1.0, 1.0))
        # Lift only in 2 of 10 families: a resample without them (about 11% of draws) has lift 0.
        obs = [(f"f{i}", i < 2, False) for i in range(10) for _ in range(4)]
        lift, lo, hi = c3.bootstrap(obs, IT)
        self.assertAlmostEqual(lift, 0.2)
        self.assertLessEqual(lo, 0.0)
        self.assertGreater(hi, 0.2)


class Tuples(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()

    def tearDown(self):
        self.tmp.cleanup()

    def replicate(self, special=(), arms=ARMS):
        f = Fixture(pathlib.Path(self.tmp.name) / "r1", arms=arms)
        for arm in ARMS:
            for t in TASKS:
                kw = dict(special).get((arm, t), {})
                f.tuple(arm, t, **{"passed": arm == "X:frozen" or t < "c3-f10", **kw})
        return f.write()

    def tuples(self, out):
        return c3.replicate_tuples(out, 1, ARMS, TASKS)

    def test_voids_per_tuple(self):
        out = self.replicate({("X:frozen", "c3-f00-l1"): {"void": "provider"},
                              ("X:frozen", "c3-f01-l1"): {"tools": 1},
                              ("X:frozen", "c3-f02-l1"): {"transcript": None},              # failed call: no transcript
                              ("X:frozen", "c3-f03-l1"): {"transcript": "OpenAI Codex v0.162.0\n"},  # legacy provider
                              ("D", "c3-f04-l1"): {"claimed": 3}})                          # records != submission count
        t, stats = self.tuples(out)
        void = sorted((a, k) for (_, a, k), v in t.items() if v["void"])
        self.assertEqual(void, [("D", "c3-f04-l1"), ("X:frozen", "c3-f00-l1"), ("X:frozen", "c3-f01-l1"),
                                ("X:frozen", "c3-f02-l1"), ("X:frozen", "c3-f03-l1")])
        self.assertEqual(len(t), 60)
        self.assertEqual(stats, {"invocations": 121, "tool_calls": 1})

    def test_an_invocation_level_tool_event_voids_its_tuple_and_counts(self):
        # A judge-lane call's events may not sum into the submission (fresh-Claude review B3).
        out = self.replicate({("X:frozen", "c3-f05-l1"): {"inv_tools": 3}})
        t, stats = self.tuples(out)
        self.assertEqual([k for (_, a, k), v in t.items() if v["void"]], ["c3-f05-l1"])
        self.assertEqual(stats["tool_calls"], 3)

    def test_arm_order_in_the_ledger_does_not_matter(self):
        t, _ = self.tuples(self.replicate(arms=list(reversed(ARMS))))
        self.assertFalse(any(v["void"] for v in t.values()))

    def test_a_malformed_record_is_void_not_a_crash(self):
        t, _ = self.tuples(self.replicate({("D", "c3-f06-l1"): {"malformed_score": True}}))
        self.assertEqual([(a, k) for (_, a, k), v in t.items() if v["void"]], [("D", "c3-f06-l1")])

    def test_the_independent_recount_and_host_off_check_void_tuples(self):
        tool = JSON_TRANSCRIPT.replace('{"type":"turn.completed"}',
                                       '{"type":"item.completed","item":{"type":"mcp_tool_call"}}\n{"type":"turn.completed"}')
        host_on = JSON_TRANSCRIPT.replace(NOTICE_LINE + "\n", "")
        t, stats = self.tuples(self.replicate({("X:frozen", "c3-f07-l1"): {"transcript": tool},
                                               ("B", "c3-f08-l1"): {"transcript": host_on}}))
        self.assertEqual(sorted((a, k) for (_, a, k), v in t.items() if v["void"]),
                         [("B", "c3-f08-l1"), ("X:frozen", "c3-f07-l1")])
        self.assertEqual(stats["tool_calls"], 1)                # recorded 0; the port counted 1

    def test_py_tool_events_matches_the_provider_rules(self):
        T = lambda *lines: "\n".join(lines) + "\n--- codex stderr ---\n"   # noqa: E731
        st, tu, msg, done = '{"type":"thread.started"}', '{"type":"turn.started"}', \
            '{"type":"item.completed","item":{"type":"agent_message"}}', '{"type":"turn.completed"}'
        other = '{"type":"item.completed","item":{"type":"error","message":"x"}}'
        self.assertEqual(c3.py_tool_events(T(st, NOTICE_LINE, tu, msg, done)), 0)
        self.assertEqual(c3.py_tool_events(T(st, tu, NOTICE_LINE, msg, done)), 1)
        self.assertEqual(c3.py_tool_events(T(st, other, tu, msg, done)), 1)
        self.assertEqual(c3.py_tool_events(T(st, NOTICE_LINE, NOTICE_LINE, tu, msg, done)), 1)
        self.assertEqual(c3.py_tool_events(T(st, NOTICE_LINE, tu, msg)), 1)              # no turn.completed
        self.assertEqual(c3.py_tool_events(T(st, NOTICE_LINE, tu, msg), failed=True), 0)
        self.assertEqual(c3.py_tool_events(T(st, NOTICE_LINE, tu, '{"type":"item.comp')), 2)  # cut-off line
        self.assertTrue(c3.host_off(T(st, NOTICE_LINE, tu, msg, done)))
        self.assertFalse(c3.host_off(T(st, tu, msg, done)))
        self.assertTrue(c3.host_off(T(st)))                    # no turn ran

    def test_unicode_line_separators_inside_json_strings_are_not_lines(self):
        # Rust's str::lines() splits on \n only; serde_json leaves U+2028/U+2029/U+0085 raw (Claude re-check 3).
        msg = json.dumps({"type": "item.completed", "item": {"type": "agent_message",
                                                              "text": "a\u2028b\u2029c\u0085d\x0be\x0cf\x1cg"}},
                         ensure_ascii=False)
        t = '{"type":"thread.started"}\n' + NOTICE_LINE + '\n{"type":"turn.started"}\n' + msg + \
            '\r\n{"type":"turn.completed"}\n--- codex stderr ---\n'
        self.assertEqual(c3.py_tool_events(t), 0)
        self.assertTrue(c3.host_off(t))
        self.assertEqual(c3.py_tool_events(t.replace(msg, '{"type":"item.completed","v":NaN}')), 1)  # NaN: unparsable
        with tempfile.TemporaryDirectory() as d:
            (pathlib.Path(d) / "ledger.jsonl").write_text(json.dumps({"kind": "x", "t": "a\u2028b"}, ensure_ascii=False) + "\n")
            self.assertEqual(c3.records_tolerant(d)[1], 0)

    def test_ledger_planned_otherwise_voids_everything(self):
        t, _ = self.tuples(self.replicate(arms=["D", "X:frozen"]))
        self.assertTrue(all(v["void"] for v in t.values()))

    def test_missing_score_is_void(self):
        f = Fixture(pathlib.Path(self.tmp.name) / "r1")
        for arm in ARMS:
            for t in TASKS[:-1]:
                f.tuple(arm, t)
        t, _ = self.tuples(f.write())
        self.assertTrue(all(t[(1, a, TASKS[-1])]["void"] for a in ARMS))

    def test_imputation_worst_and_best(self):
        t, _ = self.tuples(self.replicate({("X:frozen", "c3-f00-l1"): {"void": "x"}, ("D", "c3-f15-l1"): {"void": "x"}}))
        # X passes everything, D passes f00..f09. Worst: X loses f00, D gains f15 -> (19 - 11) / 20.
        self.assertAlmostEqual(c3.bootstrap(c3.imputed(t, "D", "worst"), 0)[0], 8 / 20)
        # Best: X keeps f00, D loses f15 (it failed anyway) -> (20 - 10) / 20.
        self.assertAlmostEqual(c3.bootstrap(c3.imputed(t, "D", "best"), 0)[0], 10 / 20)


class Decide(unittest.TestCase):
    def tuples(self, x_void=0, d_void=0, x_pass=20, d_pass=10, b_pass=10):
        t = {}
        for i, task in enumerate(TASKS):
            t[(1, "X:frozen", task)] = {"pass": i < x_pass, "void": i < x_void}
            t[(1, "D", task)] = {"pass": i < d_pass, "void": i >= 20 - d_void}
            t[(1, "B", task)] = {"pass": i < b_pass, "void": False}
        return t

    def decide(self, s, t, rate=(0, 100)):
        # iterations=0: the imputed CI collapses to the point, isolating the point and gate rules
        # (the fixture has one task per family); the clustered test runs the real bootstrap.
        return c3.decide(s, t, {1: {"tool_calls": rate[0], "invocations": rate[1]}}, 0)

    def test_supported_needs_the_worst_case_point_and_ci(self):
        self.assertEqual(self.decide(summary(), self.tuples())["overall"], "SUPPORTED")
        # 3 void X tuples (5% of 60, at the gate): worst-case X 17/20 vs D 10/20 -> +35 pp, still SUPPORTED.
        self.assertEqual(self.decide(summary(), self.tuples(x_void=3))["overall"], "SUPPORTED")
        # X 12/20 with 3 voids: worst case 9/20 vs 10/20 -> below +10 pp.
        d = self.decide(summary(), self.tuples(x_void=3, x_pass=12))
        self.assertEqual(d["overall"], "INCONCLUSIVE (worst-case void imputation)")
        self.assertEqual(d["worst_case_void"]["D"]["lift"], -0.05)

    def test_clustered_lift_fails_the_imputed_ci_although_the_point_passes(self):
        # 2 families of 10 tasks, r = 2: the +50 pp lift sits entirely in the first family, so a
        # family bootstrap that draws only the second family has lift 0 (25% of draws).
        t = {}
        for i in range(20):
            task = f"c3-f{i // 10:02d}-l{i % 10}"
            for rep in (1, 2):
                t[(rep, "X:frozen", task)] = {"pass": True, "void": False}
                t[(rep, "D", task)] = {"pass": i >= 10, "void": False}
                t[(rep, "B", task)] = {"pass": i >= 10, "void": False}
        d = c3.decide(summary(), t, {1: {"tool_calls": 0, "invocations": 100}}, IT)
        self.assertGreaterEqual(d["worst_case_void"]["D"]["lift"], 0.1)
        self.assertLessEqual(d["worst_case_void"]["D"]["ci_low"], 0.0)
        self.assertEqual(d["overall"], "INCONCLUSIVE (worst-case void imputation)")

    def test_controller_classes_must_agree(self):
        # A controller INCONCLUSIVE can never become a claim through the re-classification.
        s = summary(claim="INCONCLUSIVE", classes=("Inconclusive", "Inconclusive"))
        self.assertEqual(self.decide(s, self.tuples())["overall"], "INCONCLUSIVE")
        r = summary(claim="INCONCLUSIVE", d=(0.0, -0.05, 0.08), classes=("Inconclusive", "Inconclusive"))
        self.assertEqual(self.decide(r, self.tuples(x_pass=10))["overall"], "INCONCLUSIVE")

    def test_controller_verdicts_dominate(self):
        self.assertEqual(self.decide(summary(claim="IntegrityReview", integrity_ok=False), self.tuples())["overall"],
                         "IntegrityReview")
        self.assertEqual(self.decide(summary(claim="Invalid", classes=("Invalid", "Invalid")), self.tuples())["overall"],
                         "Invalid")

    def test_a_tool_event_below_the_rate_voids_its_tuple_only(self):
        self.assertEqual(self.decide(summary(), self.tuples(x_void=1), rate=(1, 1000))["overall"], "SUPPORTED")

    def test_run_level_integrity_and_validity(self):
        self.assertEqual(self.decide(summary(hits=["h"]), self.tuples())["overall"], "IntegrityReview")
        self.assertEqual(self.decide(summary(), self.tuples(), rate=(6, 1000))["overall"], "IntegrityReview")
        self.assertEqual(self.decide(summary(), self.tuples(), rate=(5, 1000))["overall"], "SUPPORTED")
        # 3 of 60 void tuples is 5.0% (valid); 4 of 60 is 6.7% > 5%.
        self.assertEqual(self.decide(summary(), self.tuples(x_void=3))["overall"], "SUPPORTED")
        self.assertEqual(self.decide(summary(), self.tuples(x_void=3, d_void=1))["overall"], "Invalid")
        self.assertEqual(self.decide(summary(excluded=0.06), self.tuples())["overall"], "Invalid")

    def test_ruled_out_needs_the_best_case_ci(self):
        s = summary(claim="RULED OUT", d=(0.0, -0.05, 0.08), classes=("RuledOut", "Inconclusive"))
        self.assertEqual(self.decide(s, self.tuples(x_pass=10))["overall"], "RULED OUT")
        # 3 void X tuples that failed: best case counts them as passes -> 13/20 vs 10/20 = +15 pp.
        x = self.tuples(x_pass=10)
        for task in TASKS[17:]:
            x[(1, "X:frozen", task)] = {"pass": False, "void": True}
        self.assertEqual(self.decide(s, x)["overall"], "INCONCLUSIVE (best-case void imputation)")

    def test_a_malformed_comparison_is_invalid_not_a_crash(self):
        s = summary()
        for c in s["comparisons"]:
            c.pop("ci95", None)
        self.assertEqual(self.decide(s, self.tuples())["overall"], "Invalid")

    def test_malformed_summaries_are_invalid(self):
        for broken in ({**summary(), "comparisons": 1}, {**summary(), "excluded_share": None}, [1, 2]):
            self.assertEqual(self.decide(broken, self.tuples())["overall"], "Invalid")
        nan = summary()
        nan["comparisons"][0]["ci95"] = [0.02, float("nan")]
        self.assertEqual(self.decide(nan, self.tuples())["overall"], "Invalid")

    def test_guard_fields_and_replicate_summaries_are_validated(self):
        for broken in ({**summary(), "min_families": True}, {**summary(), "min_families": -1},
                       {**summary(), "excluded_share": 1.5}, {**summary(), "max_excluded_share": -0.1}):
            self.assertEqual(self.decide(broken, self.tuples())["overall"], "Invalid")
        s = c3.fold_replicate_summary(summary(), [1, 2])               # a malformed replicate summary
        self.assertEqual(self.decide(s, self.tuples())["overall"], "IntegrityReview")
        s = c3.fold_replicate_summary(summary(), {"hidden_name_hits": 1, "integrity_ok": True})
        self.assertEqual(self.decide(s, self.tuples())["overall"], "IntegrityReview")
        s = c3.fold_replicate_summary(summary(), {"hidden_name_hits": [], "integrity_ok": True})
        self.assertEqual(self.decide(s, self.tuples())["overall"], "SUPPORTED")
        self.assertEqual(self.decide(c3.load_summary_obj("not json"), self.tuples())["overall"], "Invalid")

    def test_a_truncated_replicate_summary_blocks_the_claim_end_to_end(self):
        # Sol re-check 4: load -> fold -> decide on "", "{" and a summary without its integrity flag.
        for text in ("", "{", json.dumps({"hidden_name_hits": []})):
            s = c3.fold_replicate_summary(summary(), c3.load_summary_obj(text))
            self.assertEqual(self.decide(s, self.tuples())["overall"], "IntegrityReview", text)

    def test_ruled_out_needs_the_best_case_point_too(self):
        # D voids imputed as fails can lift X's best case above +10 pp at the point while the CI
        # bound alone would not show it with iterations=0 (point = bounds): both must hold.
        s = summary(claim="RULED OUT", d=(0.0, -0.05, 0.08), classes=("RuledOut", "Inconclusive"))
        t = self.tuples(x_pass=12, d_pass=10, d_void=3)
        self.assertEqual(self.decide(s, t)["overall"], "INCONCLUSIVE (best-case void imputation)")

    def test_lane_usage_counts_calls_and_tokens(self):
        with tempfile.TemporaryDirectory() as d:
            recs = [{"kind": "invocation", "arm": "X:frozen", "provider": "codex/gpt-6.1-sol",
                     "usage": {"prompt_tokens": 10, "completion_tokens": 5, "reported_total": 15}},
                    {"kind": "invocation", "arm": "X:frozen", "provider": "codex/gpt-6-luna", "usage": None}]
            (pathlib.Path(d) / "ledger.jsonl").write_text("\n".join(map(json.dumps, recs)) + "\n")
            self.assertEqual(c3.lane_usage(d), {"X:frozen": {"codex/gpt-6.1-sol": {"calls": 1, "tokens": 15},
                                                             "codex/gpt-6-luna": {"calls": 1, "tokens": 0}}})

    def test_a_malformed_summary_is_invalid_not_a_crash(self):
        self.assertEqual(self.decide({"claim": "SUPPORTED"}, self.tuples())["overall"], "Invalid")
        self.assertFalse(c3.transcript_ok("/x", None))

    def test_output_is_counts_only(self):
        self.assertNotIn("c3-f00", json.dumps(self.decide(summary(), self.tuples())))


class FailedAttempt(unittest.TestCase):
    def test_test_names_port_matches_the_controller(self):
        self.assertEqual(c3.test_names("#[test] fn one_line() {}\n"), {"one_line"})
        self.assertEqual(c3.test_names("fn helper() {}\n#[test]\n#[should_panic]\nfn panics() {}\n#[test]\nfn plain() {}\n"),
                         {"panics", "plain"})

    def test_audit_counts_invocations_unaudited_calls_and_hidden_names(self):
        with tempfile.TemporaryDirectory() as d:
            suite, out = pathlib.Path(d) / "suite", pathlib.Path(d) / "r1"
            (suite / "t-l1").mkdir(parents=True)
            (suite / "t-l1" / "public_tests.rs").write_text("#[test]\nfn shown() {}\n")
            (suite / "t-l1" / "hidden_tests.rs").write_text("#[test]\nfn shown() {}\n#[test]\nfn secret_case() {}\n")
            inv = out / "invocations" / "X-frozen" / "t-l1" / "r0"
            inv.mkdir(parents=True)
            (inv / "inv00-lift-coder.prompt.txt").write_text("PROMPT shown")
            (inv / "inv00-lift-coder.transcript.txt").write_text("{}" + c3.CODEX_STDERR_MARKER + "x")
            (inv / "inv01-lane_judge.prompt.txt").write_text("files: fn secret_case() {}")   # no transcript
            recs = [{"kind": "invocation", "prompt_path": str(inv / "inv00-lift-coder.prompt.txt"), "tool_calls": 2},
                    {"kind": "invocation", "prompt_path": str(inv / "inv01-lane_judge.prompt.txt"), "tool_calls": None}]
            (out / "ledger.jsonl").write_text("\n".join(map(json.dumps, recs)) + "\n")
            with mock.patch.object(c3, "SUITE", suite):
                a = c3.failed_attempt_audit(out, ["t-l1"])
        self.assertEqual(a, {"invocations": 2, "tool_calls": 3, "unaudited": 1, "hidden_name_hits": 1})

    def test_an_unauditable_failed_call_blocks_the_claim_and_submission_events_count(self):
        # Sol re-check: one unauditable call could hide any number of events; it blocks a claim.
        stats = {1: {"invocations": 1600, "tool_calls": 0}}
        s = c3.merge_failed_attempt(summary(), stats, 1, {"invocations": 1, "tool_calls": 1, "unaudited": 1,
                                                         "hidden_name_hits": 0})
        self.assertEqual(c3.decide(s, Decide.tuples(Decide()), stats, 0)["overall"], "IntegrityReview")
        with tempfile.TemporaryDirectory() as d:
            out = pathlib.Path(d)
            (out / "ledger.jsonl").write_text(json.dumps({"kind": "submission", "tool_calls": 10, "invocations": 5}) + "\n")
            with mock.patch.object(c3, "SUITE", out / "none"):
                self.assertEqual(c3.failed_attempt_audit(out, [])["tool_calls"], 10)

    def test_a_dead_attempts_truncated_ledger_counts_as_unaudited_not_a_crash(self):
        with tempfile.TemporaryDirectory() as d:
            out = pathlib.Path(d)
            (out / "ledger.jsonl").write_text(json.dumps({"kind": "invocation", "tool_calls": 0}) + '\n{"kind": "invoc')
            with mock.patch.object(c3, "SUITE", out / "none"):
                a = c3.failed_attempt_audit(out, [])
        self.assertEqual((a["invocations"], a["unaudited"]), (1, 2))   # the bad line and the transcript-less call

    def test_a_failed_attempt_above_the_rate_on_its_own_is_integrity_review(self):
        stats = {1: {"invocations": 100000, "tool_calls": 0}}
        s = c3.merge_failed_attempt(summary(), stats, 1, {"invocations": 100, "tool_calls": 1, "unaudited": 0,
                                                         "hidden_name_hits": 0})
        self.assertEqual(c3.decide(s, Decide.tuples(Decide()), stats, 0)["overall"], "IntegrityReview")

    def test_a_call_started_but_never_recorded_is_unauditable(self):
        # The meter writes a durable invocation_started before each call (plan §0.16): a call in
        # flight when the run died is an unmatched start.
        with tempfile.TemporaryDirectory() as d:
            out = pathlib.Path(d)
            recs = [{"kind": "invocation_started", "arm": "D", "task": "t", "replicate": 0, "index": 0},
                    {"kind": "invocation_started", "arm": "D", "task": "t", "replicate": 0, "index": 1}]
            (out / "ledger.jsonl").write_text("\n".join(map(json.dumps, recs)) + "\n")
            with mock.patch.object(c3, "SUITE", out / "none"):
                self.assertEqual(c3.failed_attempt_audit(out, [])["unaudited"], 2)

    def test_a_failed_attempt_without_a_ledger_is_unauditable(self):
        with tempfile.TemporaryDirectory() as d:
            out = pathlib.Path(d) / "r1"
            (out / "invocations" / "D" / "t" / "r0").mkdir(parents=True)
            (out / "invocations" / "D" / "t" / "r0" / "inv00-lift-coder.prompt.txt").write_text("p")
            with mock.patch.object(c3, "SUITE", pathlib.Path(d) / "none"):
                a = c3.failed_attempt_audit(out, [])
        self.assertGreaterEqual(a["unaudited"], 1)

    def test_complete_tolerates_a_cut_off_final_line(self):
        with tempfile.TemporaryDirectory() as d:
            (pathlib.Path(d) / "ledger.jsonl").write_text('{"kind": "registration"}\n{"kind": "generation_comp')
            self.assertFalse(c3.complete(d))
            (pathlib.Path(d) / "ledger.jsonl").write_text(
                '{"kind": "generation_complete", "aborted": false}\n{"kind": "x')
            self.assertTrue(c3.complete(d))

    def test_a_failed_attempts_evidence_reaches_the_decision(self):
        stats = {1: {"invocations": 100, "tool_calls": 0}}
        s = c3.merge_failed_attempt(summary(), stats, 1, {"invocations": 50, "tool_calls": 1, "unaudited": 1,
                                                         "hidden_name_hits": 1})
        self.assertEqual(stats[1], {"invocations": 150, "tool_calls": 1})
        self.assertEqual(len(s["hidden_name_hits"]), 1)
        t = Decide.tuples(Decide())
        self.assertEqual(c3.decide(s, t, stats, 0)["overall"], "IntegrityReview")
        clean = c3.merge_failed_attempt(summary(), {1: {"invocations": 100, "tool_calls": 0}}, 1,
                                        {"invocations": 50, "tool_calls": 0, "unaudited": 0, "hidden_name_hits": 0})
        self.assertEqual(c3.decide(clean, t, {1: {"invocations": 150, "tool_calls": 0}}, 0)["overall"], "SUPPORTED")


class Gates(unittest.TestCase):
    def test_luna_today_counts_phase_b_smoke_ledgers(self):
        # Phase-B proposal smokes sit five levels under runs/ (round/arm/attempt/smoke).
        with tempfile.TemporaryDirectory() as d:
            root, hold = pathlib.Path(d) / "root", pathlib.Path(d) / "hold"
            day = datetime.date(2026, 10, 24)
            ts = int(datetime.datetime(2026, 10, 24, 1, tzinfo=datetime.timezone.utc).timestamp() * 1000)
            rec = json.dumps({"kind": "invocation", "provider": c3.LUNA, "ts_ms": ts})
            for rel in ("c2-pc/round1/informed/attempt2/smoke", "c2-pc/round1/run"):
                p = root / "runs" / rel
                p.mkdir(parents=True)
                (p / "ledger.jsonl").write_text(rec + "\n" + rec + "\n")
                import os
                os.utime(p / "ledger.jsonl", (ts / 1000, ts / 1000))
            with mock.patch.object(c3, "ROOT", root), mock.patch.object(c3, "HOLDOUT", hold), \
                    mock.patch.object(c3, "today", lambda: day):
                self.assertEqual(c3.luna_today(), 4)

    def test_spend_day(self):
        d = datetime.date
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
    """The replicate runner: gates (deferrals write no attempt), the attempt reserved before the
    first model call, the codex bracket, failure records and the one relaunch."""
    DAY = datetime.datetime(2026, 10, 24, 1, 0, tzinfo=datetime.timezone.utc)

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        root = pathlib.Path(self.tmp.name)
        self.dir, self.runs = root / "repo-runs", root / "holdout-runs"
        self.dir.mkdir()
        (root / "a2d").write_text("binary")
        self.reg = {"tasks": TASKS, "arms": ARMS, "k": 5, "model": "codex/gpt-6-luna", "effort": "low",
                    "seeds": {"1": 9191, "2": 9292}, "claim": "X:frozen>D,X:frozen>B", "lanes": {},
                    "suite_hash": "h" * 64, "codex_version": "codex-cli 0.162.0", "codex_files": {"codex": "s"}}
        self.calls, self.fail = [], False
        fp = {"realpath": "/x/codex", "files": {"codex": "s"}, "version": "codex-cli 0.162.0"}
        patches = {"registration": lambda sha: self.reg, "registration_commit": lambda: "abc",
                   "commit_day": lambda sha: datetime.date(2026, 10, 23), "now": lambda: self.DAY,
                   "today": lambda: self.DAY.date(), "drained": lambda: True, "isolation_ok": lambda: True,
                   "codex_fingerprint": lambda: dict(fp), "luna_today": lambda: 0,
                   "probe_run": lambda model, effort, n, tag, fd=None: ([5.0] * n, False, 0, n), "DIR": self.dir,
                   "A2D": str(root / "a2d"),
                   "out_dir": lambda rep, relaunch=False: self.runs / f"r{rep}{'-relaunch1' if relaunch else ''}",
                   "C2_SEALED": root / "sealed.txt", "build_controller": lambda: c3.sha256(root / "a2d"),
                   "suite_hash_now": lambda: self.reg["suite_hash"], "crates_clean": lambda sha: True}
        (root / "sealed.txt").write_text("x\n")
        self.patchers = [mock.patch.object(c3, k, v) for k, v in patches.items()]
        self.patchers.append(mock.patch.object(c3.subprocess, "Popen", side_effect=self.fake_popen))
        for pt in self.patchers:
            pt.start()

    def tearDown(self):
        for pt in self.patchers:
            pt.stop()
        self.tmp.cleanup()

    def fake_run(self, cmd, **kw):
        self.calls.append(cmd)
        out = pathlib.Path(cmd[cmd.index("--out") + 1])
        rep = cmd[cmd.index("--experiment") + 1][-1]
        name = f"attempt-r{rep}{'-relaunch1' if 'relaunch1' in str(out) else ''}.json"
        self.assertTrue((self.dir / name).exists(), "attempt record precedes the run")
        out.mkdir(parents=True)
        recs = [{"kind": "registration"}] + ([] if self.fail else
                                             [{"kind": "generation_complete", "aborted": False, "ts_ms": 1_792_800_000_000}])
        (out / "ledger.jsonl").write_text("".join(json.dumps(r) + "\n" for r in recs))
        return mock.Mock(returncode=1 if self.fail else 0)

    def fake_popen(self, cmd, **kw):
        self.assertIn("pass_fds", kw)                     # the child inherits the runner lock
        code = self.fake_run(cmd, **kw).returncode
        return mock.Mock(pid=4242, wait=lambda: code)

    def generate(self, rep=1, relaunch=False):
        out = io.StringIO()
        with mock.patch("sys.stdout", out):
            c3.cmd_generate(argparse.Namespace(rep=rep, registered_at="abc", relaunch=relaunch, jobs=8))
        return [json.loads(x) for x in out.getvalue().splitlines() if x.strip()]

    def test_deterministic_preconditions_refuse_before_any_attempt_or_call(self):
        for name, value in (("suite_hash_now", lambda: "other"), ("crates_clean", lambda sha: False)):
            probes = []
            with mock.patch.object(c3, name, value), \
                    mock.patch.object(c3, "probe_run", lambda *a, **k: probes.append(1) or ([5.0], False, 0, 1)):
                with self.assertRaises(SystemExit):
                    self.generate(1)
            self.assertEqual((probes, self.calls), ([], []))
            self.assertFalse(list(self.dir.glob("attempt-*")))

    def test_controller_built_before_the_preflight_and_unchanged_at_launch(self):
        order = []
        real_probe = c3.probe_run
        with mock.patch.object(c3, "build_controller", lambda: order.append("build") or c3.sha256(c3.A2D)), \
                mock.patch.object(c3, "probe_run", lambda *a, **k: order.append("probe") or real_probe(*a, **k)):
            self.generate(1)
        self.assertEqual(order[0], "build")
        self.assertIn("probe", order)

    def test_a_controller_changed_after_the_preflight_stops_the_launch(self):
        real_probe = c3.probe_run
        def probe(*a, **k):
            pathlib.Path(c3.A2D).write_text("other binary")
            return real_probe(*a, **k)
        with mock.patch.object(c3, "probe_run", probe):
            with self.assertRaises(SystemExit):
                self.generate(1)
        self.assertEqual(self.calls, [])

    def test_generates_once_with_attempt_record_and_bracket(self):
        self.assertEqual(self.generate()[-1]["decision"], "generated (scoring deferred)")
        self.assertEqual(len(self.calls), 1)
        cmd = self.calls[0]
        self.assertEqual(cmd[cmd.index("--seed") + 1], "9191")
        self.assertEqual(cmd[cmd.index("--registered-at") + 1], "abc")
        self.assertIn("--defer-scoring", cmd)
        self.assertTrue(json.loads((self.dir / "bracket-r1.json").read_text())["match"])
        self.assertEqual(json.loads((self.dir / "attempt-r1.json").read_text())["registered_at"], "abc")
        self.assertEqual(json.loads((self.dir / "generation-r1.json").read_text())["pid"], 4242)
        self.assertEqual(self.generate()[-1]["decision"], "already generated")
        self.assertEqual(len(self.calls), 1)

    def test_deferrals_write_no_attempt_and_hold_for_the_day(self):
        for name, value in (("today", lambda: datetime.date(2026, 10, 23)),
                            ("now", lambda: datetime.datetime(2026, 10, 24, 0, 2, tzinfo=datetime.timezone.utc))):
            with mock.patch.object(c3, name, value):
                self.assertTrue(self.generate()[-1]["decision"].startswith("deferred"), name)
        with mock.patch.object(c3, "probe_run", lambda model, effort, n, tag, fd=None: ([90.0, 90.0, 5.0][:n], False, 0, n)):
            self.assertEqual(self.generate()[-1]["decision"], "deferred (latency)")
        # The same day stays deferred even once latency is fine (next 00:05 UTC).
        self.assertTrue(self.generate()[-1]["decision"].startswith("deferred (a no-go today"))
        self.assertFalse((self.dir / "attempt-r1.json").exists())
        self.assertEqual(self.calls, [])
        nxt = datetime.datetime(2026, 10, 25, 1, 0, tzinfo=datetime.timezone.utc)
        with mock.patch.object(c3, "now", lambda: nxt), mock.patch.object(c3, "today", lambda: nxt.date()):
            self.assertEqual(self.generate()[-1]["decision"], "generated (scoring deferred)")

    def test_canary_leak_or_tool_event_stops_the_replicate(self):
        for result in (([5.0] * 3, True, 0), ([5.0] * 3, False, 1)):
            with mock.patch.object(c3, "probe_run", lambda model, effort, n, tag, fd=None, r=result: (r[0][:n], r[1], r[2], n)), \
                    self.assertRaises(SystemExit):
                self.generate()
            # The failed preflight holds the day (a retry the same day is deferred, not run).
            self.assertTrue(self.generate()[-1]["decision"].startswith("deferred (a no-go today"))
            (self.dir / "probes.jsonl").unlink()
        self.assertFalse((self.dir / "attempt-r1.json").exists())
        self.assertEqual(self.calls, [])

    def test_a_lane_without_audited_canary_evidence_is_a_no_go(self):
        self.reg["lanes"] = {"judge": "codex/gpt-6.1-sol:medium"}
        fake = lambda model, effort, n, tag, fd=None: ([5.0] * n, False, 0, 0 if "sol" in model else n)
        with mock.patch.object(c3, "probe_run", fake):
            self.assertEqual(self.generate()[-1]["decision"], "deferred (canary evidence missing on a lane)")
        self.assertFalse((self.dir / "attempt-r1.json").exists())

    def test_an_interrupted_probe_group_holds_the_day(self):
        c3.record_probe(1, self.DAY.date(), "g1", "started")          # crashed before its go
        self.assertTrue(self.generate()[-1]["decision"].startswith("deferred (a no-go today"))

    def test_cap_deferral_holds_for_the_day(self):
        with mock.patch.object(c3, "luna_today", lambda: 2800):
            self.assertEqual(self.generate()[-1]["decision"], "deferred (daily Luna cap)")
        self.assertTrue(self.generate()[-1]["decision"].startswith("deferred (a no-go today"))

    def test_refusals(self):
        for name, value in (("drained", lambda: False), ("isolation_ok", lambda: False),
                            ("codex_fingerprint", lambda: {"realpath": "/x", "files": {"codex": "other"},
                                                           "version": "codex-cli 0.162.0"}),
                            ("codex_fingerprint", lambda: None)):
            with mock.patch.object(c3, name, value), self.assertRaises(SystemExit):
                self.generate()
        (self.dir / "attempt-r9.json").write_text(json.dumps({"registered_at": "other"}) + "\n")
        with self.assertRaises(SystemExit):
            self.generate()
        self.assertEqual(self.calls, [])

    def test_a_failed_attempt_is_never_rerun_but_may_be_relaunched_once(self):
        self.fail = True
        with self.assertRaises(SystemExit):
            self.generate()
        self.assertTrue((self.dir / "failed-r1.json").exists())
        with self.assertRaises(SystemExit):
            self.generate()                                  # never rerun
        self.fail = False
        self.assertEqual(self.generate(relaunch=True)[-1]["decision"], "generated (scoring deferred)")
        self.assertIn("r1-relaunch1", self.calls[-1][self.calls[-1].index("--out") + 1])
        self.assertTrue((self.dir / "bracket-r1-relaunch1.json").exists())
        self.assertEqual(self.generate(relaunch=True)[-1]["decision"], "already generated")
        self.assertEqual(self.generate()[-1]["decision"], "already generated")
        self.assertEqual(len(self.calls), 2)

    def test_relaunch_after_a_dead_runner_records_the_failure(self):
        (self.dir / "attempt-r1.json").write_text(json.dumps({"registered_at": "abc"}) + "\n")
        self.assertEqual(self.generate(relaunch=True)[-1]["decision"], "generated (scoring deferred)")
        self.assertIn("no runner alive", (self.dir / "failed-r1.json").read_text())

    def test_relaunch_needs_a_first_attempt(self):
        with self.assertRaises(SystemExit):
            self.generate(relaunch=True)
        self.assertEqual(self.calls, [])


if __name__ == "__main__":
    unittest.main()
