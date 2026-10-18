"""Tests for the confirmation-3 authoring pipeline, on fixtures with a fake codex.

Every script runs as a subprocess with A2D_HOLDOUT_DIR pointing at a fresh temp dir and
C3_CODEX pointing at tests/fake_codex.sh, under the real seatbelt profile. Covered: the
output gateway (no fixture content, names or model text on stdout/stderr), isolation
checks and probes, the sandbox (canary read denied from inside the job), selection
(write-once, strict), authoring caps and verified completion, symlink refusal in
assembly, similarity conflict -> re-author -> spare replacement, strict validation
parsing, and the fail-closed drain.

usage: python3 -I -m unittest discover -s tools/holdout/tests -v
"""
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
TOOLS = HERE.parent
FAKE = HERE / "fake_codex.sh"
SECRETS = ("SECRET", "secret-family-name", "Brief ", "hosts")


class Pipeline(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="c3test-")).resolve()
        self.holdout = self.tmp / "holdout"
        self.holdout.mkdir()
        (self.tmp / "auth.json").write_text("{}")
        self.canary = self.tmp / "canary.txt"
        sha = hashlib.sha256(FAKE.read_bytes()).hexdigest()[:16]
        self.env = dict(os.environ, A2D_HOLDOUT_DIR=str(self.holdout), C3_CODEX=str(FAKE),
                        C3_AUTH=str(self.tmp / "auth.json"), C3_CANARY=str(self.canary),
                        C3_PINNED_SHA=sha, C3_TEST_SKIP_PREREQS="1")
        self.c3 = self.holdout / "authoring" / "confirmation-3"

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def run_script(self, name, *args, env=None):
        r = subprocess.run([sys.executable, "-I", str(TOOLS / name), *args], capture_output=True, text=True,
                           env=env or self.env, timeout=900)
        for x in (r.stdout + r.stderr).splitlines():
            for s in SECRETS:
                self.assertNotIn(s, x, f"{name} leaked content")
        self.assertEqual(r.stderr.strip(), "", f"{name} wrote to stderr")
        return r.returncode, [json.loads(x) for x in r.stdout.splitlines() if x.strip()]

    def lib(self, code, timeout=300):
        r = subprocess.run([sys.executable, "-I", "-c", f"import sys; sys.path.insert(0, {str(TOOLS)!r})\n{code}"],
                           capture_output=True, text=True, env=self.env, timeout=timeout)
        self.assertEqual(r.returncode, 0, r.stderr[-2000:])
        return r.stdout.strip()

    def active(self):
        return json.loads((self.c3 / "_records" / "active.json").read_text())

    # ------------------------------------------------------------------ units

    def test_emit_redacts(self):
        out = self.lib("from c3lib import emit\nemit(stage='x', id='c3-f01', count=3, error='Has Spaces!', bogus=1)\n"
                       "emit(stage='y', id='secret-family-name', status='a' * 60)")
        a, b = [json.loads(x) for x in out.splitlines()]
        self.assertEqual((a["stage"], a["id"], a["count"], a["error"]), ("x", "c3-f01", 3, "redacted"))
        self.assertNotIn("bogus", a)
        self.assertEqual((b["id"], b["status"]), ("redacted", "redacted"))

    def test_isolation_checks(self):
        w = self.c3 / "c3-f01"
        w.mkdir(parents=True)
        check = f"from c3lib import isolation_check\nprint(isolation_check({str(w)!r}))"
        self.assertEqual(self.lib(check), "True")
        (w / "AGENTS.md").write_text("x")
        self.assertEqual(self.lib(check), "False")
        (w / "AGENTS.md").unlink()
        self.env["C3_TEST_VERSION"] = "codex-cli 9.9.9"
        self.assertEqual(self.lib(check), "False")
        del self.env["C3_TEST_VERSION"]
        self.env["C3_PINNED_SHA"] = "0000"
        self.assertEqual(self.lib(check), "False")

    def test_symlink_refused(self):
        w = self.c3 / "c3-f01"
        w.mkdir(parents=True)
        (w / "x").symlink_to("/etc/hosts")
        out = self.lib("from c3lib import safe_read_text\n"
                       f"try:\n    safe_read_text({str(w / 'x')!r})\n    print('READ')\nexcept OSError:\n    print('REFUSED')")
        self.assertEqual(out, "REFUSED")

    def test_drained_fails_closed(self):
        self.c3.mkdir(parents=True)
        drained = "from c3lib import drained\nprint(drained())"
        self.assertEqual(self.lib(drained), "False")            # no STOP
        (self.c3 / "STOP").write_text("stop")
        self.assertEqual(self.lib(drained), "False")            # no pid record
        (self.c3 / "_records").mkdir()
        live = subprocess.Popen(["/bin/sleep", "60"], start_new_session=True)
        start = self.lib(f"from c3lib import _proc_start\nprint(_proc_start({live.pid}))")
        (self.c3 / "_records" / "pids.txt").write_text(f"{live.pid} {start}\n")
        self.assertEqual(self.lib(drained), "False")            # a live group
        (self.c3 / "_records" / "pids.txt").write_text(f"{live.pid} 1\n")
        self.assertEqual(self.lib(drained), "True")             # same id, other start time: reused, not ours
        live.kill()
        live.wait()
        dead = subprocess.Popen(["/usr/bin/true"], start_new_session=True)
        dead.wait()
        (self.c3 / "_records" / "pids.txt").write_text(f"{dead.pid} 1\n")
        self.assertEqual(self.lib(drained), "True")

    def test_unregistered_launcher_exits_on_driver_death(self):
        """The handshake launcher never execs if the driver dies before saying go."""
        marker = self.tmp / "ran"
        out = self.lib("import os, subprocess\nfrom c3lib import _HANDSHAKE\n"
                       "r, w = os.pipe()\n"
                       f"p = subprocess.Popen(['/bin/sh', '-c', _HANDSHAKE, '/dev/null', '/usr/bin/touch', {str(marker)!r}],"
                       " stdin=r)\n"
                       "os.close(r)\nos.close(w)\nprint(p.wait())")
        self.assertEqual(out, "125")
        self.assertFalse(marker.exists())

    def test_recovery_kills_recorded_launcher_groups(self):
        """A dead driver's launcher group is killed before the next driver proceeds; a
        recorded id whose live leader has another start time is never signalled."""
        rec = self.c3 / "_records"
        rec.mkdir(parents=True)
        # the leader records its start time, then leaves an orphan in its group and exits
        # (as a launcher's group looks after its driver died: no parent of ours to reap it)
        leader = subprocess.Popen(["/bin/sh", "-c", "/bin/sleep 300 </dev/null >/dev/null 2>&1 & echo $!; read x"],
                                  start_new_session=True, stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
        sleeper = int(leader.stdout.readline())
        start = self.lib(f"from c3lib import _proc_start\nprint(_proc_start({leader.pid}))")
        leader.stdin.close()
        leader.wait()
        bystander = subprocess.Popen(["/bin/sleep", "300"], start_new_session=True)
        (rec / "pids.txt").write_text(f"{leader.pid} {start}\n{bystander.pid} 1\n")
        os.kill(sleeper, 0)                                          # alive before recovery
        self.lib("from c3lib import DriverLock\nwith DriverLock():\n    pass")
        with self.assertRaises(ProcessLookupError):
            os.kill(sleeper, 0)
        self.assertIsNone(bystander.poll())
        bystander.kill()
        bystander.wait()
        self.assertEqual((rec / "pids.txt").read_text(), "")

    def test_driver_lock_is_exclusive(self):
        out = self.lib("import subprocess, sys\n"
                       "from c3lib import DriverLock\n"
                       "with DriverLock():\n"
                       "    try:\n"
                       "        with DriverLock():\n"
                       "            print('SECOND')\n"
                       "    except RuntimeError:\n"
                       "        print('REFUSED')")
        self.assertEqual(out, "REFUSED")

    def test_stop_gate_refuses_launch(self):
        out = self.lib("from c3lib import StopRequested, launch, request_stop\n"
                       "request_stop()\n"
                       "try:\n    launch('/dev/null', ['/usr/bin/true'])\n    print('LAUNCHED')\n"
                       "except StopRequested:\n    print('REFUSED')")
        self.assertEqual(out, "REFUSED")

    def test_brokers_closed(self):
        """Copied open/launchctl/pbpaste/mdfind and a local TCP listener are unreachable."""
        w = self.c3 / "c3-f01"
        w.mkdir(parents=True)
        self.assertEqual(self.lib(f"from c3lib import _brokers_closed\n"
                                  f"print(_brokers_closed({str(w)!r}, 'C3-CANARY-test-brokers-0123456789'))"), "True")

    def test_inventory_traversal_error_fails_selection(self):
        bad = self.holdout / "old-set" / "unreadable"
        bad.mkdir(parents=True)
        bad.chmod(0)
        try:
            code, lines = self.run_script("c3_select.py")
        finally:
            bad.chmod(0o755)
        self.assertNotEqual(code, 0)
        self.assertEqual(lines[-1].get("error"), "permissionerror", lines)
        self.assertFalse((self.c3 / "_records" / "prior_manifest.json").exists())
        self.assertFalse((self.c3 / "_select").exists())

    def test_check_sh_compiles_under_sandbox(self):
        """The real toolchain must compile, link and run tests inside the job profile."""
        w = self.c3 / "c3-f01"
        w.mkdir(parents=True)
        for f in ("check.sh", "api_check.sh"):
            shutil.copy(TOOLS / "authoring" / f, w / f)
            (w / f).chmod(0o755)
        for f in ("reference.rs", "public_tests.rs", "hidden_tests.rs"):
            shutil.copy(HERE / "fixture" / f, w / f)
        out = self.lib(
            f"from c3lib import sandboxed\n"
            f"r = sandboxed({str(w)!r}, ['/bin/sh', './check.sh', 'reference.rs', 'hidden_tests.rs'],"
            f" capture_output=True, text=True, timeout=400)\n"
            f"a = sandboxed({str(w)!r}, ['/bin/sh', './api_check.sh', 'reference.rs', 'public_tests.rs'],"
            f" capture_output=True, text=True, timeout=400)\n"
            f"print(r.returncode, a.returncode, 'API_OK' in a.stdout, 'test result: ok' in r.stdout)", timeout=900)
        self.assertEqual(out, "0 0 True True", out)

    # ------------------------------------------------------------------- flows

    def select(self):
        code, lines = self.run_script("c3_select.py")
        self.assertEqual(code, 0, lines)
        return lines

    def test_full_flow(self):
        lines = self.select()
        nc = [x for x in lines if x.get("stage") == "namecheck" and "conflicts" in x][0]
        self.assertEqual((nc["conflicts"], nc["count"]), (1, 39))
        fams = (self.c3 / "FAMILIES.txt").read_text().split()
        self.assertEqual(len(fams), 36)
        self.assertNotIn("c3-f05", fams)
        self.assertEqual(self.active()["spares"], ["c3-f38", "c3-f39", "c3-f40"])
        code, lines = self.run_script("c3_select.py")          # write-once
        self.assertNotEqual(code, 0)

        self.canary.write_text("CANARY-CONTENT\n")
        env = dict(self.env, C3_TEST_PROBE=str(self.canary))
        code, lines = self.run_script("c3_author.py", "--only", fams[0], env=env)
        self.assertEqual(code, 0, lines)
        probe = (self.c3 / fams[0] / "probe-result.txt").read_text()
        self.assertNotIn("CANARY-CONTENT", probe)
        self.assertIn("not permitted", probe)

        code, lines = self.run_script("c3_author.py", "--parallel", "3")
        self.assertEqual(code, 0, lines)
        self.assertEqual(lines[-1]["done"], 36)
        code, lines = self.run_script("c3_assemble.py")
        self.assertEqual((code, lines[-1]["families"], lines[-1]["tasks"]), (0, 36, 144), lines)
        task = self.holdout / "confirmation-3" / f"{fams[0]}-l1"
        self.assertTrue((task / "hidden_tests.rs").is_file())
        self.assertFalse((task / "REPORT.md").exists())
        code, lines = self.run_script("c3_similarity.py")
        self.assertEqual((code, lines[0]["tasks"], lines[0]["conflicts"]), (0, 144, 0), lines)

        code, lines = self.run_script("c3_author.py", "--drained")
        self.assertFalse(lines[-1]["ok"])                        # no STOP yet
        self.run_script("c3_author.py", "--stop")
        code, lines = self.run_script("c3_author.py", "--drained")
        self.assertTrue(lines[-1]["ok"])
        code, lines = self.run_script("c3_author.py", "--parallel", "3")
        self.assertEqual(lines[-1]["done"], 36)                  # STOP: nothing new launched

    def test_reset_retires_the_selection_record(self):
        env = dict(self.env, C3_TEST_CONFLICTS="5")              # 35 OK < 36: short, reselect allowed
        code, lines = self.run_script("c3_select.py", env=env)
        self.assertNotEqual(code, 0)
        self.assertIn("short-reselect-allowed", [x.get("status") for x in lines])
        rec = self.c3 / "_records"
        old = (rec / "select.json").read_text()
        code, lines = self.run_script("c3_select.py", "--reset", env=dict(env, C3_TEST_FAILSELECT="1"))
        self.assertNotEqual(code, 0)                              # generation 1's selection job failed
        self.assertFalse((rec / "select.json").exists())          # generation 0's record was retired
        # a stale generation-0 success record next to its own briefs is never accepted
        shutil.rmtree(self.c3 / "_select")
        shutil.copytree(self.c3 / "_select-failed1", self.c3 / "_select")
        (rec / "select.json").write_text(old)
        code, lines = self.run_script("c3_select.py")
        self.assertNotEqual(code, 0)
        self.assertFalse((self.c3 / "FAMILIES.txt").exists())

    def test_orphan_is_reaped_before_artifacts_are_read(self):
        self.select()
        fid = (self.c3 / "FAMILIES.txt").read_text().split()[0]
        code, lines = self.run_script("c3_author.py", "--only", fid, env=dict(self.env, C3_TEST_ORPHAN="8641"))
        self.assertEqual(code, 0, lines)
        alive = subprocess.run(["/usr/bin/pgrep", "-f", "sleep 8641"], capture_output=True, text=True).stdout.split()
        self.assertEqual(alive, [], "a detached job process survived")
        self.assertTrue([x for x in lines if x.get("stage") == "author" and x.get("id") == fid][0]["ok"])

    def test_exhausted_family_is_replaced_at_assembly(self):
        self.select()
        fams = (self.c3 / "FAMILIES.txt").read_text().split()
        env = dict(self.env, C3_TEST_FAILFID=fams[0])
        for _ in range(2):
            self.run_script("c3_author.py", "--only", fams[0], env=env)
        code, lines = self.run_script("c3_author.py", "--only", fams[0], env=env)
        self.assertIn("cap-exhausted", [x.get("status") for x in lines])
        code, lines = self.run_script("c3_assemble.py")
        self.assertIn({"stage": "assemble-exhausted", "id": fams[0], "status": "replaced"}, lines)
        self.assertNotIn(fams[0], self.active()["active"])
        self.assertEqual(self.active()["active"][0], "c3-f38")

    def test_symlinked_artifact_is_not_assembled(self):
        self.select()
        fams = (self.c3 / "FAMILIES.txt").read_text().split()
        env = dict(self.env, C3_TEST_SYMLINK=fams[0])
        code, lines = self.run_script("c3_author.py", "--only", fams[0], env=env)
        ev = [x for x in lines if x.get("id") == fams[0]][0]
        self.assertFalse(ev["ok"])                               # completion check refuses the symlink
        self.run_script("c3_author.py", "--parallel", "3", "--only", fams[1])
        code, lines = self.run_script("c3_assemble.py")
        self.assertFalse((self.holdout / "confirmation-3" / f"{fams[0]}-l1").exists())

    def test_similarity_conflict_reauthor_then_spare(self):
        self.select()
        fams = (self.c3 / "FAMILIES.txt").read_text().split()
        dup = f"{fams[0]} {fams[1]}"
        env = dict(self.env, C3_TEST_DUP=dup)
        code, lines = self.run_script("c3_author.py", "--parallel", "3", env=env)
        self.assertEqual((code, lines[-1]["done"]), (0, 36), lines)
        code, lines = self.run_script("c3_assemble.py")
        self.assertEqual(lines[-1]["families"], 36, lines)
        code, lines = self.run_script("c3_similarity.py")
        self.assertEqual(lines[0]["conflicts"], 2, lines)
        sim = json.loads((self.c3 / "_records" / "similarity.json").read_text())
        self.assertEqual((sim[fams[0]], sim[fams[1]]), ("reauthor", "reauthor"))
        code, lines = self.run_script("c3_similarity.py")              # re-run before re-authoring
        self.assertEqual({x["status"] for x in lines if x.get("stage") == "similarity-conflict"}, {"pending"})
        self.assertIn(fams[0], self.active()["active"])
        for f in fams[:2]:
            code, lines = self.run_script("c3_author.py", "--reauthor", f, env=env)
            self.assertEqual(code, 0, lines)
        code, lines = self.run_script("c3_similarity.py")              # suite still holds the old version
        self.assertEqual({x["status"] for x in lines if x.get("stage") == "similarity-conflict"}, {"pending"})
        self.assertIn(fams[0], self.active()["active"])
        self.run_script("c3_assemble.py")
        self.run_script("c3_similarity.py")
        act = self.active()
        self.assertNotIn(fams[0], act["active"])
        self.assertIn("c3-f38", act["active"])
        self.assertIn("c3-f39", act["active"])
        self.assertEqual(len(act["active"]), 36)

    def test_validate_strict_parse(self):
        self.select()
        fams = (self.c3 / "FAMILIES.txt").read_text().split()
        self.run_script("c3_author.py", "--parallel", "3", "--only", fams[0])
        self.run_script("c3_assemble.py")
        fake = self.tmp / "fake_a2d.sh"
        fake.write_text("#!/bin/sh\n" + "".join(f"echo '{fams[0]}-l{i}: OK (x)'\n" for i in (1, 2, 3)))
        fake.chmod(0o755)
        code, lines = self.run_script("c3_validate.py", "--binary", str(fake))
        self.assertNotEqual(code, 0)                              # l4 missing -> no state change
        self.assertFalse((self.c3 / "_records" / "validate.json").exists())
        fake.write_text("#!/bin/sh\n" + "".join(f"echo '{fams[0]}-l{i}: OK (x)'\n" for i in (1, 2, 3))
                        + f"echo '{fams[0]}-l4: REJECT mutant m1 passes hidden'\n"
                        + "echo 'suite " + "a" * 64 + " \u2014 4 tasks, 3 accepted, 1 rejected'\nexit 1\n")
        code, lines = self.run_script("c3_validate.py", "--binary", str(fake))
        self.assertEqual(code, 0, lines)
        self.assertTrue((self.c3 / fams[0] / "FIX-VERDICTS.txt").exists())
        self.assertEqual([x["id"] for x in lines if x.get("stage") == "validate-reject"], [f"{fams[0]}-l4"])
        # a failed repair round restores the verified files; a clean one updates the hashes
        ref1 = (self.c3 / fams[0] / "out" / fams[0] / f"{fams[0]}-l1" / "reference.rs").read_bytes()
        code, lines = self.run_script("c3_fix.py", env=dict(self.env, C3_TEST_FIXFAIL="1"))
        ev = [x for x in lines if x.get("stage") == "fix" and x.get("id") == fams[0]][0]
        self.assertFalse(ev["ok"])
        self.assertEqual((self.c3 / fams[0] / "out" / fams[0] / f"{fams[0]}-l1" / "reference.rs").read_bytes(), ref1)
        code, lines = self.run_script("c3_assemble.py")
        self.assertEqual(lines[-1]["families"], 1, lines)            # restored files still verify

    def test_tool_events_counted(self):
        self.select()
        fid = (self.c3 / "FAMILIES.txt").read_text().split()[0]
        env = dict(self.env, C3_TEST_TOOL="1")
        code, lines = self.run_script("c3_author.py", "--only", fid, env=env)
        ev = [x for x in lines if x.get("stage") == "author" and x.get("id") == fid][0]
        self.assertEqual(ev["other_events"], 1)
        self.assertFalse(ev["ok"])                               # fail closed


if __name__ == "__main__":
    unittest.main()
