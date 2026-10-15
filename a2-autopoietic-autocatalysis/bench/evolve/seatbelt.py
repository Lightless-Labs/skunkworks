"""Small SBPL profile builder and sandbox-exec wrapper."""

from __future__ import annotations

import os
import subprocess
from pathlib import Path

DEV_LITERALS = ("/dev/null", "/dev/tty", "/dev/urandom", "/dev/random", "/dev/dtracehelper")


def _canonical(path: str | Path) -> str:
    return os.path.realpath(os.fspath(path))


def _quoted(path: str | Path) -> str:
    value = _canonical(path)
    if '"' in value or "\\" in value:
        raise ValueError("Seatbelt paths may not contain quotes or backslashes")
    return f'"{value}"'


def _spellings(path: str | Path) -> list[str]:
    """The canonical path, plus the unresolved absolute path when a symlink is on the way.

    exec and open traverse the path as written (e.g. uv's `cpython-3.11-...` alias), so an allow
    rule for the resolved target alone is denied at the symlink.
    """
    raw = os.path.abspath(os.fspath(path))
    canonical = _quoted(path)
    if raw == _canonical(path) or '"' in raw or "\\" in raw:
        return [canonical]
    return [canonical, f'"{raw}"']


def pytest_profile(read_allow: list[str | Path], write_allow: list[str | Path], home: str | Path) -> str:
    """Build a deny-first profile; explicit allow rules come after denies (last match wins)."""
    lines = ["(version 1)", "(allow default)", "(deny network*)",
             f"(deny file-read* file-write* (subpath {_quoted(home)}))",
             "(deny file-write*)",
             # stat/existence checks only (pytest walks rootdir ancestors); contents and listings stay denied
             "(allow file-read-metadata)"]
    lines.extend(f"(allow file-read* (subpath {quoted}))" for path in read_allow for quoted in _spellings(path))
    lines.extend(f"(allow file-write* (subpath {quoted}))" for path in write_allow for quoted in _spellings(path))
    lines.extend(f"(allow file-read* file-write* (literal \"{path}\"))" for path in DEV_LITERALS)
    return "\n".join(lines) + "\n"


def run_sandboxed(argv: list[str], profile: str, **subprocess_kwargs) -> subprocess.CompletedProcess:
    """Run argv under macOS Seatbelt with the supplied SBPL profile."""
    return subprocess.run(["/usr/bin/sandbox-exec", "-p", profile, *map(str, argv)], **subprocess_kwargs)
