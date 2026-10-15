from __future__ import annotations

import tempfile
import unittest
import os
from pathlib import Path

from bench.evolve import seatbelt


def skip_without_sandbox_exec(case: unittest.TestCase) -> None:
    """Skip only when Seatbelt itself is unusable (e.g. nested inside another sandbox)."""
    try:
        probe = seatbelt.run_sandboxed(["/usr/bin/true"], "(version 1)(allow default)", capture_output=True)
    except OSError as exc:
        case.skipTest(f"sandbox-exec unavailable: {exc}")
    if probe.returncode != 0:
        case.skipTest("sandbox-exec cannot apply even a permissive profile here (nested sandbox)")


class SeatbeltProfileTests(unittest.TestCase):
    def test_profile_is_deny_by_default_after_allow_default(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            profile = seatbelt.pytest_profile([root], [root / "write"], root / "home")
        self.assertIn("(allow default)", profile)
        self.assertIn("(deny network*)", profile)
        self.assertLess(profile.index("(deny file-write*"), profile.index("(allow file-write*"))
        self.assertIn('(literal "/dev/null")', profile)
        self.assertIn(f'(subpath "{os.path.realpath(root)}")', profile)

    def test_profile_canonicalizes_paths(self) -> None:
        profile = seatbelt.pytest_profile([Path("/tmp")], [], Path("/tmp/private-home"))
        self.assertIn('(subpath "/private/tmp")', profile)
        self.assertIn('(subpath "/private/tmp/private-home")', profile)

    def test_run_sandboxed_enforces_profile(self) -> None:
        skip_without_sandbox_exec(self)
        allowed = seatbelt.run_sandboxed(["/usr/bin/true"], "(version 1)(allow default)", capture_output=True)
        self.assertEqual(allowed.returncode, 0)
        denied = seatbelt.run_sandboxed(["/bin/cat", "/etc/hosts"],
                                        '(version 1)(allow default)(deny file-read* (literal "/private/etc/hosts"))',
                                        capture_output=True, text=True)
        self.assertNotEqual(denied.returncode, 0)

if __name__ == "__main__":
    unittest.main()
