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

    def lib(self, code):
        r = subprocess.run([sys.executable, "-I", "-c", f"import sys; sys.path.insert(0, {str(TOOLS)!r})\n{code}"],
                           capture_output=True, text=True, env=self.env, timeout=300)
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
        (self.c3 / "_records" / "pids.txt").write_text(f"{os.getpgid(0)}\n")
        self.assertEqual(self.lib(drained), "False")            # a live group
        dead = subprocess.Popen(["/usr/bin/true"], start_new_session=True)
        dead.wait()
        (self.c3 / "_records" / "pids.txt").write_text(f"{dead.pid}\n")
        self.assertEqual(self.lib(drained), "True")

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
            f"print(r.returncode, a.returncode, 'API_OK' in a.stdout, 'test result: ok' in r.stdout)")
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
        for f in fams[:2]:
            code, lines = self.run_script("c3_author.py", "--reauthor", f, env=env)
            self.assertEqual(code, 0, lines)
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
                        + f"echo '{fams[0]}-l4: REJECT mutant m1 passes hidden'\n")
        code, lines = self.run_script("c3_validate.py", "--binary", str(fake))
        self.assertEqual(code, 0, lines)
        self.assertTrue((self.c3 / fams[0] / "FIX-VERDICTS.txt").exists())
        self.assertEqual([x["id"] for x in lines if x.get("stage") == "validate-reject"], [f"{fams[0]}-l4"])

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
