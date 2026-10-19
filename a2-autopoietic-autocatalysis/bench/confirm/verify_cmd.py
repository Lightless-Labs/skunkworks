#!/usr/bin/env python3
"""Visible hardened verifier used by confirmation's a2-full arm."""

from __future__ import annotations

import argparse
import os
import subprocess
import sys
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
if str(REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(REPO_ROOT))

from bench.evolve import hardened  # noqa: E402


def candidate_diff(worktree: str | Path) -> str:
    """Diff tracked and untracked candidate files against the history-free root commit."""
    root = Path(worktree).resolve()
    root_commit = subprocess.run(
        ["git", "rev-list", "--max-parents=0", "HEAD"], cwd=root, check=True,
        capture_output=True, text=True,
    ).stdout.strip()
    fd, index_name = tempfile.mkstemp(prefix="a2-confirm-index-")
    os.close(fd)
    Path(index_name).unlink()
    env = {**os.environ, "GIT_INDEX_FILE": index_name}
    try:
        subprocess.run(["git", "read-tree", "HEAD"], cwd=root, env=env, check=True,
                       capture_output=True, text=True)
        subprocess.run(["git", "add", "-A"], cwd=root, env=env, check=True,
                       capture_output=True, text=True)
        return subprocess.run(
            ["git", "diff", "--cached", "--binary", "--no-renames", root_commit], cwd=root, env=env,
            check=True, capture_output=True, text=True,
        ).stdout
    finally:
        Path(index_name).unlink(missing_ok=True)


def render(result: dict) -> str:
    tests = result.get("tests") or []
    passed = sum(test.get("outcome") == "pass" for test in tests)
    lines = []
    for test in tests:
        outcome = test.get("outcome", "error")
        if outcome == "pass":
            continue
        lines.append(f"[{outcome}] {test.get('name', test.get('index', 'unknown'))}")
        message = test.get("message") or ""
        if message:
            lines.append(message)
    for reason in result.get("rejected") or []:
        lines.append(f"rejected: {reason}")
    if result.get("error"):
        lines.append(f"error: {result['error']}")
    lines.append(f"visible verification: {passed}/{len(tests)} passed")
    return "\n".join(lines)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--task", required=True, help="opaque task identifier")
    args = parser.parse_args(argv)
    result = hardened.verify_structured(args.task, candidate_diff(Path.cwd()))
    print(render(result))
    return 0 if result.get("passed") else 1


if __name__ == "__main__":
    raise SystemExit(main())
