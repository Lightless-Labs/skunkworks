"""Pure helpers shared by the E1 tooling (see docs/plans/2026-10-14-value-test-first.md)."""

from __future__ import annotations

import datetime
import hashlib
from typing import Callable, Iterable

# Path components and basenames treated as test code, mirroring SWE-Bench Pro V2 tests/test.sh.
TEST_DIR_COMPONENTS = {"tests", "test", "__tests__", "spec", "fixtures", "mocks", "__snapshots__", "snapshots"}


def split_fail_to_pass(test_ids: Iterable[str]) -> tuple[list[str], list[str]]:
    """Deterministically split fail-to-pass ids into (visible, hidden) halves by SHA-256 order.

    Visible gets floor(n/2); hidden gets the rest, so both are non-empty for n >= 2.
    """
    ids = sorted(set(test_ids), key=lambda t: hashlib.sha256(t.encode("utf-8")).hexdigest())
    if len(ids) < 2:
        raise ValueError("visible/hidden split requires at least two fail-to-pass tests")
    cut = len(ids) // 2
    return ids[:cut], ids[cut:]


def is_test_path(path: str) -> bool:
    parts = path.strip("/").split("/")
    if any(part in TEST_DIR_COMPONENTS for part in parts[:-1]):
        return True
    name = parts[-1]
    stem = name.split(".", 1)[0]
    return (
        name == "conftest.py"
        or (name.startswith("test_") and name.endswith(".py"))
        or stem.endswith("_test")
        or ".test." in name
        or ".spec." in name
    )


def python_for_base_date(base_date: str) -> str:
    year = datetime.date.fromisoformat(base_date).year
    if year <= 2021:
        return "3.9"
    if year == 2022:
        return "3.10"
    return "3.11"


def exclude_newer_for_base_date(base_date: str) -> str:
    return (datetime.date.fromisoformat(base_date) + datetime.timedelta(days=1)).isoformat()


def opaque_task_id(instance_id: str) -> str:
    return "e1-" + hashlib.sha256(f"a2-e1:{instance_id}".encode("utf-8")).hexdigest()[:12]


def instruction_text(row: dict) -> str:
    """Agent-facing task text: the official instruction fields only (never patches or test lists)."""
    sections = [row["problem_statement"].strip()]
    if (row.get("requirements") or "").strip():
        sections.append("Requirements:\n" + row["requirements"].strip())
    if (row.get("interface") or "").strip():
        sections.append("Interface:\n" + row["interface"].strip())
    return "\n\n".join(sections)


def resolved(required: set[str], passed: set[str]) -> bool:
    return bool(required) and required <= passed


def junit_case_to_node_id(classname: str, name: str, file_exists: Callable[[str], bool]) -> str | None:
    """Map a pytest JUnit (classname, name) pair to a pytest node id, given which files exist."""
    parts = classname.split(".") if classname else []
    for i in range(len(parts), 0, -1):
        candidate = "/".join(parts[:i]) + ".py"
        if file_exists(candidate):
            return "::".join([candidate, *parts[i:], name])
    return None
