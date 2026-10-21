"""Tests for the gym v3 pipeline, on fixtures with a fake codex and a fake `a2d lift run`.

Every script runs as a subprocess with GYM_ROOT pointing at a fresh temp dir (TESTING mode:
non-default root + GYM_TEST_SKIP_PREREQS, confined to GYM_TEST_ROOT = the temp dir, with the fake
codex and a fixture auth file), a fake confirmation-3 holdout view under the temp dir
(GYM_TEST_HOLDOUT) and a scratch git repo for repo outputs (GYM_TEST_OUT_REPO), under the real
seatbelt profiles. No model is ever called. Gate (b) of the launch gate always runs against the
real repo. One test reads the real ~/.a2d-holdout, only as allowed: a digest of
authoring/confirmation-3/_records and `c3_author.py --drained`, before and after the full flow
(nothing from there is printed; it is skipped when ~/.a2d-holdout does not exist).

usage: python3 -I -m unittest discover -s tools/gym/tests -v
"""
import ast
import hashlib
import json
import os
import re
import shutil
import stat
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
TOOLS = HERE.parent
A2D = TOOLS.parents[1]
FAKE = HERE / "fake_codex.sh"
FAKE_A2D = HERE / "fake_a2d.py"
FIX = HERE / "fixture"
SECRETS = ("SECRET", "secret-family-name", "Brief ", "hosts")
REAL_HOLDOUT = Path.home() / ".a2d-holdout"
GIT_ID = ["-c", "user.name=gym-test", "-c", "user.email=gym-test@example.invalid", "-c", "commit.gpgsign=false"]
DRIVERS = ("gym_select.py", "gym_author.py", "gym_assemble.py", "gym_similarity.py", "gym_validate.py",
           "gym_fix.py", "gym_audit.py", "gym_adjudicate.py", "gym_pilot.py", "gym_split.py", "gym_band.py",
           "gym_export.py")


def clean_env():
    """The parent environment without any holdout, confirmation-3 or gym variable."""
    return {k: v for k, v in os.environ.items()
            if k != "A2D_HOLDOUT_DIR" and not k.startswith("C3_") and not k.startswith("GYM_")}


def git(repo, *args):
    return subprocess.run(["git", "-C", str(repo), *GIT_ID, *args], capture_output=True, text=True, check=True)


class Base(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="gymtest-")).resolve()
        self.root = self.tmp / "gym"
        self.root.mkdir()
        self.holdout = self.tmp / "holdout"
        for d in ("confirmation-3", "authoring/confirmation-3/_records", "runs/c2-confirm2-decision",
                  "runs/c2-confirm3-r2"):
            (self.holdout / d).mkdir(parents=True)
        self.ledger = self.holdout / "runs" / "c2-confirm3-r2" / "ledger.jsonl"
        self.ledger.write_text('{"kind": "registration"}\n{"kind": "generation_complete", "aborted": false}\n')
        self.repo = self.tmp / "repo"
        (self.repo / "runs").mkdir(parents=True)
        (self.repo / "README").write_text("scratch\n")
        subprocess.run(["git", "init", "-q", str(self.repo)], check=True)
        git(self.repo, "add", "README")
        git(self.repo, "commit", "-q", "-m", "init")
        (self.tmp / "auth.json").write_text("{}")
        self.canary = self.tmp / "canary.txt"
        sha = hashlib.sha256(FAKE.read_bytes()).hexdigest()[:16]
        self.env = dict(clean_env(), GYM_ROOT=str(self.root), GYM_TEST_SKIP_PREREQS="1", GYM_TEST_ROOT=str(self.tmp),
                        GYM_CODEX=str(FAKE),
                        GYM_AUTH=str(self.tmp / "auth.json"), GYM_CANARY=str(self.canary), GYM_PINNED_SHA=sha,
                        GYM_TEST_HOLDOUT=str(self.holdout), GYM_TEST_OUT_REPO=str(self.repo),
                        GYM_TEST_QUOTA="1", GYM_TEST_SPARES="1")
        self.gym = self.root / "authoring" / "gym-v3"
        self.rec = self.gym / "_records"

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def run_script(self, name, *args, env=None, timeout=900):
        r = subprocess.run([sys.executable, "-I", str(TOOLS / name), *args], capture_output=True, text=True,
                           env=env or self.env, timeout=timeout)
        for x in (r.stdout + r.stderr).splitlines():
            for s in SECRETS:
                self.assertNotIn(s, x, f"{name} leaked content")
        self.assertEqual(r.stderr.strip(), "", f"{name} wrote to stderr")
        return r.returncode, [json.loads(x) for x in r.stdout.splitlines() if x.strip()]

    def lib(self, code, timeout=600, env=None):
        r = subprocess.run([sys.executable, "-I", "-c", f"import sys; sys.path.insert(0, {str(TOOLS)!r})\n{code}"],
                           capture_output=True, text=True, env=env or self.env, timeout=timeout)
        self.assertEqual(r.returncode, 0, r.stderr[-3000:])
        return r.stdout.strip()

    def active(self):
        return json.loads((self.rec / "active.json").read_text())

    def statuses(self, lines, stage=None):
        return [x.get("status") for x in lines if stage is None or x.get("stage") == stage]

    # ------------------------------------------------------------------ flow helpers

    def select_and_split(self, seed=7, holdout=1, wave1=3):
        code, lines = self.run_script("gym_select.py")
        self.assertEqual(code, 0, lines)
        code, lines = self.run_script("gym_split.py", "register", "--seed", str(seed), "--holdout", str(holdout),
                                      "--wave1", str(wave1))
        self.assertEqual(code, 0, lines)
        git(self.repo, "add", "runs/c2-gym3/split.json")
        git(self.repo, "commit", "-q", "-m", "split")
        return json.loads((self.repo / "runs" / "c2-gym3" / "split.json").read_text())

    def settle(self):
        self.assertEqual(self.run_script("gym_assemble.py")[0], 0)
        code, lines = self.run_script("gym_validate.py", "--jobs", "4")
        self.assertEqual(code, 0, lines)
        code, lines = self.run_script("gym_similarity.py")
        self.assertEqual(code, 0, lines)
        return lines

    def audit(self, **hooks):
        return self.run_script("gym_audit.py", "--binary", str(FAKE_A2D), env=dict(self.env, **hooks))

    def flow(self):
        """The full fake-codex gym flow, up to a certified, exported, piloted and accepted suite.
        Every dev family is in wave 1, so the acceptance replicate may run once wave 1 is decided."""
        sp = self.select_and_split(wave1=4)
        fams = (self.gym / "FAMILIES.txt").read_text().split()
        self.canary.write_text("CANARY-CONTENT\n")
        code, lines = self.run_script("gym_author.py", "--only", fams[0], env=dict(self.env, GYM_TEST_PROBE=str(self.canary)))
        self.assertEqual(code, 0, lines)
        code, lines = self.run_script("gym_author.py", "--parallel", "3")
        self.assertEqual((code, lines[-1]["done"]), (0, 5), lines)
        self.settle()
        f1, f2 = fams[0], fams[1]
        code, lines = self.audit(GYM_TEST_AUDIT_FAIL=f"{f1}-l2", GYM_TEST_AUDIT_VOID=f"{f2}-l3")
        self.assertEqual(code, 0, lines)
        code, lines = self.run_script("gym_adjudicate.py")
        self.assertEqual(code, 0, lines)
        code, lines = self.audit()
        self.assertEqual((code, lines[0].get("tasks")), (0, 1), lines)      # only the void task
        code, lines = self.run_script("gym_adjudicate.py")
        self.assertEqual(code, 0, lines)
        code, lines = self.run_script("gym_export.py", "certify")
        self.assertEqual(code, 0, lines)
        code, lines = self.run_script("gym_export.py", "export")
        self.assertEqual(code, 0, lines)
        w1 = sorted(f for f, a in sp["assign"].items() if a["wave"] == 1)
        fail = [f"{w1[0]}-l1", f"{w1[0]}-l2"] + [f"{w1[2]}-l{i}" for i in (1, 2, 3, 4)]
        code, lines = self.run_script("gym_pilot.py", "--wave", "1", "--binary", str(FAKE_A2D),
                                      env=dict(self.env, GYM_TEST_PILOT_FAIL=" ".join(fail)))
        self.assertEqual(code, 0, lines)
        code, lines = self.run_script("gym_band.py", "band", "--wave", "1", "--run", "runs/c2-gym3-pilot-w1/r1")
        self.assertEqual(code, 0, lines)
        code, lines = self.run_script("gym_pilot.py", "--acceptance", "--binary", str(FAKE_A2D))
        self.assertEqual(code, 0, lines)
        code, lines = self.run_script("gym_band.py", "accept", "--run", "runs/c2-gym3-accept/r1")
        self.assertEqual(code, 0, lines)
        return sp, fams, w1


class Units(Base):
    """Fast checks of the pure parts and of the static isolation rules."""

    def test_power_table(self):
        out = self.lib("import power\nfor r in power.table(240, [1, 2], 0.3, [0.05, 0.075, 0.10]):\n"
                       "    print(r['se_pp'], r['power'])\n"
                       "print(power.table(100, [2], 0.3, [0.10])[0]['se_pp'])")
        self.assertEqual(out.splitlines(), ["4.4 [0.3, 0.52, 0.73]", "3.1 [0.48, 0.77, 0.94]", "4.8"])

    def test_vendored_functions_match_confirm3_source(self):
        """Parity by AST: confirm3.py is parsed, never imported or executed."""
        def defs(p):
            t = ast.parse(Path(p).read_text())
            f = {n.name: ast.dump(n) for n in t.body if isinstance(n, ast.FunctionDef)}
            c = {n.targets[0].id: ast.dump(n.value) for n in t.body if isinstance(n, ast.Assign)
                 and len(n.targets) == 1 and isinstance(n.targets[0], ast.Name)}
            return f, c
        src_f, src_c = defs(A2D / "nano" / "confirm3.py")
        ven_f, ven_c = defs(TOOLS / "gymvendor.py")
        names = self.lib("import gymvendor\nprint(' '.join(gymvendor.VENDORED))\n"
                         "print(' '.join(gymvendor.VENDORED_CONSTANTS))").splitlines()
        for n in names[0].split():
            self.assertEqual(ven_f.get(n), src_f.get(n), n)
        for n in names[1].split():
            self.assertEqual(ven_c.get(n), src_c.get(n), n)

    def test_vendored_behaviour_on_fixtures(self):
        notice = ("Code Mode is unavailable because code-mode host is disabled. Code mode will fail closed; "
                  "enable `features.code_mode_host` and install `codex-code-mode-host`.")
        good = "\n".join(json.dumps(e) for e in (
            {"type": "item.completed", "item": {"type": "error", "message": notice}}, {"type": "turn.started"},
            {"type": "item.completed", "item": {"type": "agent_message"}}, {"type": "turn.completed"})) \
            + "\n--- codex stderr ---\nx"
        tool = good.replace('"agent_message"', '"command_execution"')
        no_end = good.replace('{"type": "turn.completed"}', "")
        (self.tmp / "t.json").write_text(json.dumps({"good": good, "tool": tool, "no_end": no_end}))
        out = self.lib(
            "import json, gymvendor as v\n"
            f"t = json.load(open({str(self.tmp / 't.json')!r}))\n"
            "print(v.py_tool_events(t['good']), v.py_tool_events(t['tool']), v.py_tool_events(t['no_end']),"
            " v.py_tool_events('plain text'), v.host_off(t['good']), v.host_off(t['tool'].replace('error', 'x')))\n"
            f"print(len(v.test_names(open({str(FIX / 'hidden_tests.rs')!r}).read())))\n"
            "print(v.lines('a\\r\\nb\\u2028c\\n'))")
        self.assertEqual(out.splitlines(), ["0 1 1 1 True False", "26", "['a', 'b\\u2028c']"])

    def test_gate_a_ledger_kinds_only(self):
        """Gate (a) mirrors confirm3.complete: only a NON-ABORTED generation_complete counts (an
        aborted replicate 2 may still be relaunched), in the first attempt's or the relaunch's
        ledger; the window closes again once confirmation-3's manifests exist (its scoring is
        due); decision.json opens it for good."""
        led = self.tmp / "l.jsonl"
        cases = [("", False), ('{"kind": "registration"}\n', False), ("not json\n", False),
                 ('{"kind": "generation_complete", "aborted": true}\n', False),
                 ('garbage\n{"kind": "generation_complete"}\n', False),
                 ('{"kind": "generation_complete", "aborted": 0}\n', False),
                 ('garbage\n{"kind": "generation_complete", "aborted": false}\n', True),
                 ('{"kind": "generation_complete", "aborted": false}\u2028x\n', False),
                 ('{"kind": NaN}\n', False), ('["generation_complete"]\n', False)]
        for text, want in cases:
            led.write_text(text)
            self.assertEqual(self.lib(f"from gymvendor import generation_complete_in\n"
                                      f"print(generation_complete_in({str(led)!r}))"), str(want), text)
        self.assertEqual(self.lib(f"from gymvendor import generation_complete_in\n"
                                  f"print(generation_complete_in({str(self.tmp / 'missing')!r}))"), "False")
        window = "import gymlib\nprint(gymlib.c3_window_open())"
        # the gym's view: the fake replicate-2 ledger (non-aborted) opens the window
        self.assertEqual(self.lib(window), "True")
        self.ledger.write_text('{"kind": "registration"}\n{"kind": "generation_complete", "aborted": true}\n')
        self.assertEqual(self.lib(window), "False")                    # aborted: a relaunch may follow
        relaunch = self.holdout / "runs" / "c2-confirm3-r2-relaunch1"
        relaunch.mkdir(parents=True)
        (relaunch / "ledger.jsonl").write_text('{"kind": "generation_complete", "aborted": false}\n')
        self.assertEqual(self.lib(window), "True")                     # the relaunch completed
        c3 = self.repo / "runs" / "c2-confirm3"
        c3.mkdir(parents=True)
        (c3 / "manifest-r1.json").write_text("{}")
        self.assertEqual(self.lib(window), "False")                    # scoring is due: closed again
        (c3 / "decision.json").write_text("{}")
        self.assertEqual(self.lib(window), "True")                     # decided
        (relaunch / "ledger.jsonl").unlink()
        self.assertEqual(self.lib(window), "True")
        self.assertEqual(self.lib("import gymlib\nprint(gymlib.GATE_PATHS)"),
                         "('tools/holdout', 'nano/confirm3.py', 'nano/eval.py', 'crates', 'Cargo.toml', 'Cargo.lock', "
                         "'runs/c2-confirm3/registration.json', 'runs/c2-confirm3/frozen.py')")

    def test_launch_rechecks_gate_per_job(self):
        """launch() evaluates gate (a) on every job: a manifest written between two jobs refuses the next."""
        code = ("from gymlib import LaunchGateClosed, launch, RECORDS\nimport subprocess\n"
                "RECORDS.mkdir(parents=True, exist_ok=True)\n"
                "p = launch('/dev/null', ['/usr/bin/true'], stdin=subprocess.DEVNULL)\np.wait()\nprint('FIRST')\n"
                f"open({str(self.repo / 'runs' / 'm')!r}, 'w').close()\n"
                f"import os; os.makedirs({str(self.repo / 'runs' / 'c2-confirm3')!r}, exist_ok=True)\n"
                f"open({str(self.repo / 'runs' / 'c2-confirm3' / 'manifest-r2.json')!r}, 'w').close()\n"
                "try:\n    launch('/dev/null', ['/usr/bin/true'])\n    print('LAUNCHED')\n"
                "except LaunchGateClosed:\n    print('REFUSED')")
        self.assertEqual(self.lib(code).splitlines(), ["FIRST", "REFUSED"])

    def test_testing_mode_is_confined(self):
        """Test mode cannot reach a real codex, real auth or real state: each unconfined setting
        refuses at import (before any driver runs)."""
        def refused(**over):
            env = dict(self.env, **over)
            for k in [k for k, v in over.items() if v is None]:
                env.pop(k)
            r = subprocess.run([sys.executable, "-I", "-c", f"import sys; sys.path.insert(0, {str(TOOLS)!r}); import gymlib"],
                               capture_output=True, text=True, env=env)
            return r.returncode != 0 and "RuntimeError" in r.stderr
        self.assertFalse(refused())                                            # the fixture env is confined
        macho = self.tmp / "native-codex"
        shutil.copy("/bin/echo", macho)
        pin = Path.home() / ".a2d-codex-pin" / "0.162.0" / "bin" / "codex"
        elsewhere = Path(tempfile.mkdtemp(prefix="gymtest-outside-")).resolve()
        self.addCleanup(shutil.rmtree, elsewhere, True)
        (elsewhere / "auth.json").write_text("{}")
        cases = {
            "no test root": dict(GYM_TEST_ROOT=None),
            "test root is HOME": dict(GYM_TEST_ROOT=str(Path.home())),
            "gym root outside the test root": dict(GYM_ROOT=str(elsewhere / "gym")),
            "no codex": dict(GYM_CODEX=None),
            "pinned codex": dict(GYM_CODEX=str(pin)),
            "homebrew codex": dict(GYM_CODEX="/opt/homebrew/bin/codex"),
            "a native codex in the test root": dict(GYM_CODEX=str(macho)),
            "no auth": dict(GYM_AUTH=None),
            "real auth": dict(GYM_AUTH=str(Path.home() / ".codex" / "auth.json")),
            "auth outside the test root": dict(GYM_AUTH=str(elsewhere / "auth.json")),
            "canary outside": dict(GYM_CANARY=str(elsewhere / "canary.txt")),
            "canary at the production location": dict(GYM_CANARY=str(Path.home() / ".a2d-gym-canary.txt")),
            "holdout view outside": dict(GYM_TEST_HOLDOUT=str(elsewhere)),
            "repo out outside": dict(GYM_TEST_OUT_REPO=str(elsewhere)),
            "harness outside": dict(GYM_TEST_HARNESS=str(A2D / "nano" / "harness.py")),
            "toolchain override outside": dict(GYM_TOOLCHAIN=str(elsewhere)),
        }
        for name, over in cases.items():
            self.assertTrue(refused(**over), name)

    def test_testing_child_paths_hold_no_real_codex(self):
        out = self.lib("import gymlib, json, os\n"
                       "e = gymlib.child_env()['PATH'].split(':')\nj = gymlib._env('/tmp', '/tmp/h')['PATH'].split(':')\n"
                       "print(json.dumps([e[0] == gymlib.codex_bin_dir(), e[1:], j[1:],"
                       " [d for d in e + j if os.path.exists(os.path.join(d, 'codex')) and gymlib.is_macho(os.path.join(d, 'codex'))]]))")
        self.assertEqual(json.loads(out), [True, ["/usr/bin", "/bin"], ["/usr/bin", "/bin"], []])

    def test_binary_override_confined_to_fakes(self):
        out = self.lib("import gymlib\n"
                       f"print(gymlib.binary_allowed({str(FAKE_A2D)!r}, gymlib.BINARY),"
                       " gymlib.binary_allowed('/usr/bin/true', gymlib.BINARY),"
                       f" gymlib.binary_allowed({str(A2D / 'target' / 'debug' / 'a2d')!r}, gymlib.BINARY),"
                       " gymlib.binary_allowed(gymlib.BINARY, gymlib.BINARY))")
        self.assertEqual(out, "True False False True")
        for name in ("gym_audit.py", "gym_pilot.py"):
            code, lines = self.run_script(name, "--binary", "/usr/bin/true", *(["--wave", "1"] if name == "gym_pilot.py" else []))
            self.assertEqual((code, lines[-1]["status"]), (2, "binary-override-refused"), name)

    def test_job_events_need_turn_completed_and_exact_host_off(self):
        """A successful job needs turn.completed; the only exempt event is the byte-identical
        host-off notice, once, before turn.started. Lines split by gymvendor.lines(): a U+2028
        inside a JSON string is not a line break (Sol B15)."""
        notice = ("Code Mode is unavailable because code-mode host is disabled. Code mode will fail closed; "
                  "enable `features.code_mode_host` and install `codex-code-mode-host`.")
        ev = lambda *xs: "".join(json.dumps(x) + "\n" for x in xs)                   # noqa: E731
        start, turn, done = {"type": "thread.started"}, {"type": "turn.started"}, {"type": "turn.completed"}
        msg = {"type": "item.completed", "item": {"type": "agent_message", "text": "a\u2028b\u0085c"}}
        off = {"type": "item.completed", "item": {"type": "error", "message": notice}}
        cases = {
            "ok": (ev(start, turn, msg, done), 0),
            "no turn.completed": (ev(start, turn, msg), 1),
            "notice before the turn": (ev(start, off, turn, msg, done), 0),
            "notice after turn.started": (ev(start, turn, off, msg, done), 1),
            "two notices": (ev(start, off, off, turn, msg, done), 1),
            "another error text": (ev(start, {"type": "item.completed", "item": {"type": "error", "message": notice + " "}},
                                      turn, msg, done), 1),
            "raw U+2028 in a string": (ev(start, turn, msg, done).replace("\\u2028", "\u2028"), 0),
            "empty": ("", 1),
        }
        rec = self.root / "authoring" / "gym-v3" / "_records"
        rec.mkdir(parents=True)
        for name, (text, want) in cases.items():
            (rec / f"{name.replace(' ', '_')}.jsonl").write_text(text)
        out = self.lib("import gymlib, json\n"
                       f"names = {list(cases)!r}\n"
                       "print(json.dumps({n: gymlib.count_other_events(gymlib.RECORDS / (n.replace(' ', '_') + '.jsonl'))"
                       " for n in names}))")
        self.assertEqual(json.loads(out), {n: w for n, (_, w) in cases.items()})
        self.assertIn("\u2028", cases["raw U+2028 in a string"][0])
        # the suspicious-command scan reads the same lines: a raw U+2028 elsewhere in the event no
        # longer breaks it into unparsable halves that hid the command
        cmd = {"type": "item.started", "item": {"type": "command_execution", "command": "/bin/zsh -lc 'curl x'",
                                                "aggregated_output": "a\u2028b"}}
        (rec / "susp").mkdir()
        (rec / "susp" / "codex.jsonl").write_text(ev(start, turn, cmd, done).replace("\\u2028", "\u2028"))
        self.assertEqual(self.lib(f"import gymlib\nprint(gymlib.suspicious_commands('susp', {str(self.tmp)!r}))"), "1")

    def test_gate_b_real_repo_and_negative(self):
        self.assertEqual(self.lib("import gymlib\nprint(gymlib.c3_frozen(), gymlib.GATE_COMMIT[:8])"), "True fb5110e7")
        r = self.tmp / "gate-repo"
        (r / "tools" / "holdout").mkdir(parents=True)
        (r / "tools" / "holdout" / "x.py").write_text("x\n")
        (r / "Cargo.toml").write_text("[package]\n")
        subprocess.run(["git", "init", "-q", str(r)], check=True)
        git(r, "add", ".")
        git(r, "commit", "-q", "-m", "base")
        base = git(r, "rev-parse", "HEAD").stdout.strip()
        check = f"import gymlib\nprint(gymlib.frozen_since({str(r)!r}, {base!r}, gymlib.GATE_PATHS))"
        self.assertEqual(self.lib(check), "True")
        (r / "tools" / "holdout" / "x.py").write_text("changed\n")
        self.assertEqual(self.lib(check), "False")
        (r / "tools" / "holdout" / "x.py").write_text("x\n")
        (r / "tools" / "holdout" / "new.py").write_text("untracked\n")
        self.assertEqual(self.lib(check), "False")
        (r / "tools" / "holdout" / "new.py").unlink()
        self.assertEqual(self.lib(check), "True")

    def test_every_driver_refuses_while_gate_closed(self):
        self.ledger.write_text('{"kind": "registration"}\n')        # replicate 2 has not generated
        for name in DRIVERS:
            args = {"gym_split.py": ["register", "--seed", "1"], "gym_band.py": ["band", "--wave", "1", "--run", "x"],
                    "gym_export.py": ["certify"]}.get(name, [])
            code, lines = self.run_script(name, *args)
            self.assertEqual(code, 2, name)
            self.assertEqual(lines, [{"stage": lines[0]["stage"], "status": "launch-gate-closed"}], name)
        self.assertFalse((self.gym / "_select").exists())
        code, lines = self.run_script("gym_author.py", "--stop")             # stopping is never gated
        self.assertEqual((code, lines[-1]["status"]), (0, "stop-requested"))
        code, lines = self.run_script("gym_author.py", "--drained")
        self.assertEqual(code, 0)
        out = self.lib("from gymlib import LaunchGateClosed, launch\n"
                       "try:\n    launch('/dev/null', ['/usr/bin/true'])\n    print('LAUNCHED')\n"
                       "except LaunchGateClosed:\n    print('REFUSED')")
        self.assertEqual(out, "REFUSED")

    def test_static_isolation_rules(self):
        """No gym source reads A2D_HOLDOUT_DIR or C3_* variables, imports confirm3, or puts nano/
        on the import path; tests are excluded (they only scrub those variables)."""
        reads = re.compile(r"(environ\s*(\.get\s*\(|\[)|getenv\s*\()\s*[\"'](A2D_HOLDOUT_DIR|C3_)")
        exports = re.compile(r"(^|\s)(export\s+)?A2D_HOLDOUT_DIR=")
        nano_path = re.compile(r"sys\.path\.(insert|append)\([^\n]*nano")
        for p in sorted(TOOLS.glob("*.py")) + sorted(TOOLS.glob("*.sh")):
            text = p.read_text()
            self.assertIsNone(reads.search(text), p.name)
            self.assertIsNone(exports.search(text), p.name)
            self.assertIsNone(nano_path.search(text), p.name)
            if p.suffix == ".py":
                for node in ast.walk(ast.parse(text)):
                    if isinstance(node, ast.Import):
                        self.assertFalse(any("confirm3" in a.name or a.name == "eval" for a in node.names), p.name)
                    if isinstance(node, ast.ImportFrom):
                        self.assertNotIn("confirm3", node.module or "", p.name)
        self.assertIn("-u A2D_HOLDOUT_DIR", (TOOLS / "detached.sh").read_text())
        self.assertIn(".a2d-gym-drivers", (TOOLS / "detached.sh").read_text())

    def test_testing_mode_keyed_to_gym_root(self):
        """GYM_TEST_* hooks need a non-default GYM_ROOT; with the default root the gym uses the
        production paths (computed as strings only; nothing under them is touched)."""
        env = dict(clean_env(), GYM_TEST_SKIP_PREREQS="1", GYM_TEST_HOLDOUT=str(self.holdout),
                   GYM_CODEX="/bin/false", A2D_HOLDOUT_DIR=str(self.tmp / "elsewhere"))
        out = self.lib("import gymlib\nfrom pathlib import Path\n"
                       "print(gymlib.TESTING, gymlib.HOLDOUT_VIEW == Path.home() / '.a2d-holdout', gymlib.CODEX,"
                       " gymlib.OUT_REPO == gymlib.REPO, gymlib.ROOT == Path.home().resolve() / '.a2d-gym')\n"
                       "print([str(p.relative_to(Path.home())) for p in gymlib.must_deny()])\n"
                       "print(gymlib.BINARY == gymlib.ROOT / 'target' / 'debug' / 'a2d', gymlib.CANARY.name)",
                       env=env)
        a, b, c = out.splitlines()
        pin = Path.home() / ".a2d-codex-pin" / "0.162.0" / "bin" / "codex"
        self.assertEqual(a, f"False True {pin} True True")
        self.assertEqual(b, "['.a2d-holdout/confirmation-3', '.a2d-holdout/authoring/confirmation-3/_records', "
                            "'.a2d-holdout/runs/c2-confirm2-decision']")
        self.assertEqual(c, "True .a2d-gym-canary.txt")
        self.assertFalse((self.tmp / "elsewhere").exists())

    def test_roots_are_disjoint(self):
        for bad in (self.holdout / "gym", A2D / "runs" / "gym-root-test"):
            env = dict(self.env, GYM_ROOT=str(bad))
            r = subprocess.run([sys.executable, "-I", "-c", f"import sys; sys.path.insert(0, {str(TOOLS)!r}); import gymlib"],
                               capture_output=True, text=True, env=env)
            self.assertNotEqual(r.returncode, 0, bad)
            self.assertIn("overlaps", r.stderr)
            self.assertFalse(bad.exists())

    def test_writes_confined(self):
        out = self.lib(
            "import gymlib\nfrom pathlib import Path\n"
            "def tryw(p):\n"
            "    try:\n        gymlib.save_json(Path(p), {'a': 1})\n        return 'ok'\n"
            "    except PermissionError:\n        return 'refused'\n"
            f"print(tryw({str(self.tmp / 'outside.json')!r}), tryw({str(self.repo / 'runs' / 'other' / 'x.json')!r}),"
            f" tryw({str(self.repo / 'x.json')!r}), tryw({str(self.repo / 'runs' / 'c2-gym3' / 'x.json')!r}),"
            f" tryw({str(self.root / 'x.json')!r}))\n"
            "c = gymlib.PriorCorpus()\n"
            "print(all(not hasattr(c, n) for n in ('save', 'write', 'add')), [n for n, _, _ in c.roots])")
        a, b = out.splitlines()
        self.assertEqual(a, "refused refused refused ok ok")
        self.assertEqual(b, "True ['repo-tasks', 'holdout']")

    def test_prior_corpus_adds_confirmation3_only_after_decision(self):
        def task(d, fam):
            d.mkdir(parents=True)
            (d / "task.json").write_text(json.dumps({"id": d.name, "family": fam, "requirements": "R1. words"}))
        task(self.holdout / "confirmation-1" / "a-l1", "old-family")
        task(self.holdout / "confirmation-3" / "c3-f01-l1", "c3-f01")
        task(self.holdout / "authoring" / "confirmation-3" / "c3-f02" / "out" / "c3-f02-l1", "c3-f02")
        task(self.holdout / "authoring" / "confirmation-3" / "_records" / "x-l1", "records-family")
        probe = ("import gymlib\nc = gymlib.PriorCorpus()\nrows, counts = c.tasks()\n"
                 "fams = sorted({f for f, _ in rows if not f.startswith(('c1', 'calib', 'dev'))} & "
                 "{'old-family', 'c3-f01', 'c3-f02', 'records-family'})\n"
                 "print(fams, c.manifest()['sha256'][:12])")
        before = self.lib(probe)
        self.assertTrue(before.startswith("['old-family']"), before)
        (self.repo / "runs" / "c2-confirm3").mkdir(parents=True)
        (self.repo / "runs" / "c2-confirm3" / "decision.json").write_text("{}")
        after = self.lib(probe)
        self.assertTrue(after.startswith("['c3-f01', 'c3-f02', 'old-family']"), after)
        self.assertEqual(before.split()[-1], after.split()[-1])           # the frozen base manifest holds

    def test_prior_corpus_refuses_aliases_of_confirmation3(self):
        """Traversal follows symlinks, so a differently named alias of confirmation-3 (or of its
        authoring records) is pruned by its resolved target, not by its name (Sol I5)."""
        def task(d, fam):
            d.mkdir(parents=True)
            (d / "task.json").write_text(json.dumps({"id": d.name, "family": fam, "requirements": "R1. words"}))
        task(self.holdout / "confirmation-1" / "a-l1", "old-family")
        task(self.holdout / "confirmation-3" / "c3-f01-l1", "c3-f01")
        task(self.holdout / "authoring" / "confirmation-3" / "_records" / "x-l1", "records-family")
        (self.holdout / "confirmation-1" / "alias").symlink_to(self.holdout / "confirmation-3")
        (self.holdout / "dev-holdout").mkdir()
        (self.holdout / "dev-holdout" / "recs").symlink_to(self.holdout / "authoring" / "confirmation-3" / "_records")
        probe = ("import gymlib\nrows, counts = gymlib.PriorCorpus().tasks()\n"
                 "print(sorted({f for f, _ in rows} & {'old-family', 'c3-f01', 'records-family'}))")
        self.assertEqual(self.lib(probe), "['old-family']")
        (self.repo / "runs" / "c2-confirm3").mkdir(parents=True)
        (self.repo / "runs" / "c2-confirm3" / "decision.json").write_text("{}")
        self.assertEqual(self.lib(probe), "['c3-f01', 'old-family']")          # records stay out after the decision

    def test_child_env_is_scrubbed(self):
        env = dict(self.env, A2D_HOLDOUT_DIR=str(self.tmp / "nope"), C3_CODEX="/x", C3_TEST_PROBE="/y")
        out = self.lib("import gymlib, json\n"
                       "e = gymlib.child_env()\nj = gymlib._env('/tmp', '/tmp/h')\n"
                       "print(json.dumps([sorted(k for k in e if k == 'A2D_HOLDOUT_DIR' or k.startswith('C3_')),"
                       " e['A2D_CODEX_HOME_ROOT'] == str(gymlib.ROOT / 'codex-homes'),"
                       " e['CARGO_TARGET_DIR'] == str(gymlib.ROOT / 'target'),"
                       " sorted(k for k in j if k == 'A2D_HOLDOUT_DIR' or k.startswith('C3_'))]))", env=env)
        self.assertEqual(json.loads(out), [[], True, True, []])

    def test_emit_redacts(self):
        out = self.lib("from gymlib import emit\nemit(stage='x', id='g3-f001', count=3, error='Has Spaces!', bogus=1)\n"
                       "emit(stage='y', id='secret-family-name', status='a' * 60)\nemit(stage='z', id='c3-f01')")
        a, b, c = [json.loads(x) for x in out.splitlines()]
        self.assertEqual((a["stage"], a["id"], a["count"], a["error"]), ("x", "g3-f001", 3, "redacted"))
        self.assertNotIn("bogus", a)
        self.assertEqual((b["id"], b["status"], c["id"]), ("redacted", "redacted", "redacted"))

    def test_profiles_deny_the_holdout_last(self):
        out = self.lib("import gymlib\nfrom pathlib import Path\n"
                       f"w = Path({str(self.tmp)!r})\n"
                       "p = gymlib.profile(w, w / 'tag').strip().splitlines()[-1]\n"
                       "c = gymlib.compile_profile(w, '/x')\nr = gymlib.run_profile('/bin/ls', w)\n"
                       "h = str(Path.home() / '.a2d-holdout')\n"
                       "d = '/System/Volumes/Data' + h\n"
                       "sig = '(deny signal)(allow signal (target same-sandbox))(deny process-info*)'\n"
                       "print(p.startswith('(deny file-read* file-write*') and h in p and d in p,"
                       " c.endswith(gymlib._protect_rules()) and h in c and d in c, r.endswith(gymlib._protect_rules()),"
                       " str(gymlib.ROOT) in gymlib.profile(w, w / 'tag'))\n"
                       "print(sig in c, sig in r, all(x in c and x in r for x in ('(subpath \"/Users\")',"
                       " '(subpath \"/System/Volumes/Data/Users\")', '(subpath \"/Volumes\")',"
                       " '(subpath \"/System/Volumes/Data/private/tmp\")', '(subpath \"/System/Volumes/Data/private/var/folders\")',"
                       " '(subpath \"/System/Volumes/Data' + str(Path.home().resolve()) + '\")')))")
        self.assertEqual(out.splitlines(), ["True True True True", "True True True"])

    def test_symlink_refused(self):
        w = self.gym / "g3-f001"
        w.mkdir(parents=True)
        (w / "x").symlink_to("/etc/hosts")
        out = self.lib("from gymlib import safe_read_text\n"
                       f"try:\n    safe_read_text({str(w / 'x')!r})\n    print('READ')\nexcept OSError:\n    print('REFUSED')")
        self.assertEqual(out, "REFUSED")

    def test_driver_lock_and_stop_gate(self):
        out = self.lib("from gymlib import DriverLock\nwith DriverLock():\n    try:\n        with DriverLock():\n"
                       "            print('SECOND')\n    except RuntimeError:\n        print('REFUSED')")
        self.assertEqual(out, "REFUSED")
        out = self.lib("from gymlib import StopRequested, launch, request_stop\nrequest_stop()\n"
                       "try:\n    launch('/dev/null', ['/usr/bin/true'])\n    print('LAUNCHED')\n"
                       "except StopRequested:\n    print('REFUSED')")
        self.assertEqual(out, "REFUSED")

    def test_drained_fails_closed(self):
        self.gym.mkdir(parents=True)
        drained = "from gymlib import drained\nprint(drained())"
        self.assertEqual(self.lib(drained), "False")
        (self.gym / "STOP").write_text("stop")
        self.assertEqual(self.lib(drained), "False")
        self.rec.mkdir()
        dead = subprocess.Popen(["/usr/bin/true"], start_new_session=True)
        dead.wait()
        (self.rec / "pids.txt").write_text(f"{dead.pid} 1\n")
        self.assertEqual(self.lib(drained), "True")
        (self.rec / "active.json").write_text(json.dumps({"active": ["g3-f001"]}))
        (self.rec / "author-g3-f001").mkdir()
        (self.rec / "author-g3-f001" / "status.json").write_text(json.dumps({"attempts": 1}))
        self.assertEqual(self.lib(drained), "False")

    # ------------------------------------------------------------------ rubric (M1-M5, tags)

    def fixture_task(self, cls="depth", level=3, *flags):
        d = self.tmp / f"author-{cls}-{level}-{'-'.join(flags)}"
        d.mkdir()
        subprocess.run([sys.executable, "-I", str(HERE / "fake_author.py"), "author", "g3-f001", cls, *flags],
                       cwd=d, check=True)
        return d / "out" / "g3-f001" / f"g3-f001-l{level}"

    def test_rubric_mechanical_checks(self):
        t = self.fixture_task("depth", 3)
        bad = self.fixture_task("depth", 3, "bad-rubric")
        out = self.lib(
            "import gym_rubric as r, json, re\nfrom pathlib import Path\n"
            f"t, bad = Path({str(t)!r}), Path({str(bad)!r})\n"
            "req = json.loads((t / 'task.json').read_text())['requirements']\n"
            "hid, pub, ref = [(t / f).read_text() for f in ('hidden_tests.rs', 'public_tests.rs', 'reference.rs')]\n"
            "rub = json.loads((t / 'rubric.json').read_text())\n"
            "print(r.check_task_dir(t, 3, 'depth')['ok'], r.check_task_dir(bad, 3, 'depth')['m1'])\n"
            "print(r.m1(req, hid.replace('// R12,R20', '// R12,R20,R99'))['unknown'], r.m1(req.replace('R20.', 'R21.'), hid)['ok'],"
            " r.m1(req + '\\nR21. extra clause', hid)['uncovered_ids'])\n"
            "m = json.loads(json.dumps(rub)); m['mutants']['m1']['refuted_by'] = 'words that are not in the clause at all'\n"
            "n = json.loads(json.dumps(rub)); n['mutants']['m2']['violates'] = ['R99']\n"
            "w = json.loads(json.dumps(rub)); w['mutants']['m1']['refuted_by'] = rub['mutants']['m1']['refuted_by'].replace(' ', '  ', 1)\n"
            "print(r.m2(req, rub)['ok'], r.m2(req, m), r.m2(req, n)['labelled'], r.m2(req, w)['misreadings'])\n"
            "names = sorted(r.test_names(hid))\n"
            "print(r.m5(req, pub, hid, ref)['ok'], r.m5(req + ' ' + names[0], pub, hid, ref)['leaked'],"
            " r.m5(req, pub, hid.replace(names[0], 'hidden_named'), ref)['not_opaque'],"
            " r.m5(req, pub, hid.replace(names[1], names[0]), ref)['duplicates'],"
            " r.m5(req + ' prefix_' + names[0] + '_suffix', pub, hid, ref)['leaked'], r.m5(req, pub, hid, ref + 'x' + names[2] + '0')['leaked'])\n"
            "print(r.tags(req, rub, 3, 'depth', hid)['ok'], r.tags(req, rub, 3, 'scale', hid)['ok'],"
            " r.tags(req, dict(rub, difficulty=['depth', 'edges']), 3, 'depth', hid)['ok'], r.tags(req, dict(rub, difficulty=['depth']), 1, 'depth', hid)['ok'],"
            " r.tags(re.sub(r' This clause modifies R[0-9]+[.]', '', req), rub, 3, 'depth', hid)['ok'],"
            " r.tags(req, dict(rub, difficulty=['surface', 'edges']), 1, 'surface', hid)['ok'], r.tags(req, rub, 3, None, hid)['ok'])")
        lines = out.splitlines()
        self.assertEqual(lines[0], "True {'ok': False, 'clauses': 20, 'untraced': 1, 'unknown': 0, 'uncovered': 0, "
                                   "'overcited': 0, 'uncovered_ids': []}")
        self.assertEqual(lines[1], "1 False ['R21']")
        self.assertEqual(lines[2], "True {'ok': False, 'labelled': 4, 'misreadings': 1} 3 1")    # no normalisation
        self.assertEqual(lines[3], "True 1 1 1 1 1")                                              # literal substrings
        self.assertEqual(lines[4], "True False False False False False False")

    def test_rubric_ids_traces_floors_and_evidence(self):
        """ASCII-only ids, the reference regex (COLOR21 is not R21), clause floors whatever the
        tags, at most 3 ids per trace line with coverage only from small traces (C-I3/C-I4/C-I22),
        and each tag's structured evidence checked against the text (Sol I2, C-I5)."""
        t = self.fixture_task("state", 4)
        u = self.fixture_task("surface", 1)
        sc = self.fixture_task("scale", 1)
        out = self.lib(
            "import gym_rubric as r, json\nfrom pathlib import Path\n"
            f"t, u, sc = Path({str(t)!r}), Path({str(u)!r}), Path({str(sc)!r})\n"
            "req = json.loads((t / 'task.json').read_text())['requirements']\n"
            "hid = (t / 'hidden_tests.rs').read_text()\nrub = json.loads((t / 'rubric.json').read_text())\n"
            "print(r.check_task_dir(t, 4, 'state')['ok'], sorted(rub))\n"
            "print(r.clauses('R1. a\\nR\\u0662. b') == {1: 'R1. a\\nR\\u0662. b'}, r.hidden_traces('// R\\uff12\\n#[test]\\nfn h_0123456789() {}'),"
            " r._ids('COLOR21 xR4 R7 (R8) R9x'))\n"
            "five = '\\n'.join(f'R{i}. clause {i} words' for i in range(1, 6))\n"
            "print(r.clause_floor(five, 2)['ok'], r.clause_floor(five, 3)['ok'], r.clause_floor(req, 4)['ok'])\n"
            "big = hid.replace('// R4\\n', '// R4,R1,R2,R3\\n', 1)\n"
            "stacked = hid.replace('// R3,R19\\n', '// R3,R19\\n// R1,R2\\n')\n"
            "print(r.m1(req, big)['overcited'], r.m1(req, big)['ok'], r.m1(req, stacked)['ok'])\n"
            "def ev(**kw):\n    return r.tag_evidence(req, dict(rub, **kw), 4, hid)\n"
            "print(ev(), ev(states=rub['states'][:4])['state'], ev(states=rub['states'][:4] + ['nowhere'])['state'],"
            " ev(sequences=['h_00000000'])['state'], ev(sequences=[])['state'])\n"
            "print(ev(modifiers=rub['modifiers'][:2])['depth'], ev(modifiers=rub['modifiers'] + [{'clause': 'R2', 'modifies': 'R1'}])['depth'],"
            " ev(boundaries=['two words'])['edges'], ev(boundaries=['not in the text'])['edges'])\n"
            "def ev2(d, lv, **kw):\n"
            "    q = json.loads((d / 'task.json').read_text())['requirements']\n"
            "    b = json.loads((d / 'rubric.json').read_text())\n"
            "    return r.tag_evidence(q, dict(b, **kw), lv, hid)\n"
            "us = json.loads((u / 'rubric.json').read_text())['signatures']\n"
            "print(ev2(u, 1)['surface'], ev2(u, 1, signatures=us[:5])['surface'], ev2(u, 1, signatures=us[:5] + ['pub type Nowhere = u8'])['surface'],"
            " ev2(sc, 1)['scale'], ev2(sc, 1, scale={'bound': 'at most a few customers', 'complexity': 'O(n log n) time'})['scale'])\n"
            "print(r.rubric_problems([]), r.rubric_problems(dict(rub, mutants=['m1'])), r.rubric_problems(dict(rub, mutants={'m1': 'naive'})),"
            " r.rubric_problems(dict(rub, difficulty=[['depth']])), r.rubric_problems(dict(rub, states='x')))")
        lines = out.splitlines()
        self.assertEqual(lines[0], "True ['boundaries', 'difficulty', 'modifiers', 'mutants', 'sequences', 'states']")
        self.assertEqual(lines[1], "True {'h_0123456789': []} [7, 8]")
        self.assertEqual(lines[2], "True False True")
        self.assertEqual(lines[3], "1 False False")
        self.assertEqual(lines[4], "{'depth': True, 'state': True, 'edges': True} False False False False")
        self.assertEqual(lines[5], "False False False False")
        self.assertEqual(lines[6], "True False False True False")
        self.assertEqual(lines[7], "['rubric.json is not an object'] ['rubric.json mutants malformed'] "
                                   "['rubric.json mutants malformed'] ['rubric.json difficulty malformed'] "
                                   "['rubric.json states malformed']")

    def test_rubric_m3_quotes_and_m4_sampling(self):
        """A DETERMINED quote is a literal substring (no whitespace normalisation, Sol B11) of a
        clause the test's own trace cites (C-I2): never another clause, never the boilerplate
        before R1; a `*` verdict may quote any numbered clause."""
        req = ("Output ONLY the Rust source code inside the fences, nothing else at all.\n"
               "R1. The queue is ordered by descending id, not by arrival order, for equal times.\nR2. Two.")
        hid = "// R1\n#[test]\nfn h_00000001() {}\n// R2\n#[test]\nfn h_00000002() {}\n"
        out = self.lib(
            "import gym_rubric as r\n"
            f"req0, hid = {req!r}, {hid!r}\n"
            "req = r.quotable(req0, hid, 'h_00000001')\nother = r.quotable(req0, hid, 'h_00000002')\n"
            "star = r.quotable(req0, hid, '*')\n"
            "q = 'The queue is ordered by descending id, not by'\n"
            "pre = dict(verdict='DETERMINED', quote='Output ONLY the Rust source code inside the fences,',"
            " excluded_reading='ascending id order for ties')\n"
            "ok = {'verdict': 'DETERMINED', 'quote': q, 'excluded_reading': 'ascending id order for ties'}\n"
            "short = dict(ok, quote='ordered by descending id')\n"
            "fake = dict(ok, quote='The queue is ordered by ascending id, not by')\n"
            "noread = dict(ok, excluded_reading='')\n"
            "under = dict(ok, verdict='UNDERDETERMINED')\n"
            "wrapped = dict(ok, quote='The queue is  ordered\\nby descending id, not by')\n"
            "print(r.combine(ok, ok, req), r.combine(ok, under, req), r.combine(ok, short, req), r.combine(fake, ok, req),"
            " r.combine(ok, noread, req), r.combine(ok, None, req), r.combine(wrapped, ok, req))\n"
            "print(r.combine(ok, ok, other), r.combine(ok, ok, star), r.combine(pre, pre, star), r.combine(pre, pre, req),"
            " r.quotable(req0, hid, 'h_missing'))\n"
            "names = ['h_%08x' % i for i in range(30)]\n"
            "a = r.sample_passing('g3-f001-l1', 'c1', names, ['h_00000003'], 'seed')\n"
            "b = r.sample_passing('g3-f001-l1', 'c1', names, ['h_00000003'], 'seed')\n"
            "c = r.sample_passing('g3-f001-l1', 'c2', names, ['h_00000003'], 'seed')\n"
            "print(len(a), a == b, 'h_00000003' not in a, a != c or True, r.sample_passing('t', 'c', ['h_1'], [], 's'))\n"
            "exp = {'g3-f001-l1': ['h_1', '*']}\n"
            "good = '{\"task\": \"g3-f001-l1\", \"test\": \"h_1\", \"verdict\": \"DETERMINED\", \"quote\": \"q\", \"excluded_reading\": \"r\"}\\n'"
            " + '{\"task\": \"g3-f001-l1\", \"test\": \"*\", \"verdict\": \"UNDERDETERMINED\", \"quote\": \"\", \"excluded_reading\": \"\"}\\n'\n"
            "print(r.parse_verdicts(good, exp) is not None, r.parse_verdicts(good.split('\\n')[0], exp),"
            " r.parse_verdicts(good + good, exp), r.parse_verdicts(good.replace('\"quote\"', '\"quot\"'), exp))\n"
            "print(sorted(r.EXCLUSION_REASONS))")
        lines = out.splitlines()
        self.assertEqual(lines[0], "DETERMINED REPAIR REPAIR REPAIR REPAIR REPAIR REPAIR")
        self.assertEqual(lines[1], "REPAIR DETERMINED REPAIR REPAIR []")
        self.assertEqual(lines[2], "2 True True True ['h_1']")
        self.assertEqual(lines[3], "True None None None")
        self.assertNotIn("audit", lines[4])
        self.assertNotIn("solver", lines[4])

    def test_oracle_parsing_ports(self):
        """Ports of oracle.rs split_inner_attributes, sandbox.rs parse_test_results and
        validate.rs uncovered_api."""
        out = self.lib(
            "import gym_validate as v\n"
            "print(repr(v.split_inner_attributes('//! d\\r\\n#![allow(\\n  x\\n)]\\n\\nfn a() {}\\n')))\n"
            "print(v.parse_results('test x ... ok\\ntest result: ok. 3 passed; 1 failed; 0 ignored; 0 measured;"
            " 0 filtered out; finished in 0.02s\\n'), v.parse_results('no summary'))\n"
            "print(sorted(v.uncovered_api('pub fn a(x), pub fn Rope::from_str(s), pub fn b<T>(), pub fn c(',"
            " 'a(1); from_str(2); c::<u8>(3)')))")
        self.assertEqual(out.splitlines(), ["('//! d\\n#![allow(\\n  x\\n)]\\n', 'fn a() {}')",
                                            "(3, 1, 0.02) (0, 0, None)", "['b']"])

    def test_task_schema_parity_with_the_controller(self):
        """task.json as strictly as crates/a2d-lift/src/task.rs (serde): required string fields,
        test_timeout_secs a JSON integer (not bool, not float, not string) in u64 range, no
        duplicate keys, no NaN, no lone surrogate (Sol B14, C-B9)."""
        base = '{"id": "g3-f001-l1", "family": "g3-f001", "requirements": "R1. x"%s}'
        cases = {"ok": (base % ', "test_timeout_secs": 10', True), "default": (base % "", True),
                 "max": (base % f', "test_timeout_secs": {2**64 - 1}', True),
                 "string": (base % ', "test_timeout_secs": "10"', False), "float": (base % ', "test_timeout_secs": 10.0', False),
                 "exp": (base % ', "test_timeout_secs": 1e1', False), "bool": (base % ', "test_timeout_secs": true', False),
                 "negative": (base % ', "test_timeout_secs": -1', False), "big": (base % f', "test_timeout_secs": {2**64}', False),
                 "dup": (base % ', "id": "g3-f001-l2"', False), "nan": (base % ', "test_timeout_secs": NaN', False),
                 "no family": ('{"id": "g3-f001-l1", "requirements": "R1. x"}', False),
                 "int family": ('{"id": "g3-f001-l1", "family": 7, "requirements": "R1. x"}', False),
                 "surrogate": ('{"id": "g3-f001-l1", "family": "g3-f001", "requirements": "R1. \\ud800"}', False),
                 "array": ("[]", False), "dot id": ('{"id": ".x", "family": "g3-f001", "requirements": "R1. x"}', False)}
        (self.tmp / "cases.json").write_text(json.dumps(cases))
        out = self.lib("import gymlib, json\n"
                       f"cases = json.load(open({str(self.tmp / 'cases.json')!r}))\n"
                       "def ok(t):\n    try:\n        gymlib.parse_task_json(t.encode())\n        return True\n"
                       "    except ValueError:\n        return False\n"
                       "print(json.dumps({k: ok(v[0]) for k, v in cases.items()}))")
        self.assertEqual(json.loads(out), {k: v[1] for k, v in cases.items()})

    def test_validator_rejects_malformed_tasks_instead_of_crashing(self):
        """Malformed model output becomes per-task REJECT reasons (Sol I3, C-B9); a family without a
        rubric class is refused (C-I20). These return before any compile."""
        t = self.fixture_task("depth", 1)
        tj = json.loads((t / "task.json").read_text())
        variants = {"timeout string": lambda d: (d / "task.json").write_text(json.dumps(dict(tj, test_timeout_secs="10"))),
                    "invalid json": lambda d: (d / "task.json").write_text("{"),
                    "missing hidden": lambda d: (d / "hidden_tests.rs").unlink()}
        dirs = {}
        for name, mutate in variants.items():
            d = self.tmp / name.replace(" ", "-") / t.name
            shutil.copytree(t, d)
            mutate(d)
            dirs[name] = str(d)
        out = self.lib("import gym_validate as v, json\nfrom pathlib import Path\n"
                       f"dirs = {dirs!r}\n"
                       "print(json.dumps({k: v.validate_task_safe(Path(d), 'depth')[0] for k, d in dirs.items()}))\n"
                       f"print(v.validate_task_safe(Path({str(t)!r}), None)[0])")
        a, b = out.splitlines()
        self.assertEqual(json.loads(a), {"timeout string": ["task.json does not match the controller's schema"],
                                         "invalid json": ["task.json does not match the controller's schema"],
                                         "missing hidden": ["malformed task files"]})
        self.assertEqual(b, "['no rubric class for the family']")

    def test_compile_profile_blocks_include_str_under_home(self):
        """A compile-time include_str! of a file under HOME, the temp roots or the (fake) holdout
        fails under the gym's compile profile, by its plain AND its /System/Volumes/Data spelling;
        the same read of a file in the step dir compiles (control)."""
        scratch = Path.home() / f".a2d-gym-test-{os.getpid()}-{os.urandom(4).hex()}"
        scratch.mkdir()
        data = "/System/Volumes/Data"
        try:
            (scratch / "secret.txt").write_text("GYM-HOME-SECRET\n")
            (self.tmp / "tmp-secret.txt").write_text("GYM-TMP-SECRET\n")
            (self.holdout / "confirmation-3" / "SECRET.txt").write_text("C3-SECRET\n")
            held = self.holdout / "confirmation-3" / "SECRET.txt"
            self.assertTrue(Path(data + str(scratch / "secret.txt")).is_file())     # the alias exists here
            out = self.lib(
                "import gymlib, shutil, tempfile\nfrom pathlib import Path\n"
                "def build(target, inside=False):\n"
                f"    d = Path(tempfile.mkdtemp(prefix='gym-canary-', dir={str(self.tmp)!r})).resolve()\n"
                "    try:\n"
                "        if inside:\n"
                "            (d / 'inside.txt').write_text('IN')\n            target = d / 'inside.txt'\n"
                "        (d / 'c.rs').write_text('pub fn leak() -> &\\'static str { include_str!(%s) }\\nfn main() {}\\n' % repr(str(target)).replace(\"'\", '\"'))\n"
                "        r = gymlib.sandboxed_rustc(['--edition', '2024', str(d / 'c.rs'), '-o', str(d / 'bin')], d)\n"
                "        return r.returncode == 0\n"
                "    finally:\n"
                "        shutil.rmtree(d, ignore_errors=True)\n"
                f"print(build(Path({str(scratch / 'secret.txt')!r})), build(Path({str(self.tmp / 'tmp-secret.txt')!r})),"
                " build(None, inside=True))\n"
                f"print(build(Path({data + str(scratch / 'secret.txt')!r})), build(Path({str(held)!r})),"
                f" build(Path({data + str(held)!r})))", timeout=600)
            self.assertEqual(out.splitlines(), ["False False True", "False False False"])
        finally:
            shutil.rmtree(scratch, ignore_errors=True)

    # ------------------------------------------------------------------ split and band

    def test_split_is_seeded_stratified_and_committed(self):
        fams = {f"g3-f{i:03d}": ("depth", "state", "scale", "surface", "edges")[(i - 1) % 5] for i in range(1, 91)}
        (self.tmp / "fams.json").write_text(json.dumps(fams))
        out = self.lib(
            "import json, gym_split as s\n"
            f"fams = json.load(open({str(self.tmp / 'fams.json')!r}))\n"
            "a = s.split(fams, 11)\nb = s.split(fams, 11)\nc = s.split(fams, 12)\n"
            "hold = [f for f, x in a['assign'].items() if x['set'] == 'holdout']\n"
            "w1 = [f for f, x in a['assign'].items() if x['wave'] == 1]\n"
            "per = sorted({fams[f] for f in hold}), [sum(1 for f in hold if fams[f] == k) for k in s.CLASSES]\n"
            "print(a == b, a != c, len(hold), len(w1), len(a['wave2_order']), per)\n"
            "print(len(set(a['assign'])) == 90, not set(hold) & set(w1), sorted(a['wave2_order']) == sorted("
            "f for f, x in a['assign'].items() if x['wave'] == 2))\n"
            "try:\n    s.record(fams, 1, 25, 12, 0.3, 4, 45)\n    print('accepted')\nexcept ValueError:\n    print('refused')")
        a, b, c = out.splitlines()
        self.assertEqual(a, "True True 25 12 53 (['depth', 'edges', 'scale', 'state', 'surface'], [5, 5, 5, 5, 5])")
        self.assertEqual(b, "True True True")
        self.assertEqual(c, "refused")
        split = self.repo / "runs" / "c2-gym3" / "split.json"
        split.parent.mkdir(parents=True)
        split.write_text("{}\n")
        probe = "import gym_split\nprint(gym_split.committed())"
        self.assertEqual(self.lib(probe), "False")                        # untracked
        git(self.repo, "add", "runs/c2-gym3/split.json")
        git(self.repo, "commit", "-q", "-m", "split")
        self.assertEqual(self.lib(probe), "True")
        split.write_text('{"edited": 1}\n')
        self.assertEqual(self.lib(probe), "False")                        # modified after the commit

    def test_audit_rounds_follow_content_and_settle_on_it(self):
        """C-B7: a new audit round is due whenever the latest round's content differs from the
        current content, and after a void until the content has 2 void rounds; settled() and the
        adjudication schedule always speak about the current content; a task void twice still
        needs its M4 spec review (B12)."""
        out = self.lib(
            "import gym_audit as a, gym_export as e, gym_adjudicate as j\n"
            "R = lambda c, v=False, p=True, run=1: {'content': c, 'void': v, 'passed': p and not v, 'run': run}\n"
            "cases = {\n"
            " 'new': ({}, {}),\n"
            " 'audited': ({'r1': R('A')}, {}),\n"
            " 'adjudicated': ({'r1': R('A')}, {'r1': {'repaired': False, 'excluded': False}}),\n"
            " 'changed': ({'r1': R('X')}, {'r1': {'repaired': False, 'excluded': False}}),\n"
            " 'repaired': ({'r1': R('X', p=False)}, {'r1': {'repaired': True, 'excluded': False}}),\n"
            " 'void1': ({'r1': R('A', v=True)}, {}),\n"
            " 'void2': ({'r1': R('A', v=True), 'r2': R('A', v=True)}, {}),\n"
            " 'void2-reviewed': ({'r1': R('A', v=True), 'r2': R('A', v=True)}, {'r2': {'repaired': False, 'excluded': False}}),\n"
            " 'void-other-content': ({'r1': R('X', v=True), 'r2': R('A', v=True)}, {}),\n"
            " 'r3-after-change': ({'r1': R('X'), 'r2': R('Y'), 'r3': R('A', p=False)}, {'r1': {}, 'r2': {}}),\n"
            "}\n"
            "for k, (au, ad) in cases.items():\n"
            "    d = j.due({'t': au}, {'t': ad}, ['t'], {'t': 'A'})\n"
            "    print(k, a.needs({'t': au}, 't', 'A'), e.settled('t', {'t': au}, {'t': ad}, 'A'),"
            " [(m, x['round'], x['kind']) for (f, m), xs in d.items() for x in xs])")
        self.assertEqual(out.splitlines(), [
            "new r1 audit-pending []",
            "audited None adjudication-pending [('repair', 'r1', 'audit')]",
            "adjudicated None ok []",
            "changed r2 stale-audit []",
            "repaired r2 stale-audit []",
            "void1 r2 audit-pending []",
            "void2 None adjudication-pending [('classify', 'r2', 'void')]",
            "void2-reviewed None void-twice []",
            "void-other-content r3 audit-pending []",
            "r3-after-change None adjudication-pending [('repair', 'r3', 'audit')]"])

    def test_m4_needs_positive_task_bound_evidence(self):
        """B12/C-B8/C-I19: a passed task's evidence is the audit score with every hidden test
        passed (its hidden output is never read); a failed task's is its hidden output; a task
        that did not compile, or was void twice, still gets 2 seeded hidden tests reviewed;
        without evidence the task stays pending."""
        t = self.fixture_task("depth", 1)
        suite = self.root / "gym-v3"
        names = sorted(re.findall(r"fn (h_[0-9a-f]+)\(", (t / "hidden_tests.rs").read_text()))
        tids = {}
        run = self.root / "runs" / "g3-audit-1"
        recs = []
        for i, (case, result, hidden_out) in enumerate([
                ("passed", {"all_passed": True, "compiled": True, "passed": 26, "expected": 26}, None),
                ("passed-noscore", None, None),
                ("passed-partial", {"all_passed": True, "compiled": True, "passed": 20, "expected": 26}, None),
                ("failed", {"all_passed": False, "compiled": True, "passed": 25, "expected": 26},
                 "".join(f"test m::{n} ... {'FAILED' if k == 0 else 'ok'}\n" for k, n in enumerate(names[:-1]))),
                ("failed-nooutput", {"all_passed": False, "compiled": True, "passed": 25, "expected": 26}, None),
                ("nocompile", {"all_passed": False, "compiled": False, "passed": 0, "expected": 26}, "error[E0425]\n")]):
            tid = f"g3-f{i + 1:03d}-l1"
            tids[case] = tid
            shutil.copytree(t, suite / tid)
            sub = run / "submissions" / "D" / tid
            sub.mkdir(parents=True)
            (sub / "r0.rs").write_text("// x\n")
            if hidden_out is not None:
                (sub / "r0.hidden.txt").write_text(hidden_out)
            if result is not None:
                recs.append({"kind": "score", "arm": "D", "task": tid, "result": result})
        (run / "ledger.jsonl").write_text("".join(json.dumps(r) + "\n" for r in recs))
        out = self.lib(
            "import gym_adjudicate as j, json\n"
            f"tids = {tids!r}\n"
            "for case, tid in tids.items():\n"
            "    e = j.plan_tests({'task': tid, 'run': 1, 'passed': case.startswith('passed'), 'content': 'c', 'kind': 'audit'})\n"
            "    print(case, e['evidence'], len(e['failing']), e['failing'][:1] if e['failing'] == ['*'] else '', len(e['sampled']))\n"
            "e = j.plan_tests({'task': tids['passed'], 'run': 9, 'passed': False, 'content': 'c', 'kind': 'void'})\n"
            "print('void', e['evidence'], len(e['failing']), len(e['sampled']))")
        self.assertEqual(out.splitlines(), [
            "passed True 0  2", "passed-noscore False 0  0", "passed-partial False 0  0",
            "failed True 2  2",                          # the FAILED test and the one never reported
            "failed-nooutput False 0  0", "nocompile True 1 ['*'] 2", "void True 0 2"])

    def test_repairs_keep_clause_ids_and_rubric(self):
        """C-B6: a requirements repair may only reword or extend existing clauses; a new clause id,
        or a change that breaks a mechanical rubric check, is refused (the hidden tests stay)."""
        t = self.fixture_task("depth", 1)
        tid = "g3-f001-l1"
        shutil.copytree(t, self.root / "gym-v3" / tid)
        self.rec.mkdir(parents=True)
        (self.rec / "active.json").write_text(json.dumps({"active": ["g3-f001"], "classes": {"g3-f001": "depth"}}))
        req = json.loads((t / "task.json").read_text())["requirements"]
        rub = json.loads((t / "rubric.json").read_text())
        variants = {"extend": req + " (clarified)", "new clause": req + "\nR21. a brand new clause",
                    "drop a quoted sentence": req.replace(rub["mutants"]["m1"]["refuted_by"], "reworded"),
                    "leak a hidden name": req + " see " + re.findall(r"fn (h_[0-9a-f]+)\(", (t / "hidden_tests.rs").read_text())[0]}
        (self.tmp / "v.json").write_text(json.dumps(variants))
        out = self.lib("import gym_adjudicate as j, json\n"
                       f"v = json.load(open({str(self.tmp / 'v.json')!r}))\n"
                       f"print(json.dumps({{k: j.repair_keeps_rubric('g3-f001', {tid!r}, {req!r}, x) for k, x in v.items()}}))")
        self.assertEqual(json.loads(out), {"extend": True, "new clause": False, "drop a quoted sentence": False,
                                           "leak a hidden name": False})

    def test_band_rules(self):
        out = self.lib(
            "import gym_band as b\nT, F, V = True, False, None\n"
            "print([b.classify(x) for x in ([T, F, F, F], [T, T, T, F], [T, T, T, T], [F, F, F, F],"
            " [T, T, V, F], [T, T, T, V], [F, F, F, V], [V, V, T, T], [V, T, F, F])])\n"
            "e = ['g3-f%03d' % i for i in range(1, 41)]\n"
            "r1, r2 = b.regression_stratum(e, 0.175, 7, 1), b.regression_stratum(e, 0.175, 7, 1)\n"
            "print(len(r1), r1 == r2, set(r1) <= set(e), b.regression_stratum(e, 0.15, 7, 1) != b.regression_stratum(e, 0.15, 8, 1))\n"
            "try:\n    b.regression_stratum(e, 0.3, 7, 1)\n    print('accepted')\nexcept ValueError:\n    print('refused')\n"
            "sp = {'assign': {'g3-f001': {'set': 'dev', 'wave': 1}, 'g3-f002': {'set': 'holdout', 'wave': None},"
            " 'g3-f003': {'set': 'dev', 'wave': 2}}, 'band': {'keep_min': 1, 'keep_max': 3}}\n"
            "asg = lambda f, s, a: s['assign'].get(f)\n"
            "c = {'content': {f'g3-f001-l{i}': 'h' for i in (1, 2, 3, 4)}}\n"
            "tup = {f'g3-f001-l{i}': {'pass': i == 1, 'void': False} for i in (1, 2, 3, 4)}\n"
            "m = {'families': ['g3-f001'], 'content': dict(c['content'])}\n"
            "print(b.decide(sp, {}, 1, m, tup, c, asg))\n"
            "m2 = {'families': ['g3-f001'], 'content': dict(c['content'], **{'g3-f001-l3': 'old'})}\n"
            "print(b.decide(sp, {}, 1, m2, tup, c, asg))\n"
            "for fam, w in (('g3-f002', 1), ('g3-f003', 1)):\n"
            "    try:\n        b.decide(sp, {}, w, {'families': [fam], 'content': {}}, {}, c, asg)\n        print('accepted')\n"
            "    except ValueError:\n        print('refused')\n"
            "print(b.acceptance_rate({'a': {'pass': True, 'void': False}, 'b': {'pass': False, 'void': False},"
            " 'c': {'pass': None, 'void': True}}))")
        lines = out.splitlines()
        self.assertEqual(lines[0], "['band', 'band', 'easy', 'hard', 'band', 'repilot', 'repilot', 'repilot', 'band']")
        self.assertEqual(lines[1], "7 True True True")
        self.assertEqual(lines[2], "refused")
        self.assertEqual(lines[3], "{'g3-f001': 'band'}")
        self.assertEqual(lines[4], "{'g3-f001': 'invalidated'}")
        self.assertEqual(lines[5:7], ["refused", "refused"])                  # holdout, other wave
        self.assertEqual(lines[7], "{'tasks': 3, 'voids': 1, 'passed': 1, 'rate': 0.5, 'in_band': True}")

    def test_pilot_ledgers_match_the_cap_globs(self):
        out = self.lib("import gym_pilot as p\n"
                       "print(p.out_dir(False, 1, 3), p.cap_visible(p.out_dir(False, 1, 3)),"
                       " p.out_dir(True, None, 1), p.cap_visible(p.out_dir(True, None, 1)),"
                       " p.cap_visible('runs/c2-gym3-pilot-w1/a/b/c'), p.cap_visible('runs/gym3/r1'))")
        self.assertEqual(out, "runs/c2-gym3-pilot-w1/r3 True runs/c2-gym3-accept/r1 True False False")
        # the literal globs of nano/eval.py and nano/confirm3.py find it
        runs = self.tmp / "globs" / "runs"
        (runs / "c2-gym3-pilot-w1" / "r3").mkdir(parents=True)
        (runs / "c2-gym3-pilot-w1" / "r3" / "ledger.jsonl").write_text("")
        found = list(runs.glob("c2-*/*/ledger.jsonl")) + list(runs.glob("c2-*/ledger.jsonl")) \
            + list(runs.glob("c2-*/*/*/ledger.jsonl"))
        self.assertEqual([p.relative_to(runs).as_posix() for p in found], ["c2-gym3-pilot-w1/r3/ledger.jsonl"])
        for src in (A2D / "nano" / "eval.py", A2D / "nano" / "confirm3.py"):
            self.assertIn('glob("c2-*/*/ledger.jsonl")', src.read_text())


class Sandbox(Base):
    """Isolation checks and probes under the real seatbelt profile."""

    def test_isolation_checks(self):
        w = self.gym / "g3-f001"
        w.mkdir(parents=True)
        self.rec.mkdir(parents=True, exist_ok=True)
        check = f"from gymlib import isolation_check\nprint(isolation_check({str(w)!r}))"
        self.assertEqual(self.lib(check), "True")
        (w / "AGENTS.md").write_text("x")
        self.assertEqual(self.lib(check), "False")
        (w / "AGENTS.md").unlink()
        self.assertEqual(self.lib(check, env=dict(self.env, GYM_TEST_VERSION="codex-cli 9.9.9")), "False")
        self.assertEqual(self.lib(check, env=dict(self.env, GYM_PINNED_SHA="0000")), "False")

    def test_must_exist_denial_targets(self):
        """Each of the three confirmation-3 targets must exist (a missing one fails the check, it
        is not skipped) and be unreadable from the job."""
        w = self.gym / "g3-f001"
        w.mkdir(parents=True)
        self.rec.mkdir(parents=True, exist_ok=True)
        check = f"from gymlib import isolation_check\nprint(isolation_check({str(w)!r}))"
        for rel in ("confirmation-3", "authoring/confirmation-3/_records", "runs/c2-confirm2-decision"):
            moved = self.tmp / "moved"
            shutil.move(str(self.holdout / rel), str(moved))
            self.assertEqual(self.lib(check), "False", rel)
            shutil.move(str(moved), str(self.holdout / rel))
        (self.holdout / "confirmation-3" / "SECRET.txt").write_text("C3-SECRET\n")
        self.assertEqual(self.lib(check), "True")

    def test_brokers_closed(self):
        w = self.gym / "g3-f001"
        w.mkdir(parents=True)
        self.assertEqual(self.lib(f"from gymlib import _brokers_closed\n"
                                  f"print(_brokers_closed({str(w)!r}, 'GYM-CANARY-test-brokers-0123456789'))"), "True")

    def test_check_sh_compiles_under_job_profile(self):
        w = self.gym / "g3-f001"
        w.mkdir(parents=True)
        for f in ("check.sh", "api_check.sh"):
            shutil.copy(TOOLS / "authoring" / f, w / f)
            (w / f).chmod(0o755)
        for f in ("reference.rs", "public_tests.rs", "hidden_tests.rs"):
            shutil.copy(FIX / f, w / f)
        out = self.lib(
            f"from gymlib import sandboxed\n"
            f"r = sandboxed({str(w)!r}, ['/bin/sh', './check.sh', 'reference.rs', 'hidden_tests.rs'],"
            f" capture_output=True, text=True, timeout=400)\n"
            f"a = sandboxed({str(w)!r}, ['/bin/sh', './api_check.sh', 'reference.rs', 'public_tests.rs'],"
            f" capture_output=True, text=True, timeout=400)\n"
            f"print(r.returncode, a.returncode, 'API_OK' in a.stdout, 'test result: ok' in r.stdout)", timeout=900)
        self.assertEqual(out, "0 0 True True", out)

    def test_run_profile_blocks_signals_outside_the_sandbox(self):
        """Model-written Rust (compiled under compile_profile) that calls kill() on a sibling
        process outside its sandbox fails under run_profile; unsandboxed, the same call succeeds
        (control). The sibling survives a SIGKILL attempt."""
        d = self.tmp / "sig"
        d.mkdir()
        (d / "k.rs").write_text(
            'unsafe extern "C" { fn kill(pid: i32, sig: i32) -> i32; }\n'
            "fn main() {\n    let a: Vec<String> = std::env::args().collect();\n"
            "    let pid: i32 = a[1].parse().unwrap();\n    let sig: i32 = a[2].parse().unwrap();\n"
            "    let r = unsafe { kill(pid, sig) };\n    println!(\"{}\", r);\n}\n")
        self.lib(f"import gymlib\nr = gymlib.sandboxed_rustc(['--edition', '2024', 'k.rs', '-o', 'k'], {str(d)!r})\n"
                 "assert r.returncode == 0, r.stderr")
        sibling = subprocess.Popen(["/bin/sleep", "120"])
        try:
            run = self.tmp / "sigrun"
            run.mkdir()
            ctl = subprocess.run([str(d / "k"), str(sibling.pid), "0"], capture_output=True, text=True)
            out = self.lib("import gymlib, subprocess\n"
                           f"for sig in ('0', '9'):\n"
                           f"    r = subprocess.run(['/usr/bin/sandbox-exec', '-p', gymlib.run_profile({str(d / 'k')!r}, {str(run)!r}),"
                           f" {str(d / 'k')!r}, '{sibling.pid}', sig], capture_output=True, text=True)\n"
                           "    print(r.stdout.strip())")
            self.assertEqual(ctl.stdout.strip(), "0")                        # unsandboxed: the signal is allowed
            self.assertEqual(out.splitlines(), ["-1", "-1"])
            self.assertIsNone(sibling.poll())                                # the sibling was never killed
        finally:
            sibling.kill()
            sibling.wait()

    def test_codex_package_layouts_and_sandboxed_smoke(self):
        """The pin's exact layout (bin/codex, bin/codex-code-mode-host, codex-package.json,
        codex-path/rg, codex-resources/) resolves to its package root, which the job profile allows;
        so does bin/ + codex-path/ without the marker file; a bare bin/ resolves to bin/. A codex that
        reads its own package succeeds in the sandboxed --version smoke only when the root is allowed."""
        def layout(name, files, reads):
            root = self.tmp / name / "0.162.0"
            (root / "bin").mkdir(parents=True)
            for rel in files:
                (root / rel).parent.mkdir(parents=True, exist_ok=True)
                if not rel.endswith("/"):
                    (root / rel).write_text("x\n")
            (root / "bin" / "codex").write_text('#!/bin/sh\nhere=$(cd "$(dirname "$0")" && pwd)\n'
                                                f'cat "$here/../{reads}" > /dev/null || exit 3\necho "codex-cli 0.162.0"\n')
            (root / "bin" / "codex").chmod(0o755)
            return root
        pin = layout("pin", ["bin/codex-code-mode-host", "codex-package.json", "codex-path/rg", "codex-resources/voice/"],
                     "codex-path/rg")
        bare_marker = layout("nomarker", ["codex-path/rg"], "codex-path/rg")
        bare = layout("bare", ["share/x"], "share/x")                       # bin/ alone: no package marker
        w = self.gym / "g3-f001"
        w.mkdir(parents=True)
        self.rec.mkdir(parents=True, exist_ok=True)
        probe = ("import gymlib\nfrom pathlib import Path\n"
                 f"w = Path({str(w)!r})\n"
                 "root = gymlib.codex_package_root()\n"
                 "print(root, f'(subpath \"{root}\")' in gymlib.profile(w, w / 'tag'), gymlib.codex_smoke(w))")
        for root, want_root, smoke in ((pin, pin, "True"), (bare_marker, bare_marker, "True"), (bare, bare / "bin", "False")):
            codex = root / "bin" / "codex"
            sha = hashlib.sha256(codex.read_bytes()).hexdigest()[:16]
            out = self.lib(probe, env=dict(self.env, GYM_CODEX=str(codex), GYM_PINNED_SHA=sha))
            self.assertEqual(out, f"{want_root} True {smoke}", root)
        codex = pin / "bin" / "codex"
        out = self.lib(f"from gymlib import isolation_check\nprint(isolation_check({str(w)!r}))",
                       env=dict(self.env, GYM_CODEX=str(codex), GYM_PINNED_SHA=hashlib.sha256(codex.read_bytes()).hexdigest()[:16]))
        self.assertEqual(out, "True")

    def test_codex_fingerprint_is_the_child_executable(self):
        """The bracket fingerprints the `codex` the a2d child resolves on its PATH (the pinned
        CODEX, its siblings and package files); a CODEX the child cannot reach by name has no
        fingerprint, and a changed package file changes it."""
        pkg = self.tmp / "pkg" / "0.162.0"
        (pkg / "bin").mkdir(parents=True)
        (pkg / "codex-path").mkdir()
        (pkg / "codex-path" / "rg").write_text("rg\n")
        (pkg / "bin" / "codex").write_text('#!/bin/sh\necho "codex-cli 0.162.0"\n')
        (pkg / "bin" / "codex").chmod(0o755)
        (pkg / "bin" / "other").write_text('#!/bin/sh\necho "codex-cli 0.162.0"\n')
        (pkg / "bin" / "other").chmod(0o755)
        fp = "import gymlib, json\nf = gymlib.codex_fingerprint()\nprint(json.dumps(f and [f['realpath'], sorted(f['files']), f['version']]))"
        self.assertEqual(json.loads(self.lib(fp)), [str(FAKE), sorted(p.name for p in HERE.iterdir() if p.is_file()),
                                                    "codex-cli 0.162.0"])
        self.assertEqual(json.loads(self.lib(fp, env=dict(self.env, GYM_CODEX=str(pkg / "bin" / "codex")))),
                         [str(pkg / "bin" / "codex"), ["codex", "codex-path/rg", "other"], "codex-cli 0.162.0"])
        self.assertIsNone(json.loads(self.lib(fp, env=dict(self.env, GYM_CODEX=str(pkg / "bin" / "other")))))

    def test_validator_oracle_on_fixture(self):
        """The sandboxed oracle accepts the fixture task (depth and scale classes) and rejects an
        M1 break and a mutant that passes hidden."""
        ok = self.fixture_task("depth", 3)
        scale = self.fixture_task("scale", 1)
        bad = self.fixture_task("depth", 2, "bad-rubric")
        weak = self.fixture_task("edges", 1)
        shutil.copy(weak / "reference.rs", weak / "mutants" / "m3.rs")
        out = self.lib(
            "import gym_validate as v\nfrom pathlib import Path\n"
            f"for d, c in ((Path({str(ok)!r}), 'depth'), (Path({str(scale)!r}), 'scale'), (Path({str(bad)!r}), 'depth'),"
            f" (Path({str(weak)!r}), 'edges')):\n"
            "    p, counts = v.validate_task(d, c)\n    print(p, counts)", timeout=900)
        lines = out.splitlines()
        self.assertEqual(lines[0], "[] {'mutants': 4, 'caught': 4, 'public_green': 4}")
        self.assertEqual(lines[1], "[] {'mutants': 4, 'caught': 4, 'public_green': 4}")
        self.assertEqual(lines[2], "['rubric M1 failed'] {'mutants': 4, 'caught': 4, 'public_green': 4}")
        self.assertEqual(lines[3], "['mutant m3 passes hidden'] {'mutants': 4, 'caught': 3, 'public_green': 4}")

    def test_validator_rubric_shape_naive_public_and_stdout_timing(self):
        """A malformed rubric is a REJECT line, not an AttributeError (Sol I3, C-B9); the naive
        mutant of a scale task must pass the public tests (C-I6); libtest's summary is read from
        stdout only and the timing takes max(libtest time, wall) (C-I7)."""
        ok = self.fixture_task("depth", 3)
        shapes = {}
        for name, rub in (("list", {"mutants": ["m1"]}), ("scalar", {"mutants": {"m1": "naive"}})):
            d = self.tmp / f"shape-{name}" / ok.name
            shutil.copytree(ok, d)
            r = json.loads((d / "rubric.json").read_text())
            r.update(rub)
            (d / "rubric.json").write_text(json.dumps(r))
            shapes[name] = str(d)
        scale = self.fixture_task("scale", 1)
        slow = self.tmp / "slow-naive" / scale.name
        shutil.copytree(scale, slow)
        (slow / "mutants" / "m4.rs").write_text((FIX / "naive.rs").read_text().replace("customers.len() >= 100", "customers.len() < 100000"))
        forge = ('pub fn sim() -> u32 { 1 }\n')
        tests = ('#[test]\nfn t_forged() {\n    use std::io::Write;\n    std::thread::sleep(std::time::Duration::from_millis(300));\n'
                 '    std::io::stderr().write_all(b"\\ntest result: ok. 99 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; '
                 'finished in 0.00s\\n").unwrap();\n    assert_eq!(sim(), 1);\n}\n')
        out = self.lib(
            "import gym_validate as v, json\nfrom pathlib import Path\n"
            f"shapes = {shapes!r}\n"
            "print(json.dumps({k: [p for p in v.validate_task_safe(Path(d), 'depth')[0] if 'rubric' in p] for k, d in shapes.items()}))\n"
            f"print(v.validate_task_safe(Path({str(slow)!r}), 'scale')[0])\n"
            f"r = v.score_tests({forge!r}, {tests!r}, 'hidden', 10)\n"
            "print(r['all_passed'], r['passed'], r['expected'], r['secs'] >= r['wall_secs'] >= 0.3)", timeout=900)
        a, b, c = out.splitlines()
        self.assertEqual(json.loads(a), {"list": ["rubric.json mutants malformed"],
                                         "scalar": ["rubric.json mutants malformed"]})
        self.assertIn("scale: naive mutant m4 fails the public tests (it must be correct but slow)", b)
        self.assertEqual(c, "True 1 1 True")

    fixture_task = Units.fixture_task


class Flows(Base):
    """End-to-end runs of the drivers with the fake codex and the fake controller."""

    def test_full_flow(self):
        sp, fams, w1 = self.flow()
        # selection: equal class quotas, first OK brief per class in id order, same-class spares
        act = self.active()
        self.assertEqual(fams, ["g3-f001", "g3-f002", "g3-f003", "g3-f004", "g3-f010"])
        self.assertEqual(act["spares"], {"depth": ["g3-f006"], "state": ["g3-f007"], "scale": ["g3-f008"],
                                         "surface": ["g3-f009"], "edges": []})
        # the job sandbox denied the canary
        probe = (self.gym / fams[0] / "probe-result.txt").read_text()
        self.assertNotIn("CANARY-CONTENT", probe)
        self.assertIn("not permitted", probe)
        # sealing: hidden tests and references only under the gym root
        suite = self.root / "gym-v3"
        self.assertEqual(len([p for p in suite.iterdir() if p.is_dir()]), 20)
        self.assertTrue((suite / f"{fams[0]}-l1" / "rubric.json").is_file())
        hidden = set()
        for t in suite.iterdir():
            hidden |= set(re.findall(r"fn (h_[0-9a-f]+)\(", (t / "hidden_tests.rs").read_text()))
        repo_files = [p for p in self.repo.rglob("*") if p.is_file() and ".git" not in p.parts]
        for p in repo_files:
            text = p.read_text(errors="replace")
            self.assertFalse(any(n in text for n in hidden), p)
            self.assertNotIn("pub fn simulate(customers: &[Customer], wake_cost: u64) -> Result<Vec<Event>, Error> {",
                             text, p)                                   # no reference source
        export = self.repo / "runs" / "c2-gym3" / "export"
        holdout_fams = [f for f, a in sp["assign"].items() if a["set"] == "holdout"]
        self.assertEqual(sorted(p.name for p in export.iterdir() if p.is_dir()),
                         sorted(f"{f}-l{i}" for f in fams if f not in holdout_fams for i in (1, 2, 3, 4)))
        self.assertEqual({p.name for p in export.rglob("*") if p.is_file()}, {"task.json", "public_tests.rs", "hashes.json"})
        self.assertEqual(len(json.loads((export / "hashes.json").read_text())["tasks"]), 20)
        # never filter on solvability: the solver failure, judged DETERMINED by both, stays
        adj = json.loads((self.rec / "adjudication.json").read_text())
        failing = adj[f"{fams[0]}-l2"]["r1"]
        self.assertEqual(([v["kind"] for v in failing["tests"].values()].count("failing"),
                          failing["excluded"], failing["repaired"]), (1, False, False))
        self.assertTrue((suite / f"{fams[0]}-l2").is_dir())
        # M4: every audited task got a spec review of 2 passed hidden tests
        self.assertTrue(all(len(r["m4"]) == 2 for t in adj.values() for r in t.values()), adj)
        self.assertEqual(len(adj), 20)
        jobs = json.loads((self.rec / "adj-jobs.json").read_text())
        self.assertEqual(sorted(k for k in jobs if k.endswith("-A")), [f"{fams[0]}-repair-A"])
        self.assertEqual(len([k for k in jobs if k.endswith("-B")]), 6)       # 5 repair units + 1 classify
        # the audit ran on the gym's codex homes and never saw A2D_HOLDOUT_DIR
        cert = json.loads((self.rec / "certified.json").read_text())
        self.assertEqual(len(cert["content"]), 20)
        # pilot: ledger in the scratch repo at the cap depth, registered, holdout never piloted
        reg = json.loads((self.repo / "runs" / "c2-gym3" / "replicates.json").read_text())
        self.assertEqual(sorted(reg), ["runs/c2-gym3-accept/r1", "runs/c2-gym3-pilot-w1/r1"])
        self.assertTrue((self.repo / "runs" / "c2-gym3-pilot-w1" / "r1" / "ledger.jsonl").is_file())
        man = json.loads((self.repo / "runs" / "c2-gym3-pilot-w1" / "r1.manifest.json").read_text())
        self.assertEqual(man["families"], w1)
        self.assertFalse(set(man["families"]) & set(holdout_fams))
        band = json.loads((self.repo / "runs" / "c2-gym3" / "band-w1.json").read_text())
        self.assertEqual(band["decisions"], {w1[0]: "band", w1[1]: "easy", w1[2]: "hard", w1[3]: "easy"})
        self.assertEqual((band["kept"], band["regression"], band["dropped"]), ([w1[0]], [], [w1[1], w1[2], w1[3]]))
        acc = json.loads((self.repo / "runs" / "c2-gym3" / "acceptance.json").read_text())
        self.assertEqual((acc["tasks"], acc["decision"]), (4, "adjust-next-wave"))      # 4/4 pass: 100%
        self.assertTrue(reg["runs/c2-gym3-accept/r1"]["threshold"])
        # an acceptance (A/A threshold) replicate can never feed a selection; nor a second acceptance
        code, lines = self.run_script("gym_band.py", "band", "--wave", "1", "--run", "runs/c2-gym3-accept/r1")
        self.assertEqual((code, lines[-1].get("error")), (1, "valueerror"))
        code, lines = self.run_script("gym_pilot.py", "--acceptance", "--binary", str(FAKE_A2D))
        self.assertIn("acceptance-exists", self.statuses(lines))
        # nothing reached the real repo's runs/
        self.assertEqual(list((A2D / "runs").glob("c2-gym3*")), [])
        self.run_script("gym_author.py", "--stop")
        code, lines = self.run_script("gym_author.py", "--drained")
        self.assertTrue(lines[-1]["ok"])

    def test_m3_disagreement_repairs_requirements_then_classify_excludes(self):
        """A and B disagree (or a quote is invalid): the task gets a requirements-only repair, its
        hidden tests unchanged; it is re-audited; failing again and still not determined, it is
        excluded and its family replaced by a same-class spare in the same split slot."""
        sp = self.select_and_split()
        f = (self.gym / "FAMILIES.txt").read_text().split()[0]
        self.run_script("gym_author.py", "--only", f)
        self.settle()
        out = self.gym / f / "out" / f
        hidden_before = (out / f"{f}-l2" / "hidden_tests.rs").read_bytes()
        code, lines = self.audit(GYM_TEST_AUDIT_FAIL=f"{f}-l2 {f}-l3")
        self.assertEqual(code, 0, lines)
        code, lines = self.run_script("gym_adjudicate.py", env=dict(self.env, GYM_TEST_ADJ_SPLIT=f"{f}-l2",
                                                                     GYM_TEST_ADJ_BADQUOTE=f"{f}-l3"))
        self.assertEqual(code, 0, lines)
        adj = json.loads((self.rec / "adjudication.json").read_text())
        self.assertEqual((adj[f"{f}-l2"]["r1"]["repaired"], adj[f"{f}-l3"]["r1"]["repaired"]), (True, True))
        self.assertIn("(clarified)", json.loads((out / f"{f}-l2" / "task.json").read_text())["requirements"])
        self.assertEqual((out / f"{f}-l2" / "hidden_tests.rs").read_bytes(), hidden_before)
        code, lines = self.audit()
        self.assertIn("suite-not-clean", self.statuses(lines))                # the suite changed: settle first
        self.settle()
        code, lines = self.audit(GYM_TEST_AUDIT_FAIL=f"{f}-l2")
        self.assertEqual(lines[0]["tasks"], 2, lines)                          # only the repaired tasks
        code, lines = self.run_script("gym_adjudicate.py", env=dict(self.env, GYM_TEST_ADJ_UNDER=f"{f}-l2"))
        adj = json.loads((self.rec / "adjudication.json").read_text())
        self.assertTrue(adj[f"{f}-l2"]["r2"]["excluded"])
        # passed its re-audit: the repaired content still gets its own M4 spec review (B12)
        self.assertEqual((len(adj[f"{f}-l3"]["r2"]["m4"]), adj[f"{f}-l3"]["r2"]["excluded"]), (2, False))
        code, lines = self.run_script("gym_assemble.py")
        self.assertIn({"stage": "assemble-exclude", "id": f, "status": "replaced"}, lines)
        act = self.active()
        spare = act["events"][-1]["replacement"]
        self.assertEqual(act["classes"][spare], act["classes"][f])
        self.assertEqual(self.lib(f"import gym_split\nprint(gym_split.assignment({spare!r}, gym_split.load_split())"
                                  f" == gym_split.load_split()['assign'][{f!r}])"), "True")
        excluded = json.loads((self.rec / "excluded.json").read_text())
        self.assertEqual(excluded["reasons"][f], "adjudication-underdetermined")

    def test_validation_rejects_rubric_break_and_fix_repairs_it(self):
        self.select_and_split()
        f = (self.gym / "FAMILIES.txt").read_text().split()[0]
        self.run_script("gym_author.py", "--only", f, env=dict(self.env, GYM_TEST_BADRUBRIC=f))
        self.assertEqual(self.run_script("gym_assemble.py")[0], 0)
        code, lines = self.run_script("gym_validate.py")
        self.assertEqual(code, 0, lines)
        self.assertEqual(sorted(x["id"] for x in lines if x.get("stage") == "validate-reject"),
                         [f"{f}-l{i}" for i in (1, 2, 3, 4)])
        self.assertIn("rubric M1 failed", (self.gym / f / "FIX-VERDICTS.txt").read_text())
        code, lines = self.run_script("gym_fix.py")
        self.assertTrue([x for x in lines if x.get("stage") == "fix" and x.get("id") == f][0]["ok"])
        self.settle()
        val = json.loads((self.rec / "validate.json").read_text())
        self.assertEqual((val["rejected"], val["sandboxed"]), ([], True))
        # passing verdicts are cached by content hash (+ class + validator code): unchanged tasks
        # are not re-timed on the next validation (C-B7)
        cache = json.loads((self.rec / "validate-cache.json").read_text())
        self.assertEqual(len([k for k in cache if k.endswith(":" + val["oracle_sha256"])]), 4)
        code, lines = self.run_script("gym_validate.py")
        self.assertEqual((code, json.loads((self.rec / "validate-cache.json").read_text())), (0, cache))

    def test_fix_never_edits_hidden_tests_after_audit(self):
        """C-B6: once a family has an audit round, a FIX that changes a hidden_tests.rs fails and
        the verified files are restored."""
        self.select_and_split()
        f = (self.gym / "FAMILIES.txt").read_text().split()[0]
        self.run_script("gym_author.py", "--only", f, env=dict(self.env, GYM_TEST_BADRUBRIC=f))
        self.assertEqual(self.run_script("gym_assemble.py")[0], 0)
        self.assertEqual(self.run_script("gym_validate.py")[0], 0)
        out = self.gym / f / "out" / f
        before = {lv: (out / f"{f}-l{lv}" / "hidden_tests.rs").read_bytes() for lv in (1, 2, 3, 4)}
        (self.rec / "audit.json").write_text(json.dumps({f"{f}-l1": {"r1": {"content": "x", "void": True, "passed": False, "run": 1}}}))
        code, lines = self.run_script("gym_fix.py")
        self.assertFalse([x for x in lines if x.get("stage") == "fix" and x.get("id") == f][0]["ok"], lines)
        self.assertEqual({lv: (out / f"{f}-l{lv}" / "hidden_tests.rs").read_bytes() for lv in (1, 2, 3, 4)}, before)

    def test_similarity_flags_only_the_unaudited_family(self):
        """C-B7: on a gym-gym overlap, only the family that has not been audited is re-authored."""
        self.select_and_split()
        f1, f2 = (self.gym / "FAMILIES.txt").read_text().split()[:2]
        for f in (f1, f2):
            self.run_script("gym_author.py", "--only", f, env=dict(self.env, GYM_TEST_DUP=f"{f1} {f2}"))
        self.assertEqual(self.run_script("gym_assemble.py")[0], 0)
        (self.rec / "audit.json").write_text(json.dumps({f"{f1}-l1": {"r1": {"content": "x", "void": False, "passed": True, "run": 1}}}))
        code, lines = self.run_script("gym_similarity.py")
        self.assertEqual(code, 0, lines)
        self.assertEqual(json.loads((self.rec / "similarity.json").read_text()), {f2: "reauthor"})

    def test_audit_codex_bracket_voids_a_changed_package(self):
        """I4/C-I8: the audit fingerprints the codex the a2d child resolves on its PATH, before and
        after; a package file changing during the run voids every tuple of the run."""
        self.select_and_split()
        f = (self.gym / "FAMILIES.txt").read_text().split()[0]
        self.run_script("gym_author.py", "--only", f)
        self.settle()
        pkg = self.tmp / "pkg" / "0.162.0"
        (pkg / "bin").mkdir(parents=True)
        (pkg / "codex-path").mkdir()
        (pkg / "codex-path" / "rg").write_text("rg\n")
        (pkg / "bin" / "codex").write_text('#!/bin/sh\necho "codex-cli 0.162.0"\n')
        (pkg / "bin" / "codex").chmod(0o755)
        env = dict(GYM_CODEX=str(pkg / "bin" / "codex"))
        code, lines = self.audit(**env, GYM_TEST_A2D_TOUCH=str(pkg / "codex-path" / "rg"))
        self.assertEqual((code, lines[-1]["skipped"]), (0, 4), lines)               # all 4 void
        runs = json.loads((self.rec / "audit-runs.json").read_text())["runs"]["1"]
        self.assertEqual((runs["bracket_ok"], runs["codex"]["realpath"]), (False, str(pkg / "bin" / "codex")))
        code, lines = self.audit(**env)
        self.assertEqual((code, lines[-1]["skipped"], lines[-1]["tasks"]), (0, 0, 4), lines)
        os.rename(pkg / "bin" / "codex", pkg / "bin" / "codex-moved")
        (self.rec / "audit.json").write_text("{}")
        code, lines = self.audit(GYM_CODEX=str(pkg / "bin" / "codex-moved"))
        self.assertEqual((code, lines[-1]["status"]), (2, "codex-fingerprint-missing"), lines)

    def test_pilot_refusals_and_seal_check(self):
        sp = self.select_and_split()
        self.run_script("gym_author.py", "--parallel", "3")
        self.settle()
        self.audit()
        self.run_script("gym_adjudicate.py")
        self.assertEqual(self.run_script("gym_export.py", "certify")[0], 0)
        # a split that is not committed blocks any pilot
        split = self.repo / "runs" / "c2-gym3" / "split.json"
        text = split.read_text()
        split.write_text(text.replace('"seed": 7', '"seed": 8'))
        code, lines = self.run_script("gym_pilot.py", "--wave", "1", "--binary", str(FAKE_A2D))
        self.assertEqual((code, lines[-1]["status"]), (2, "split-not-committed"))
        split.write_text(text)
        # a harness other than E007 is refused
        other = self.tmp / "other.py"
        other.write_text("# lanes: judge=codex/gpt-6.1-sol:medium\n")
        code, lines = self.run_script("gym_pilot.py", "--wave", "1", "--binary", str(FAKE_A2D),
                                      env=dict(self.env, GYM_TEST_HARNESS=str(other)))
        self.assertEqual((code, lines[-1]["status"]), (2, "harness-not-e007"))
        # a hidden test name in the run's out dir is a seal breach: the run is not usable
        code, lines = self.run_script("gym_pilot.py", "--wave", "1", "--binary", str(FAKE_A2D),
                                      env=dict(self.env, GYM_TEST_LEAK="1"))
        self.assertEqual((code, lines[-1]["status"]), (1, "seal-breach"))
        reg = json.loads((self.repo / "runs" / "c2-gym3" / "replicates.json").read_text())
        self.assertEqual(reg["runs/c2-gym3-pilot-w1/r1"]["status"], "failed")
        code, lines = self.run_script("gym_band.py", "band", "--wave", "1", "--run", "runs/c2-gym3-pilot-w1/r1")
        self.assertEqual(code, 1)
        # two voids in a family: re-pilot; then a clean re-pilot decides it
        w1 = sorted(f for f, a in sp["assign"].items() if a["wave"] == 1)
        code, lines = self.run_script("gym_pilot.py", "--wave", "1", "--binary", str(FAKE_A2D),
                                      env=dict(self.env, GYM_TEST_PILOT_VOID=f"{w1[0]}-l1 {w1[0]}-l2",
                                               GYM_TEST_PILOT_FAIL=f"{w1[1]}-l1"))
        self.assertEqual(code, 0, lines)
        code, lines = self.run_script("gym_band.py", "band", "--wave", "1", "--run", "runs/c2-gym3-pilot-w1/r2")
        self.assertEqual((lines[-1]["repilot"], lines[-1]["status"]), (1, "pending"), lines)
        code, lines = self.run_script("gym_pilot.py", "--wave", "1", "--repilot", "--binary", str(FAKE_A2D),
                                      env=dict(self.env, GYM_TEST_PILOT_FAIL=f"{w1[0]}-l4"))
        self.assertEqual((code, lines[-1]["families"]), (0, 1), lines)
        code, lines = self.run_script("gym_band.py", "band", "--wave", "1", "--run", "runs/c2-gym3-pilot-w1/r3")
        band = json.loads((self.repo / "runs" / "c2-gym3" / "band-w1.json").read_text())
        self.assertTrue(band["finalized"])
        self.assertEqual(band["decisions"][w1[0]], "band")
        # the dev-holdout is never piloted, even when asked for by wave
        hold = [f for f, a in sp["assign"].items() if a["set"] == "holdout"][0]
        self.assertNotIn(hold, json.dumps(band))
        # wave 2 has a family but no band decision: the write-once acceptance replicate is refused
        code, lines = self.run_script("gym_pilot.py", "--acceptance", "--binary", str(FAKE_A2D))
        self.assertEqual((code, lines[-1]["status"]), (2, "acceptance-not-ready"))
        self.assertNotIn("runs/c2-gym3-accept/r1", json.loads((self.repo / "runs" / "c2-gym3" / "replicates.json").read_text()))


class Acceptance(Base):
    """Read-only check on the real confirmation-3 state: identical before and after the full flow."""

    @staticmethod
    def records_digest(root):
        """sha256 over the sorted file list and contents (symlinks by target, other non-regular
        entries by type); traversal errors are folded in. Nothing is returned but the digest."""
        h = hashlib.sha256()
        for dirpath, dirs, files in os.walk(root, followlinks=False, onerror=lambda e: h.update(b"walk-error")):
            dirs.sort()
            for name in sorted(files + [d for d in dirs if os.path.islink(os.path.join(dirpath, d))]):
                p = os.path.join(dirpath, name)
                h.update(os.path.relpath(p, root).encode() + b"\0")
                try:
                    st = os.lstat(p)
                    if stat.S_ISLNK(st.st_mode):
                        h.update(b"L" + os.readlink(p).encode())
                    elif stat.S_ISREG(st.st_mode):
                        with open(p, "rb") as f:
                            h.update(hashlib.sha256(f.read()).digest())
                    else:
                        h.update(b"T%d" % stat.S_IFMT(st.st_mode))
                except OSError:
                    h.update(b"unreadable")
        return h.hexdigest()

    def drained_last_line(self):
        r = subprocess.run([sys.executable, "-I", str(A2D / "tools" / "holdout" / "c3_author.py"), "--drained"],
                           capture_output=True, text=True, env=clean_env(), timeout=600)
        lines = [x for x in r.stdout.splitlines() if x.strip()]
        return (r.returncode, lines[-1] if lines else None)

    def test_confirmation3_state_unchanged_by_gym_flow(self):
        if not REAL_HOLDOUT.exists():
            self.skipTest("~/.a2d-holdout does not exist: the read-only acceptance check is skipped")
        records = REAL_HOLDOUT / "authoring" / "confirmation-3" / "_records"
        before = (self.records_digest(records), self.drained_last_line())
        self.flow()
        after = (self.records_digest(records), self.drained_last_line())
        same_digest, same_drained = before[0] == after[0], before[1] == after[1]
        print(f"\nacceptance: _records digest identical: {same_digest}; c3_author --drained last line identical: "
              f"{same_drained}")
        self.assertTrue(same_digest)
        self.assertTrue(same_drained)


if __name__ == "__main__":
    unittest.main()
