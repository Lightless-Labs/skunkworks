"""Tests for the confirmation-3 authoring pipeline, on fixtures with a fake codex.

Runs every script as a subprocess with A2D_HOLDOUT_DIR pointing at a fresh temp dir and
C3_CODEX pointing at tests/fake_codex.sh, under the real seatbelt profile. Asserts the
isolation checks, the sandbox (canary read denied, including from the job), the output
gateway (no fixture content, names or model text on stdout), selection, authoring,
assembly, similarity and the drain check.

usage: python3 -I -m unittest discover -s tools/holdout/tests -v
"""
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
SECRETS = ("SECRET", "secret-family-name", "Brief ")


class Pipeline(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="c3test-")).resolve()
        self.holdout = self.tmp / "holdout"
        self.holdout.mkdir()
        (self.tmp / "auth.json").write_text("{}")
        self.canary = self.tmp / "canary.txt"
        self.env = dict(os.environ, A2D_HOLDOUT_DIR=str(self.holdout), C3_CODEX=str(FAKE),
                        C3_AUTH=str(self.tmp / "auth.json"), C3_CANARY=str(self.canary))

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def run_script(self, name, *args, env=None):
        r = subprocess.run([sys.executable, "-I", str(TOOLS / name), *args], capture_output=True, text=True,
                           env=env or self.env, timeout=600)
        lines = [json.loads(x) for x in r.stdout.splitlines() if x.strip()]
        for x in r.stdout.splitlines():
            for s in SECRETS:
                self.assertNotIn(s, x, f"{name} leaked content on stdout")
        return r.returncode, lines

    def lib(self, code):
        """Run a snippet against c3lib in a subprocess with the test environment."""
        r = subprocess.run([sys.executable, "-I", "-c", f"import sys; sys.path.insert(0, {str(TOOLS)!r})\n{code}"],
                           capture_output=True, text=True, env=self.env, timeout=300)
        self.assertEqual(r.returncode, 0, r.stderr[-2000:])
        return r.stdout.strip()

    def test_emit_redacts(self):
        out = self.lib("from c3lib import emit\nemit(stage='x', id='c3-f01', count=3, error='Has Spaces!', bogus=1, "
                       "status='a' * 60)")
        d = json.loads(out)
        self.assertEqual(d["stage"], "x")
        self.assertEqual(d["id"], "c3-f01")
        self.assertEqual(d["count"], 3)
        self.assertEqual(d["error"], "redacted")
        self.assertEqual(d["status"], "redacted")
        self.assertNotIn("bogus", d)

    def test_isolation_checks(self):
        w = self.holdout / "authoring" / "confirmation-3" / "c3-f01"
        w.mkdir(parents=True)
        check = f"from c3lib import isolation_check\nprint(isolation_check({str(w)!r}))"
        self.assertEqual(self.lib(check), "True")
        (w / "AGENTS.md").write_text("x")
        self.assertEqual(self.lib(check), "False")
        (w / "AGENTS.md").unlink()
        ch = self.holdout / "authoring" / "confirmation-3" / "_codex_home"
        (ch / "config.toml").write_text("x")
        self.assertEqual(self.lib(check), "False")
        (ch / "config.toml").unlink()
        self.env["C3_TEST_VERSION"] = "codex-cli 9.9.9"
        self.assertEqual(self.lib(check), "False")
        del self.env["C3_TEST_VERSION"]
        outside = self.tmp / "outside"
        outside.mkdir()
        self.assertEqual(self.lib(f"from c3lib import isolation_check\nprint(isolation_check({str(outside)!r}))"),
                         "False")

    def test_drained(self):
        p = self.tmp / "pids.txt"
        dead = subprocess.Popen(["/usr/bin/true"])
        dead.wait()
        p.write_text(f"{dead.pid}\n")
        self.assertEqual(self.lib(f"from c3lib import drained\nprint(drained({str(p)!r}))"), "True")
        p.write_text(f"{os.getpid()}\n")
        self.assertEqual(self.lib(f"from c3lib import drained\nprint(drained({str(p)!r}))"), "False")

    def test_full_flow(self):
        code, lines = self.run_script("c3_select.py")
        self.assertEqual(code, 0, lines)
        nc = [x for x in lines if x.get("stage") == "namecheck" and "conflicts" in x][0]
        self.assertEqual(nc["conflicts"], 1)
        self.assertEqual(nc["families"], 36)
        fams = (self.holdout / "authoring" / "confirmation-3" / "FAMILIES.txt").read_text().split()
        self.assertEqual(len(fams), 36)
        self.assertNotIn("c3-f05", fams)          # the conflict is skipped
        self.assertEqual(fams[-1], "c3-f37")      # the first spare fills in
        inv = (self.holdout / "authoring" / "confirmation-3" / "_select" / "INVENTORY.txt").read_text().split()
        self.assertIn("bank-queue-sim", inv)       # repo families are in the inventory

        # The job itself must not be able to read the canary outside its workdir.
        self.canary.write_text("CANARY-CONTENT\n")
        env = dict(self.env, C3_TEST_PROBE=str(self.canary))
        code, lines = self.run_script("c3_author.py", "--only", fams[0], env=env)
        self.assertEqual(code, 0, lines)
        probe = (self.holdout / "authoring" / "confirmation-3" / fams[0] / "probe-result.txt").read_text()
        self.assertNotIn("CANARY", probe.replace(str(self.canary), ""))
        self.assertIn("not permitted", probe)

        code, lines = self.run_script("c3_author.py", "--parallel", "3")
        self.assertEqual(code, 0, lines)
        final = lines[-1]
        self.assertEqual(final["done"], 36)
        self.assertTrue(all(x.get("other_events", 0) == 0 for x in lines if x.get("stage") == "author"))

        code, lines = self.run_script("c3_assemble.py")
        self.assertEqual(code, 0, lines)
        self.assertEqual(lines[-1]["tasks"], 144)
        self.assertEqual(lines[-1]["missing"], 0)
        task = self.holdout / "confirmation-3" / f"{fams[0]}-l1"
        self.assertTrue((task / "hidden_tests.rs").is_file())
        self.assertFalse((task / "REPORT.md").exists())

        code, lines = self.run_script("c3_similarity.py")
        self.assertEqual(code, 0, lines)
        self.assertEqual(lines[0]["tasks"], 144)
        self.assertEqual(lines[0]["conflicts"], 0)

        code, lines = self.run_script("c3_author.py", "--drained")
        self.assertTrue(lines[-1]["ok"])

    def test_tool_events_counted(self):
        self.run_script("c3_select.py")
        env = dict(self.env, C3_TEST_TOOL="1")
        fid = (self.holdout / "authoring" / "confirmation-3" / "FAMILIES.txt").read_text().split()[0]
        code, lines = self.run_script("c3_author.py", "--only", fid, env=env)
        ev = [x for x in lines if x.get("stage") == "author" and x.get("id") == fid][0]
        self.assertEqual(ev["other_events"], 1)


if __name__ == "__main__":
    unittest.main()
