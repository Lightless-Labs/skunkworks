#!/usr/bin/env python3
"""Run A²'s first loop-shaped self-correction benchmark.

The harness creates an isolated git worktree, injects a deterministic bug, commits
that bug only in the worktree branch, and then runs repeated `a2ctl run --apply`
attempts with the same JSONL `task_id`. Each attempt is evaluated immediately and
emitted as one JSON object.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import sqlite3
import subprocess
import sys
import tempfile
import time
import unittest
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any
from uuid import UUID

TASK_ID = "self-correction-fibonacci-regression"
CATEGORY = "self_correction"
BUG_DESCRIPTION = """\
The workspace contains a regression. `cargo test -p a2_core test_fibonacci` fails.
Diagnose the root cause and fix the implementation. Do not assume the failing
function name is the location of the bug; inspect the code and make the minimal
change that restores the test. Run `cargo test -p a2_core test_fibonacci` before
finishing.
"""
VERIFY_COMMAND = "cargo test -p a2_core test_fibonacci"
BUG_OLD = "if n == 0 {\n        return 0;\n    }"
BUG_NEW = "if n == 0 {\n        return 1;\n    }"
FNV_OFFSET_128 = 0x6C62_272E_07BB_0142_62B8_2175_6295_C58D
FNV_PRIME_128 = 0x0000_0000_0100_0000_0000_0000_0000_013B


@dataclass
class CommandResult:
    command: list[str] | str
    returncode: int
    stdout: str
    stderr: str
    duration_secs: float


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", default=".", help="Source A² project path containing Cargo.toml.")
    parser.add_argument("--provider", default="gemini", help="Provider/model for a2ctl run.")
    parser.add_argument("--attempts", type=int, default=2, help="Number of repeated A² attempts.")
    parser.add_argument("--max-tokens", type=int, default=100_000, help="Per-attempt token budget.")
    parser.add_argument("--timeout", type=int, default=1800, help="Per-attempt timeout in seconds.")
    parser.add_argument("--run-id", default=None, help="Stable run ID for result records.")
    parser.add_argument(
        "--results",
        default="bench/self-correction-results.jsonl",
        help="Path for JSONL attempt results.",
    )
    parser.add_argument("--workdir", default=None, help="Use this isolated git worktree root path.")
    parser.add_argument("--keep-workspace", action="store_true", help="Do not remove the worktree.")
    parser.add_argument(
        "--smoke-only",
        action="store_true",
        help="Create/inject/evaluate the bugged workspace without calling a model.",
    )
    return parser.parse_args(argv)


def run_command(
    command: list[str] | str,
    cwd: Path,
    *,
    stdin: str | None = None,
    timeout: int | None = None,
    shell: bool = False,
) -> CommandResult:
    start = time.monotonic()
    process = subprocess.run(
        command,
        cwd=str(cwd),
        input=stdin,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=timeout,
        shell=shell,
        executable="/bin/bash" if shell else None,
        env={**os.environ, "PYTHONUNBUFFERED": "1"},
    )
    return CommandResult(
        command=command,
        returncode=process.returncode,
        stdout=process.stdout,
        stderr=process.stderr,
        duration_secs=time.monotonic() - start,
    )


def git(args: list[str], cwd: Path) -> CommandResult:
    result = run_command(["git", *args], cwd)
    if result.returncode != 0:
        raise RuntimeError(f"git {' '.join(args)} failed: {result.stderr.strip()}")
    return result


def repo_root(path: Path) -> Path:
    result = git(["rev-parse", "--show-toplevel"], path)
    return Path(result.stdout.strip()).resolve()


def deterministic_task_uuid(key: str, prefix: str = "task") -> str:
    hash_value = FNV_OFFSET_128
    for byte in prefix.encode("utf-8") + b"\0" + key.encode("utf-8"):
        hash_value ^= byte
        hash_value = (hash_value * FNV_PRIME_128) % (1 << 128)

    raw = bytearray(hash_value.to_bytes(16, "big"))
    raw[6] = (raw[6] & 0x0F) | 0x80
    raw[8] = (raw[8] & 0x3F) | 0x80
    return str(UUID(bytes=bytes(raw)))


def serialized_task_id(task_id: str) -> str:
    return json.dumps(deterministic_task_uuid(task_id))


def lineage_count(workspace: Path, task_id: str) -> int:
    db = workspace / "lineage.sqlite"
    if not db.exists():
        return 0

    with sqlite3.connect(db) as connection:
        try:
            row = connection.execute(
                "SELECT COUNT(*) FROM lineage_records WHERE task_id = ?",
                (serialized_task_id(task_id),),
            ).fetchone()
        except sqlite3.Error:
            return 0
    return int(row[0]) if row else 0


def reconcile_latest_lineage_with_verify(
    workspace: Path,
    task_id: str,
    verify_result: CommandResult,
) -> bool:
    """Patch the newest lineage row with post-apply verification truth.

    `a2ctl run` persists lineage before its outer `git apply` + rebuild gate. For
    a self-correction benchmark, the next attempt needs to see the *actual*
    verification failure, not just the pre-apply SeedEvaluator result. This keeps
    the benchmark's prior-attempt motif honest without mutating the germline.
    """
    db = workspace / "lineage.sqlite"
    if not db.exists():
        return False

    passed = verify_result.returncode == 0
    with sqlite3.connect(db) as connection:
        row = connection.execute(
            """
            SELECT id, fitness_json, patch_rationale
            FROM lineage_records
            WHERE task_id = ?
            ORDER BY created_at DESC
            LIMIT 1
            """,
            (serialized_task_id(task_id),),
        ).fetchone()
        if row is None:
            return False

        record_id, fitness_json, patch_rationale = row
        fitness = json.loads(fitness_json)
        somatic = fitness.setdefault("somatic", {})
        somatic["tests_pass"] = passed
        somatic["task_completed"] = passed
        somatic["acceptance_met"] = [passed for _ in somatic.get("acceptance_met", [])]

        verify_note = (
            f"\n\n[external verify: {'PASS' if passed else 'FAIL'}] "
            f"{VERIFY_COMMAND} exited {verify_result.returncode}."
        )
        if not passed:
            detail = (verify_result.stderr or verify_result.stdout).strip()
            if detail:
                verify_note += " " + " ".join(detail.split())[:1000]

        connection.execute(
            """
            UPDATE lineage_records
            SET fitness_json = ?, patch_rationale = ?
            WHERE id = ?
            """,
            (json.dumps(fitness, separators=(",", ":")), (patch_rationale or "") + verify_note, record_id),
        )
    return True


def create_worktree(source_repo: Path, destination: Path, branch: str) -> Path:
    git(["worktree", "add", "-b", branch, str(destination), "HEAD"], source_repo)
    return destination


def cleanup_worktree(source_repo: Path, workspace: Path, branch: str) -> None:
    subprocess.run(
        ["git", "worktree", "remove", "--force", str(workspace)],
        cwd=str(source_repo),
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        check=False,
    )
    subprocess.run(
        ["git", "branch", "-D", branch],
        cwd=str(source_repo),
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        check=False,
    )


def inject_fibonacci_bug(workspace: Path) -> None:
    path = workspace / "crates/a2_core/src/lib.rs"
    content = path.read_text(encoding="utf-8")
    if BUG_NEW in content:
        return
    if BUG_OLD not in content:
        raise RuntimeError(f"bug fixture target not found in {path}")
    path.write_text(content.replace(BUG_OLD, BUG_NEW, 1), encoding="utf-8")


def commit_bug(workspace: Path) -> None:
    git(["add", "crates/a2_core/src/lib.rs"], workspace)
    git(
        [
            "-c",
            "user.name=A2 Self-Correction Benchmark",
            "-c",
            "user.email=a2-self-correction@example.invalid",
            "commit",
            "-m",
            "bench: inject fibonacci regression",
        ],
        workspace,
    )


def task_payload(task_id: str, run_id: str, attempt: int) -> dict[str, Any]:
    return {
        "task_id": task_id,
        "problem_statement": BUG_DESCRIPTION,
        "category": CATEGORY,
        "run_id": run_id,
        "attempt": attempt,
    }


def run_a2_attempt(
    workspace: Path,
    provider: str,
    max_tokens: int,
    timeout: int,
    payload: dict[str, Any],
) -> CommandResult:
    command = [
        "cargo",
        "run",
        "-p",
        "a2ctl",
        "--",
        "run",
        "--provider",
        provider,
        "--max-tokens",
        str(max_tokens),
        "--timeout",
        str(timeout),
        "--apply",
    ]
    return run_command(command, workspace, stdin=json.dumps(payload) + "\n", timeout=timeout + 900)


def verify(workspace: Path) -> CommandResult:
    return run_command(VERIFY_COMMAND, workspace, shell=True, timeout=300)


def result_record(
    *,
    payload: dict[str, Any],
    provider: str,
    workspace: Path,
    a2_result: CommandResult | None,
    verify_result: CommandResult,
    lineage_before: int,
    lineage_after: int,
    lineage_reconciled: bool,
) -> dict[str, Any]:
    return {
        "task_id": payload["task_id"],
        "category": payload["category"],
        "run_id": payload["run_id"],
        "attempt": payload["attempt"],
        "provider": provider,
        "model": provider,
        "resolved": verify_result.returncode == 0,
        "prior_lineage_present": lineage_before > 0,
        "lineage_records_before": lineage_before,
        "lineage_records_after": lineage_after,
        "lineage_reconciled_with_verify": lineage_reconciled,
        "workspace": str(workspace),
        "a2_returncode": a2_result.returncode if a2_result else None,
        "a2_duration_secs": round(a2_result.duration_secs, 3) if a2_result else 0.0,
        "verify_command": VERIFY_COMMAND,
        "verify_returncode": verify_result.returncode,
        "verify_duration_secs": round(verify_result.duration_secs, 3),
        "stdout": "\n\n".join(
            part
            for part in (
                a2_result.stdout if a2_result else "",
                verify_result.stdout,
            )
            if part
        ),
        "stderr": "\n\n".join(
            part
            for part in (
                a2_result.stderr if a2_result else "",
                verify_result.stderr,
            )
            if part
        ),
        "evaluated_at": datetime.now(timezone.utc).isoformat(),
    }


def append_jsonl(path: Path, record: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("a", encoding="utf-8") as handle:
        handle.write(json.dumps(record, sort_keys=True) + "\n")


def run_benchmark(args: argparse.Namespace) -> int:
    source_project = Path(args.repo).resolve()
    if not (source_project / "Cargo.toml").exists():
        raise RuntimeError(f"--repo must point at the A² project root: {source_project}")

    source_git_root = repo_root(source_project)
    project_relative = source_project.relative_to(source_git_root)
    run_id = args.run_id or datetime.now(timezone.utc).strftime("self-correction-%Y%m%dT%H%M%SZ")
    branch = f"a2-self-correction-{run_id}"
    worktree_root = Path(args.workdir).resolve() if args.workdir else Path(tempfile.mkdtemp(prefix="a2-self-correction-"))
    workspace = worktree_root / project_relative
    results = Path(args.results)
    if not results.is_absolute():
        results = source_project / results

    created = False
    try:
        if worktree_root.exists() and any(worktree_root.iterdir()):
            raise RuntimeError(f"workspace path is not empty: {worktree_root}")
        if worktree_root.exists():
            worktree_root.rmdir()
        create_worktree(source_git_root, worktree_root, branch)
        created = True
        inject_fibonacci_bug(workspace)
        commit_bug(workspace)

        initial = verify(workspace)
        if initial.returncode == 0:
            raise RuntimeError("bug fixture did not fail before A² attempts")

        attempts = 1 if args.smoke_only else max(args.attempts, 1)
        for attempt in range(1, attempts + 1):
            payload = task_payload(TASK_ID, run_id, attempt)
            lineage_before = lineage_count(workspace, TASK_ID)
            a2_result = (
                None
                if args.smoke_only
                else run_a2_attempt(
                    workspace,
                    args.provider,
                    args.max_tokens,
                    args.timeout,
                    payload,
                )
            )
            verified = verify(workspace)
            lineage_reconciled = reconcile_latest_lineage_with_verify(workspace, TASK_ID, verified)
            lineage_after = lineage_count(workspace, TASK_ID)
            record = result_record(
                payload=payload,
                provider=args.provider,
                workspace=workspace,
                a2_result=a2_result,
                verify_result=verified,
                lineage_before=lineage_before,
                lineage_after=lineage_after,
                lineage_reconciled=lineage_reconciled,
            )
            append_jsonl(results, record)
            print(json.dumps(record, sort_keys=True))
            if verified.returncode == 0:
                break

        return 0
    finally:
        if created and not args.keep_workspace:
            cleanup_worktree(source_git_root, worktree_root, branch)
        elif not created and worktree_root.exists() and not args.keep_workspace and args.workdir is None:
            shutil.rmtree(worktree_root, ignore_errors=True)


class SelfCorrectionTests(unittest.TestCase):
    def test_deterministic_task_uuid_is_stable(self) -> None:
        self.assertEqual(deterministic_task_uuid("same"), deterministic_task_uuid("same"))
        self.assertNotEqual(deterministic_task_uuid("same"), deterministic_task_uuid("other"))

    def test_task_payload_reuses_id_across_attempts(self) -> None:
        first = task_payload(TASK_ID, "run", 1)
        second = task_payload(TASK_ID, "run", 2)
        self.assertEqual(first["task_id"], second["task_id"])
        self.assertEqual(first["run_id"], second["run_id"])
        self.assertEqual(second["attempt"], 2)

    def test_result_record_reports_prior_lineage(self) -> None:
        payload = task_payload(TASK_ID, "run", 2)
        verify_result = CommandResult(VERIFY_COMMAND, 0, "ok", "", 1.25)
        record = result_record(
            payload=payload,
            provider="gemini",
            workspace=Path("/tmp/workspace"),
            a2_result=None,
            verify_result=verify_result,
            lineage_before=1,
            lineage_after=2,
            lineage_reconciled=True,
        )
        self.assertTrue(record["resolved"])
        self.assertTrue(record["prior_lineage_present"])
        self.assertEqual(record["lineage_records_after"], 2)
        self.assertTrue(record["lineage_reconciled_with_verify"])

    def test_reconcile_latest_lineage_with_verify_updates_fitness_and_rationale(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            workspace = Path(directory)
            db = workspace / "lineage.sqlite"
            task_id = TASK_ID
            with sqlite3.connect(db) as connection:
                connection.execute(
                    """
                    CREATE TABLE lineage_records (
                        id TEXT PRIMARY KEY,
                        task_id TEXT NOT NULL,
                        patch_id TEXT NOT NULL,
                        patch_diff TEXT,
                        patch_rationale TEXT,
                        parent_germline TEXT NOT NULL,
                        model_attributions_json TEXT NOT NULL,
                        fitness_json TEXT NOT NULL,
                        created_at TEXT NOT NULL
                    )
                    """
                )
                connection.execute(
                    """
                    INSERT INTO lineage_records (
                        id, task_id, patch_id, patch_diff, patch_rationale,
                        parent_germline, model_attributions_json, fitness_json, created_at
                    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                    """,
                    (
                        '"lineage"',
                        serialized_task_id(task_id),
                        '"patch"',
                        "+bad",
                        "original rationale",
                        '"germline"',
                        "[]",
                        json.dumps(
                            {
                                "somatic": {
                                    "task_completed": True,
                                    "tests_pass": True,
                                    "acceptance_met": [True],
                                }
                            }
                        ),
                        "2026-04-28T00:00:00Z",
                    ),
                )

            verify_result = CommandResult(VERIFY_COMMAND, 101, "failed", "boom", 0.1)
            self.assertTrue(reconcile_latest_lineage_with_verify(workspace, task_id, verify_result))

            with sqlite3.connect(db) as connection:
                fitness_json, rationale = connection.execute(
                    "SELECT fitness_json, patch_rationale FROM lineage_records"
                ).fetchone()
            fitness = json.loads(fitness_json)
            self.assertFalse(fitness["somatic"]["task_completed"])
            self.assertFalse(fitness["somatic"]["tests_pass"])
            self.assertEqual(fitness["somatic"]["acceptance_met"], [False])
            self.assertIn("external verify: FAIL", rationale)


if __name__ == "__main__":
    if sys.argv[1:2] == ["--self-test"]:
        sys.argv = [sys.argv[0]]
        raise SystemExit(unittest.main())
    raise SystemExit(run_benchmark(parse_args(sys.argv[1:])))
