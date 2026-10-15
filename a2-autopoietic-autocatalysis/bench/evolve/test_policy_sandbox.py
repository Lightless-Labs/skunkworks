from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from bench.evolve import episode, seatbelt
from bench.evolve.test_seatbelt import skip_without_sandbox_exec


class PolicySandboxTests(unittest.TestCase):
    def setUp(self) -> None:
        # An existing file under the real home: denied by the profile, not merely missing.
        root = Path.home() / ".a2-e1" / "test-tmp"
        root.mkdir(parents=True, exist_ok=True)
        self.home_secret = root / "policy-probe-secret.txt"
        self.home_secret.write_text("secret")
        self.addCleanup(self.home_secret.unlink)

    def _run(self, body: str):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        root = Path(tmp.name)
        genome = root / "genome"
        base = root / "base"
        genome.mkdir()
        base.mkdir()
        (genome / "policy.py").write_text(body)
        state = {"nonce": "n", "problem_statement": "x", "base_snapshot": str(base),
                 "budget": {"agent_calls_left": 0, "steps_left": 1}, "candidates": []}
        return episode.run_policy(genome, state, base)

    def test_policy_cannot_read_home_write_file_or_open_socket(self):
        skip_without_sandbox_exec(self)
        body = (
            "import json, pathlib, socket, sys\n"
            "state=json.load(sys.stdin)\n"
            "checks=[]\n"
            "for fn in (lambda: pathlib.Path(%r).read_text(), "
            "lambda: pathlib.Path('/tmp/evolve-policy-write-probe').write_text('x'), "
            "lambda: socket.socket().connect(('127.0.0.1', 9))):\n"
            "    try: fn(); checks.append('allowed')\n"
            "    except OSError: checks.append('denied')\n"
            "print(json.dumps(checks))\n"
        ) % str(self.home_secret)
        stdout, _stderr, code = self._run(body)
        self.assertEqual(code, 0)
        self.assertEqual(json.loads(stdout), ["denied", "denied", "denied"])

    def test_policy_loop_is_killed_by_wall_limit(self):
        skip_without_sandbox_exec(self)
        with mock.patch.object(episode, "POLICY_WALL_TIMEOUT", 1):
            _stdout, stderr, code = self._run("while True: pass\n")
        self.assertEqual(code, -9)
        self.assertIn("timed out", stderr)

    def test_garbage_policy_output_is_policy_error_and_submits_last(self):
        self.assertIn('(deny file-read* file-write* (subpath "', episode._policy_profile(Path("/tmp/g"), Path("/tmp/b")))
        self.assertIn("(deny network*)", episode._policy_profile(Path("/tmp/g"), Path("/tmp/b")))
        self.assertIn("(deny file-write*)", episode._policy_profile(Path("/tmp/g"), Path("/tmp/b")))


if __name__ == "__main__":
    unittest.main()
