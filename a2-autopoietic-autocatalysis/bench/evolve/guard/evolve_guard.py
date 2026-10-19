"""Pytest guard loaded from outside the candidate tree.

Besides pinning pytest to the venv, it streams every test report over a pipe inherited from the
grader (EVOLVE_REPORT_FD) as each test reports, and writes an END marker at session finish. The grader
trusts a pass only if that stream agrees with junit: code under test shares this process and could
rewrite junit.xml (e.g. from an atexit handler), but records it sends after END are rejected, and a
genuine failing record cannot be retracted.
"""

from __future__ import annotations

import json
import os
from pathlib import Path

_stream = None


def pytest_configure(config) -> None:
    global _stream
    import pytest
    import _pytest

    venv = Path(os.environ["EVOLVE_VENV"]).resolve()
    for module in (pytest, _pytest):
        try:
            Path(module.__file__).resolve().relative_to(venv)
        except (ValueError, TypeError):
            raise SystemExit(97)
    fd = os.environ.pop("EVOLVE_REPORT_FD", None)
    if fd is not None:
        _stream = os.fdopen(int(fd), "w", buffering=1)


def pytest_runtest_logreport(report) -> None:
    if _stream is not None:
        _stream.write(json.dumps([report.nodeid, report.when, report.outcome]) + "\n")


def pytest_sessionfinish(session, exitstatus) -> None:
    global _stream
    if _stream is not None:
        _stream.write(json.dumps(["END"]) + "\n")
        _stream.close()
        _stream = None
