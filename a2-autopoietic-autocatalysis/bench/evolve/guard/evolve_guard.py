"""Pytest guard loaded from outside the candidate tree."""

from __future__ import annotations

import os
from pathlib import Path


def pytest_configure(config) -> None:
    import pytest
    import _pytest

    venv = Path(os.environ["EVOLVE_VENV"]).resolve()
    for module in (pytest, _pytest):
        try:
            Path(module.__file__).resolve().relative_to(venv)
        except (ValueError, TypeError):
            raise SystemExit(97)
