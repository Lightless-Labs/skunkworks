import hashlib
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from bench.evolve import lock


class LockTests(unittest.TestCase):
    def test_compute_is_stable_and_verify_detects_change(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "bench/e1").mkdir(parents=True)
            (root / "bench/evolve").mkdir(parents=True)
            frozen = root / "bench/e1/frozen.py"
            frozen.write_text("value = 1\n")
            with patch.object(lock, "REPO_ROOT", root), patch.object(lock, "A2CTL", root / "missing"), \
                    patch.object(lock, "LOCK_PATH", root / "bench/evolve/evaluator.lock"), \
                    patch.object(lock, "FROZEN", ["bench/e1/frozen.py"]):
                first = lock.compute()
                self.assertEqual(first, lock.compute())
                lock.write()
                self.assertEqual(lock.verify(), (True, []))
                frozen.write_text("value = 2\n")
                ok, mismatches = lock.verify()
                self.assertFalse(ok)
                self.assertIn("bench/e1/frozen.py", mismatches)
                self.assertEqual(first["files"]["bench/e1/frozen.py"], hashlib.sha256(b"value = 1\n").hexdigest())


if __name__ == "__main__":
    unittest.main()
