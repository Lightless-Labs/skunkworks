"""Offline tests: python3 -I -m unittest discover -s docs/experiments/analysis -p test_c2_phaseb.py -v"""
import argparse
import contextlib
import csv
import hashlib
import importlib.util
import io
import json
import pathlib
import random
import tempfile
import types
import unittest
from unittest import mock


REPO = pathlib.Path(__file__).resolve().parents[3]
spec = importlib.util.spec_from_file_location("c2_phaseb", pathlib.Path(__file__).with_name("c2_phaseb.py"))
pc = importlib.util.module_from_spec(spec)
spec.loader.exec_module(pc)
GOOD = '# lanes: judge=codex/gpt-6.1-sol:high\nimport json\nprint("ok")\n'
REPLY = ('### CANDIDATE 1: example\nHypothesis: Improve reliability.\nMechanism: Check the spec.\n'
         'Expected: +5 pp, at risk of extra tokens.\n```python\n' + GOOD + '```\n')
EVENTS = '\n'.join(json.dumps(e) for e in [
    {"type": "thread.started", "thread_id": "example"},
    {"type": "turn.started"},
    {"type": "item.started", "item": {"type": "reasoning"}},
    {"type": "item.completed", "item": {"type": "agent_message", "text": "reply"}},
    {"type": "turn.completed", "usage": {"input_tokens": 100, "output_tokens": 50}},
])


class Fixtures(unittest.TestCase):
    def setUp(self):
        # Every write, including runtime log writes, stays inside this removable
        # scratch directory. Block all subprocesses unless a test explicitly
        # provides a stub. No test can accidentally call a provider or compiler.
        self.tmp = tempfile.TemporaryDirectory(dir=REPO / "runs", prefix="phaseb-test-")
        self.addCleanup(self.tmp.cleanup)
        self.root = pathlib.Path(self.tmp.name).resolve()
        for obj, name, value in ((pc, "ROOT", self.root), (pc.ev, "ROOT", self.root),
                                 (pc.ev, "NANO", self.root / "nano"), (pc.ev, "DEV", self.root / "tasks/train-all"),
                                 (pc.ev, "A2D", str(self.root / "controller"))):
            patch = mock.patch.object(obj, name, value)
            patch.start()
            self.addCleanup(patch.stop)
        patch = mock.patch.object(pc.subprocess, "run", side_effect=AssertionError("unexpected subprocess"))
        patch.start()
        self.addCleanup(patch.stop)
        self.write("controller", "controller")
        self.write("nano/eval.py", "eval")
        self.write("nano/harness.py", GOOD)
        self.write("nano/history/before-E002b.py", "import json\n")
        self.write("nano/aa.json", json.dumps({"threshold_allpass": .1}))
        self.log([])
        self.suite = self.root / "tasks/train-all"
        for i in range(12):
            self.write(f"tasks/train-all/task-{i}/task.json", json.dumps({"family": f"family-{i // 2}"}))
            self.write(f"tasks/train-all/task-{i}/hidden_tests.rs", "SECRET_TEST_SENTINEL")
        original_read = pathlib.Path.read_text

        def forbid_test_source(path, *args, **kwargs):
            if path.name == "hidden_tests.rs":
                raise AssertionError("test source entered an offline tooling code path")
            return original_read(path, *args, **kwargs)

        patch = mock.patch.object(pathlib.Path, "read_text", new=forbid_test_source)
        patch.start()
        self.addCleanup(patch.stop)

    def write(self, path, body):
        path = self.root / path
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(body)
        return path

    def log(self, rows):
        stream = io.StringIO()
        writer = csv.DictWriter(stream, fieldnames=pc.ev.FIELDS, delimiter="\t", lineterminator="\n")
        writer.writeheader()
        writer.writerows(rows)
        self.write("nano/results.tsv", stream.getvalue())

    def run_fixture(self, name="source", arm="X:cand", results=None, branch="judge"):
        run = self.root / "runs" / name
        rows, pool = [], []
        for task, family in pc.task_families(self.suite).items():
            ok = (results or {}).get(task, False)
            rows += [{"kind": "submission", "arm": arm, "task": task, "family": family,
                      "public": {"all_passed": True}, "replicate": 0},
                     {"kind": "score", "arm": arm, "task": task, "family": family,
                      "result": {"compiled": True, "all_passed": ok, "passed": int(ok), "expected": 1}}]
            base = run / "invocations" / pc.arm_dir(arm) / task / "r0"
            self.write(base / "inv00-lift-coder.prompt.txt", f"SYSTEM:\nnot forwarded\nPROMPT:\nPublic contract case {task.split('-')[-1]}.\n#[test] fn visible() {{}}\n")
            self.write(pathlib.Path(pc.TASK_PROMPTS) / task / "r0/inv00-lift-coder.prompt.txt", f"SYSTEM:\nnot forwarded\nPROMPT:\nPublic contract case {task.split('-')[-1]}.\n#[test] fn visible() {{}}\n")
            self.write(base / "inv00-lift-coder.response.txt", "```rust\nfn answer() {}\n```\n")
            self.write(base / "harness.stderr.txt", "DIAG " + json.dumps({"task": task, "branch": branch}) + "\n")
            self.write(run / "submissions" / pc.arm_dir(arm) / task / "r0.rs", "fn answer() {}\n")
            pool.append({"run": name, "arm": pc.arm_dir(arm), "task": task, "family": family,
                         "inv": "inv00", "has_code": True, "public": {"ok": True}, "hidden": {"ok": ok}})
        self.write(run / "ledger.jsonl", "\n".join(map(json.dumps, rows)))
        return run, rows, pool


class QuartersTests(Fixtures):
    def test_deterministic_disjoint_and_covering_all_six_rounds(self):
        all_families = set(pc.ev.families(self.suite))
        for r in range(1, 7):
            actual = pc.quarter(r)
            self.assertEqual(actual, pc.quarter(r))
            groups = actual["quarters"]
            self.assertEqual(set.union(*(set(g) for g in groups)), all_families)
            self.assertEqual(sum(map(len, groups)), len(all_families))
            self.assertLessEqual(max(map(len, groups)) - min(map(len, groups)), 1)
            names = sorted(all_families)
            random.Random(f"A2D-C2-PC-{r}").shuffle(names)
            self.assertEqual(set(groups[0]), set(names[:2]))
            self.assertEqual(actual["evaluation_families"], groups[(r - 1) % 4])

    def test_quarters_cli_artifact_is_immutable(self):
        with contextlib.redirect_stdout(io.StringIO()):
            pc.cmd_quarters(argparse.Namespace(round=1))
        self.assertEqual(pc.read_json(pc.round_dir(1) / "quarter.json"), pc.quarter(1))


class DossierTests(Fixtures):
    def test_excludes_evaluation_families_dedupes_and_carries_only_label(self):
        run, rows, pool = self.run_fixture()
        excluded = set(pc.quarter(1)["evaluation_families"])
        families = pc.task_families(self.suite)
        # Missing files in excluded families prove that they are never opened.
        for task, family in families.items():
            if family in excluded:
                (self.root / pc.TASK_PROMPTS / task / "r0/inv00-lift-coder.prompt.txt").unlink()
        first = pc.dossiers([(run, self.suite)] * 2, "X:cand", 1)
        self.assertEqual(first, pc.dossiers([(run, self.suite)], "X:cand", 1))
        self.assertEqual(len(first), min(8, sum(f not in excluded for f in families.values())))
        for item in first:
            self.assertEqual(set(item), {"requirements_and_public_tests", "submitted_file", "label"})
            self.assertEqual(item["label"], "fails at least one hidden test")
        encoded = json.dumps(first)
        self.assertNotIn("SYSTEM", encoded)
        self.assertNotIn("task-", encoded)
        self.assertNotIn("family-", encoded)
        self.assertNotIn("hidden_tests", encoded)
        self.assertNotIn("SECRET_TEST_SENTINEL", encoded)
        ordered = sorted((t for t, f in families.items() if f not in excluded),
                         key=lambda t: hashlib.sha256(f"A2D-C2-PC-dossier-1-{t}".encode()).hexdigest())[:8]
        self.assertEqual([item["requirements_and_public_tests"].splitlines()[0] for item in first],
                         [f"Public contract case {t.split('-')[-1]}." for t in ordered])

    def test_public_red_or_hidden_pass_is_not_a_dossier(self):
        run, rows, _ = self.run_fixture()
        for row in rows:
            if row["kind"] == "submission":
                row["public"]["all_passed"] = False
        self.write(run / "ledger.jsonl", "\n".join(map(json.dumps, rows)))
        self.assertEqual(pc.dossiers([(run, self.suite)], "X:cand", 1), [])


class BriefTests(Fixtures):
    def test_information_is_the_only_template_difference(self):
        self.log([{"id": "old", "proposer": "claude", "stage": "full", "hypothesis": "pre-phase result"},
                  {"id": "PC-1", "proposer": "pc-informed", "stage": "round", "hypothesis": "own-result"},
                  {"id": "PC-1", "proposer": "pc-uninformed", "stage": "round", "hypothesis": "other-result"},
                  {"id": "PC-2", "proposer": "pc-informed", "stage": "round", "hypothesis": "future-result"}])
        pc.write_json(pc.round_dir(1) / "uninformed/candidate.json", {"slug": "own-title", "hypothesis": "own-hypothesis", "secret": "no-outcome"})
        stats, dossiers = {"n_task_runs": 12}, []
        inf = pc.brief("informed", 2, stats, dossiers)
        uninf = pc.brief("uninformed", 2)
        info = pc.information("informed", 2, stats, dossiers)
        uinfo = pc.information("uninformed", 2)
        self.assertEqual(inf.replace(info, "INFO"), uninf.replace(uinfo, "INFO"))
        self.assertTrue(inf.startswith(pc.ARCHITECT + "\n"))
        self.assertIn("pre-phase result", inf)
        self.assertIn("own-result", inf)
        for forbidden in ("pc-uninformed", "other-result", "future-result"):
            self.assertNotIn(forbidden, inf)
        self.assertIn("own-title", uninf)
        self.assertNotIn("no-outcome", uninf)
        self.assertNotIn("own-result", uninf)
        for text in (inf, uninf):
            for forbidden in ("holdout", "seeds.md", "phase b", "proposer comparison"):
                self.assertNotIn(forbidden, text.lower())
            self.assertIn("EXACTLY ONE", text)
            self.assertIn("gpt-6-luna at low effort", text)

    def test_forbidden_context_assertion(self):
        with self.assertRaises(AssertionError):
            pc.brief("informed", 1, {"forbidden": "HOLDOUT"}, [])


class AuditAndProposalTests(Fixtures):
    def test_real_call_boundary_is_isolated_and_injectable(self):
        (self.root / "runs").mkdir()

        def fake_run(cmd, **kwargs):
            self.assertEqual(cmd[:2], ["codex", "exec"])
            self.assertIn("--json", cmd)
            self.assertIn('model_reasoning_effort="high"', cmd)
            self.assertEqual(cmd[cmd.index("-m") + 1], "gpt-6.1-sol")
            self.assertEqual(kwargs["stdin"], pc.subprocess.DEVNULL)
            self.assertEqual(list(pathlib.Path(kwargs["cwd"]).iterdir()), [])
            for feature in pc.proposer.DISABLED:
                self.assertTrue(any(cmd[i:i + 2] == ["--disable", feature] for i in range(len(cmd) - 1)))
            # codex-cli 0.162 ignores the deprecated feature flag (web search is on by
            # default) and emits an `error` item that voids every call; the config key works.
            self.assertTrue(any(cmd[i:i + 2] == ["-c", 'web_search="disabled"'] for i in range(len(cmd) - 1)))
            self.assertNotIn("web_search_request", cmd)
            pathlib.Path(cmd[cmd.index("-o") + 1]).write_text(REPLY)
            return types.SimpleNamespace(returncode=0, stdout=EVENTS, stderr="")

        with mock.patch.object(pc.subprocess, "run", side_effect=fake_run):
            result = pc.call_model("synthetic prompt")
        self.assertEqual(result["reply"], REPLY)
        self.assertFalse(pc.audit_events(result["events"])["void"])
        self.assertEqual(list((self.root / "runs").iterdir()), [])

    def test_event_audit_all_lifecycles_and_usage(self):
        result = pc.audit_events(EVENTS)
        self.assertFalse(result["void"])
        self.assertEqual(result["usage"], [{"input_tokens": 100, "output_tokens": 50}])
        for item_type in ("command_execution", "mcp_tool_call", "web_search", "file_change"):
            for event_type in ("item.started", "item.updated", "item.completed"):
                result = pc.audit_events(EVENTS + "\n" + json.dumps({"type": event_type, "item": {"type": item_type}}))
                self.assertTrue(result["void"])
                self.assertEqual(result["offending_types"], [item_type])
        self.assertTrue(pc.audit_events("not json")["void"])
        self.assertTrue(pc.audit_events("")["void"])

    def test_guard_lanes_and_compile_without_executing(self):
        path = self.write("candidate.py", GOOD)
        self.assertEqual(pc.validate_candidate(path), (True, ""))
        for code in ("# lanes: coder=codex/gpt-6-luna:low\n", "import os\n", "open('secret')\n",
                     "# lanes: judge=codex/gpt-6-luna:low\n# lanes: judge=codex/gpt-6.1-sol:high\n",
                     "this is not python!", "# lanes:\n"):
            path.write_text(code)
            self.assertFalse(pc.validate_candidate(path)[0], code)

    def test_one_retry_with_error_and_usage_for_both_attempts(self):
        responses = [{"returncode": 0, "reply": REPLY,
                      "events": EVENTS + '\n{"type":"item.started","item":{"type":"command_execution"}}'},
                     {"returncode": 0, "reply": REPLY, "events": EVENTS}]
        model = mock.Mock(side_effect=responses)
        smoke = mock.Mock()
        result = pc.propose("uninformed", 1, model_call=model, smoke=smoke)
        self.assertTrue(result["valid"])
        self.assertEqual(model.call_count, 2)
        self.assertIn("command_execution", model.call_args.args[0])
        smoke.assert_called_once()
        summary = pc.read_json(pc.round_dir(1) / "uninformed/events-summary.json")
        self.assertEqual(len(summary["attempts"]), 2)
        self.assertTrue(summary["attempts"][0]["void"])
        self.assertEqual(summary["attempts"][1]["usage"][0]["output_tokens"], 50)
        # Completed proposal is immutable/cached and never calls a model again.
        self.assertEqual(pc.propose("uninformed", 1, model_call=model, smoke=smoke), result)
        self.assertEqual(model.call_count, 2)

    def test_still_invalid_recorded_after_exactly_two_calls(self):
        for failure in ("api", "parse", "guard"):
            r = {"api": 1, "parse": 2, "guard": 3}[failure]
            response = {"returncode": int(failure == "api"), "events": EVENTS,
                        "reply": "no candidate" if failure == "parse" else REPLY.replace("import json", "import os")}
            model = mock.Mock(return_value=response)
            result = pc.propose("uninformed", r, model_call=model, smoke=mock.Mock())
            self.assertFalse(result["valid"])
            self.assertTrue(result["reason"])
            self.assertEqual(model.call_count, 2)

    def test_default_lane_authorship_only(self):
        run, _, _ = self.run_fixture()
        tasks = pc.task_families(self.suite)
        self.assertEqual(pc.authorship(run, "X:cand", tasks), [])
        task = min(tasks)
        base = pc.tuple_dir(run, "X:cand", task)
        (base / "inv00-lift-coder.response.txt").rename(base / "inv00-lift-lane-judge.response.txt")
        self.assertEqual(pc.authorship(run, "X:cand", tasks), [task])


class StatsAndRoundTests(Fixtures):
    def test_failed_round_retries_once_with_new_seed_same_proposals(self):
        base = pc.round_dir(1)
        base.mkdir(parents=True)
        pc.write_json(base / "manifest.json", {"frozen": True})
        sources = {"inc": self.root / "nano/harness.py"}
        calls = []

        def runner(exp, suite, arms, externals, lanes, out, seed, jobs):
            calls.append((externals, seed))
            if len(calls) == 1:
                raise SystemExit("infrastructure failed")
            pc.write_json(out / "summary.json", {"complete": True})

        result = pc.run_round(base, self.suite, sources, {}, 23, 6, runner, lambda *args: True)
        self.assertEqual(result.name, "run-retry")
        self.assertEqual(calls, [(sources, 23), (sources, 24)])
        self.assertEqual(pc.run_round(base, self.suite, sources, {}, 23, 6, runner, lambda *args: True), result)
        self.assertEqual(len(calls), 2)

    def test_a_web_search_in_any_transcript_makes_the_run_integrity_invalid(self):
        # The frozen provider's tool count sees only `exec`; codex-cli 0.162 can web-search.
        base = pc.round_dir(1)
        base.mkdir(parents=True)
        pc.write_json(base / "manifest.json", {"frozen": True})
        sources = {"inc": self.root / "nano/harness.py"}
        seeds = []

        def runner(exp, suite, arms, externals, lanes, out, seed, jobs):
            seeds.append(seed)
            tuple_dir = out / "invocations/X-inf/t/r0"
            tuple_dir.mkdir(parents=True)
            line = "web search: rust semver\n" if len(seeds) == 1 else "exec\n"
            (tuple_dir / "inv00-lift-coder.transcript.txt").write_text("codex\n" + line)
            pc.write_json(out / "summary.json", {"complete": True, "integrity_ok": True})

        result = pc.run_round(base, self.suite, sources, {}, 23, 6, runner, lambda *args: True)
        self.assertEqual(result.name, "run-retry")
        self.assertEqual(seeds, [23, 24])
        self.assertEqual(pc.read_json(base / "run-attempts.json")["failures"][0]["reason"],
                         "aborted or integrity-invalid run")
        self.assertEqual(pc.run_round(base, self.suite, sources, {}, 23, 6, runner, lambda *args: True), result)
        self.assertEqual(len(seeds), 2)

    def test_lane_conflicts_preserve_efforts_and_default_lane(self):
        # The synthetic harness only emits protocol lines; no model is called.
        body = ('import sys\n'
                'sys.stdout.write(\'{"op":"model","lane":"judge"}\')\n'
                'sys.stdout.write("\\n")\n'
                'print(\'{"op":"model","prompt":"default lane"}\', flush=True)\n')
        inc = self.write("inc.py", "# lanes: judge=codex/gpt-6.1-sol:medium\n" + body)
        cand = self.write("cand.py", "# lanes: judge=codex/gpt-6.1-sol:high\n" + body)
        externals, lanes = pc.bind_lanes({"inc": inc, "inf": cand}, self.root)
        self.assertEqual(lanes, {"pc-inc-judge": "codex/gpt-6.1-sol:medium", "pc-inf-judge": "codex/gpt-6.1-sol:high"})
        for name, path in externals.items():
            self.assertTrue(pc.validate_candidate(path)[0])
            output = io.StringIO()
            with contextlib.redirect_stdout(output):
                exec(path.read_text(), {"__name__": "synthetic_fixture"})
            lines = [json.loads(line) for line in output.getvalue().splitlines()]
            self.assertEqual(lines, [{"op": "model", "lane": f"pc-{name}-judge"},
                                     {"op": "model", "prompt": "default lane"}])

    def test_stats_exact_subset_curve_and_failure_buckets(self):
        run, _, pool = self.run_fixture()
        # Two public-green samples, exactly one hidden passer, on every task.
        for sample in list(pool):
            pool.append({**sample, "inv": "inv01", "hidden": {"ok": True}})
        result = pc.stats([(run, self.suite)], "X:cand", pool)
        self.assertEqual(result["n_task_runs"], 12)
        self.assertEqual(result["all_pass_rate"], 0)
        self.assertEqual(result["failure_buckets"], {"judge missed a hidden-passing green": 12})
        self.assertEqual(result["any_pass_at_k"]["1"]["hidden_pass"], .5)
        self.assertEqual(result["any_pass_at_k"]["2"]["hidden_pass"], 1)
        self.assertIsNone(result["any_pass_at_k"]["3"]["hidden_pass"])
        self.assertEqual(result["per_sample_hidden_allpass_rate"], .5)
        self.assertNotIn("task-", json.dumps(result))
        self.assertNotIn("family-", json.dumps(result))

    def test_missing_scores_and_voids_count_as_failures(self):
        run, rows, _ = self.run_fixture(results={"task-0": True, "task-1": True})
        rows = [row for row in rows if not (row["kind"] == "score" and row["task"] == "task-0")]
        rows[0]["void_reason"] = "crash"
        self.write(run / "ledger.jsonl", "\n".join(map(json.dumps, rows)))
        table, issues = pc.score_table(run, self.suite, ["X:cand", "X:missing"])
        self.assertEqual(len(table), 24)
        self.assertEqual(len(issues["missing_scores"]), 13)
        self.assertEqual(table["X:cand", "task-0"][:2], (0, 0))

    def test_post_run_authorship_invalidity_survives_reopening(self):
        base = pc.round_dir(1)
        for arm in pc.ARM_NAMES:
            self.write(base / arm / "candidate.py", GOOD)
            self.write(base / arm / "PROMPT.md", "prompt")
            pc.write_json(base / arm / "candidate.json", {"valid": True, "candidate_sha": pc.ev.sha(base / arm / "candidate.py")})
        expected = {t for f, tasks in pc.ev.families(self.suite).items()
                    if f in pc.quarter(1)["evaluation_families"] for t in tasks}
        all_rows = []
        for arm in pc.ARMS:
            run, rows, _ = self.run_fixture(name="c2-pc/round1/run", arm=arm)
            all_rows.extend(row for row in rows if row["task"] in expected)
        self.write(run / "ledger.jsonl", "\n".join(map(json.dumps, all_rows)))
        self.write(run / "summary.json", "{}")
        self.write(pc.submission_path(run, "X:inf", min(expected)), "fn written_by_sol() {}")
        with mock.patch.object(pc.ev, "git_head", return_value="head"), mock.patch.object(pc.ev, "suite_hash", return_value="suitehash"):
            with contextlib.redirect_stdout(io.StringIO()):
                pc.cmd_round(argparse.Namespace(round=1, jobs=6))
                pc.cmd_round(argparse.Namespace(round=1, jobs=6))
        proposal = pc.read_json(base / "informed/candidate.json")
        self.assertFalse(proposal["valid"])
        self.assertTrue(proposal["preflight_valid"])
        result = pc.read_json(base / "scores.json")
        self.assertEqual(result["decisions"]["inf"]["delta_allpass"], 0)
        self.assertEqual(result["decisions"]["inf"]["decision"], "invalid")

    def test_three_arm_round_invalid_substitution_and_cached_idempotence(self):
        r = 1
        base = pc.round_dir(r)
        for arm in pc.ARM_NAMES:
            self.write(base / arm / "candidate.py", GOOD)
            self.write(base / arm / "PROMPT.md", "prompt")
            pc.write_json(base / arm / "candidate.json", {"valid": arm == "informed", "candidate_sha": pc.ev.sha(base / arm / "candidate.py")})

        def lift(exp, suite, arms, externals, lanes, out, seed, jobs):
            self.assertEqual(arms, list(pc.ARMS))
            self.assertEqual(externals["uninf"], pc.phase_dir() / "incumbent.py")
            rows = []
            for task, family in pc.task_families(suite).items():
                for arm in arms:
                    # Replacement's random outcomes intentionally differ from
                    # the incumbent; normalized delta must still be zero.
                    ok = arm != "X:inc"
                    rows.append({"kind": "score", "arm": arm, "task": task, "family": family,
                                 "result": {"compiled": True, "passed": int(ok), "expected": 1, "all_passed": ok}})
            self.write(out / "ledger.jsonl", "\n".join(map(json.dumps, rows)))
            pc.write_json(out / "summary.json", {})

        runner = mock.Mock(side_effect=lift)
        cap = mock.Mock(return_value=True)
        with mock.patch.object(pc.ev, "git_head", return_value="head"), mock.patch.object(pc.ev, "suite_hash", return_value="suitehash"):
            with contextlib.redirect_stdout(io.StringIO()):
                pc.cmd_round(argparse.Namespace(round=r, jobs=6), lift=runner, cap=cap)
                pc.cmd_round(argparse.Namespace(round=r, jobs=6), lift=runner, cap=cap)
        runner.assert_called_once()
        self.assertEqual(cap.call_args.args[1], 15 * len(pc.task_families(base / "suite")))
        result = pc.read_json(base / "scores.json")
        self.assertEqual(result["decisions"]["uninf"]["delta_allpass"], 0)
        self.assertEqual(result["decisions"]["uninf"]["decision"], "invalid")
        self.assertEqual(result["decisions"]["inf"]["decision"], "screen-pass")
        logged = list(csv.DictReader(io.StringIO((self.root / "nano/results.tsv").read_text()), delimiter="\t"))
        self.assertEqual(len(logged), 2)


class AnalyzeTests(Fixtures):
    def test_pooled_difference_sign_test_and_invalid_denominator(self):
        rounds = []
        # Round 1 weighs twice as much: pooled .75, round-mean 2/3.
        for r, n, gain in ((1, 4, 1), (2, 2, 1), (3, 2, 0)):
            scores = []
            for i in range(n):
                for arm, ok in (("X:inc", 0), ("X:inf", gain), ("X:uninf", 0)):
                    scores.append({"arm": arm, "task": f"t{i}", "family": "repeated-family",
                                   "fraction": ok * .8, "all_passed": ok})
            rounds.append({"round": r, "scores": scores,
                           "decisions": {"inf": {"valid": r != 3}, "uninf": {"valid": True}}})
        result = pc.analyze(rounds, .1)
        self.assertEqual(result["allpass"]["difference"], .75)
        self.assertAlmostEqual(result["hidden_fraction"]["difference"], .6)
        self.assertEqual(result["allpass"]["sign_test"], {"positive": 2, "negative": 0, "ties": 1, "p_two_sided": .5})
        self.assertEqual(result["arms"]["inf"]["invalid_count"], 1)
        self.assertEqual(result["arms"]["inf"]["count_above_aa_threshold"], 2)
        self.assertEqual(result["arms"]["inf"]["screen_pass_rate"], 2 / 3)
        self.assertEqual(result, pc.analyze(rounds, .1))
        self.assertEqual(pc.sign_test([1] * 6)["p_two_sided"], .03125)
        self.assertEqual(pc.sign_test([0] * 6)["p_two_sided"], 1)
        self.assertIsNone(pc.analyze([], .1)["allpass"]["difference"])


if __name__ == "__main__":
    unittest.main()
