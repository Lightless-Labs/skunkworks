#!/usr/bin/env python3
"""Create and verify the frozen evolution evaluator digest."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import sys

REPO_ROOT = Path(__file__).resolve().parents[2]
A2CTL = REPO_ROOT / "target" / "debug" / "a2ctl"
LOCK_PATH = REPO_ROOT / "bench" / "evolve" / "evaluator.lock"


def _frozen_paths() -> list[str]:
    paths = set()
    for folder in (REPO_ROOT / "bench" / "e1", REPO_ROOT / "bench" / "evolve", REPO_ROOT / "bench" / "confirm"):
        for path in folder.rglob("*"):
            if not path.is_file() or path.name.startswith("test_") or "__pycache__" in path.parts:
                continue
            if path.name == "evaluator.lock":
                continue
            if path.suffix in (".py", ".toml"):
                paths.add(path.relative_to(REPO_ROOT).as_posix())
    paths.update(("bench/evolve/guard/evolve_guard.py", "bench/evolve/codex-home.config.toml",
                  "bench/e1/codex-home.config.toml"))
    program = REPO_ROOT / "bench" / "evolve" / "program.md"
    if program.is_file():
        paths.add("bench/evolve/program.md")
    return sorted(paths)


FROZEN = _frozen_paths()


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def compute() -> dict:
    files = {rel: _sha256(REPO_ROOT / rel) for rel in sorted(FROZEN)}
    a2ctl_sha256 = _sha256(A2CTL) if A2CTL.is_file() else None
    payload = {"files": files, "a2ctl_sha256": a2ctl_sha256}
    canonical = json.dumps(payload, sort_keys=True, separators=(",", ":")).encode()
    payload["evaluator_sha256"] = hashlib.sha256(canonical).hexdigest()
    return payload


def write() -> dict:
    result = compute()
    LOCK_PATH.write_text(json.dumps(result, indent=1, sort_keys=True) + "\n")
    return result


def verify() -> tuple[bool, list[str]]:
    if not LOCK_PATH.is_file():
        return False, ["bench/evolve/evaluator.lock (missing)"]
    try:
        expected = json.loads(LOCK_PATH.read_text())
    except (OSError, ValueError):
        return False, ["bench/evolve/evaluator.lock (invalid JSON)"]
    actual = compute()
    mismatches = []
    expected_files = expected.get("files", {})
    actual_files = actual["files"]
    for rel in sorted(set(expected_files) | set(actual_files)):
        if expected_files.get(rel) != actual_files.get(rel):
            mismatches.append(rel)
    if expected.get("a2ctl_sha256") != actual["a2ctl_sha256"]:
        mismatches.append("target/debug/a2ctl (a2ctl_sha256)")
    if expected.get("evaluator_sha256") != actual["evaluator_sha256"]:
        mismatches.append("evaluator_sha256")
    return not mismatches, mismatches


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("write", "verify"))
    args = parser.parse_args(argv)
    if args.action == "write":
        print(json.dumps(write(), indent=1, sort_keys=True))
        return 0
    ok, mismatches = verify()
    print("evaluator lock verified" if ok else "evaluator lock mismatch: " + ", ".join(mismatches))
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
