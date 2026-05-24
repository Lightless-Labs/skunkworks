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

CATEGORY = "self_correction"
FIBONACCI_TASK_ID = "self-correction-fibonacci-regression"
FIBONACCI_DESCRIPTION = """\
The workspace contains a regression. `cargo test -p a2_core test_fibonacci` fails.
Diagnose the root cause and fix the implementation. Do not assume the failing
function name is the location of the bug; inspect the code and make the minimal
change that restores the test. Run `cargo test -p a2_core test_fibonacci` before
finishing.
"""
FIBONACCI_VERIFY_COMMAND = "cargo test -p a2_core test_fibonacci"
FIBONACCI_BUG_OLD = "if n == 0 {\n        return 0;\n    }"
FIBONACCI_BUG_NEW = "if n == 0 {\n        return 1;\n    }"
SCAN_BUG_OLD = "if byte == b'\"' {\n            in_double = true;\n            index += 1;\n            continue;\n        }"
SCAN_BUG_NEW = "if byte == b'\"' {\n            index += 1;\n            continue;\n        }"
MEMBRANE_BUG_OLD = 'if cap.denied_tools.iter().any(|d| d == tool_name || d == "*") {\n            return false;\n        }'
MEMBRANE_BUG_NEW = 'if cap.denied_tools.iter().any(|d| d == tool_name || d == "*") {\n            return true;\n        }'
ARCHIVE_BUG_OLD = "FROM lineage_records\n                WHERE task_id = ?1\n                ORDER BY created_at ASC"
ARCHIVE_BUG_NEW = "FROM lineage_records\n                WHERE task_id = ?1\n                ORDER BY created_at DESC"
SENSORIUM_TASK_ID = "self-correction-compound-sensorium-same-crate-hidden-regressions"
SENSORIUM_DESCRIPTION = """\
The workspace contains a regression. `cargo test -p a2_sensorium high_risk_gets_low_priority` fails.
Diagnose the root cause and fix the implementation. Do not assume the failing
function name is the location of the bug; inspect the code and make the minimal
change that restores the test. Run `cargo test -p a2_sensorium high_risk_gets_low_priority` before
finishing.
"""
SENSORIUM_PRIORITY_BUG_OLD = "RiskTier::High => Priority::Low, // Untrusted signals get lower priority."
SENSORIUM_PRIORITY_BUG_NEW = "RiskTier::High => Priority::Normal, // Untrusted signals get lower priority."
SENSORIUM_TRUNCATE_BUG_OLD = "let mut t = s[..max - 3].to_string();"
SENSORIUM_TRUNCATE_BUG_NEW = "let mut t = s[..max].to_string();"
FNV_OFFSET_128 = 0x6C62_272E_07BB_0142_62B8_2175_6295_C58D
FNV_PRIME_128 = 0x0000_0000_0100_0000_0000_0000_0000_013B


@dataclass(frozen=True)
class Replacement:
    path: str
    old: str
    new: str


@dataclass(frozen=True)
class Fixture:
    name: str
    task_id: str
    description: str
    verify_command: str
    replacements: tuple[Replacement, ...]


FIXTURES: dict[str, Fixture] = {
    "fibonacci": Fixture(
        name="fibonacci",
        task_id=FIBONACCI_TASK_ID,
        description=FIBONACCI_DESCRIPTION,
        verify_command=FIBONACCI_VERIFY_COMMAND,
        replacements=(
            Replacement(
                "crates/a2_core/src/lib.rs",
                FIBONACCI_BUG_OLD,
                FIBONACCI_BUG_NEW,
            ),
        ),
    ),
    "compound-hidden": Fixture(
        name="compound-hidden",
        task_id="self-correction-compound-hidden-regressions",
        description=FIBONACCI_DESCRIPTION,
        verify_command=(
            "cargo test -p a2_core test_fibonacci; core=$?; "
            "cargo test -p a2ctl ignores_non_task_mentions_inside_comments_and_strings; ctl=$?; "
            "test $core -eq 0 -a $ctl -eq 0"
        ),
        replacements=(
            Replacement(
                "crates/a2_core/src/lib.rs",
                FIBONACCI_BUG_OLD,
                FIBONACCI_BUG_NEW,
            ),
            Replacement(
                "crates/a2ctl/src/main.rs",
                SCAN_BUG_OLD,
                SCAN_BUG_NEW,
            ),
        ),
    ),
    "compound-membrane-hidden": Fixture(
        name="compound-membrane-hidden",
        task_id="self-correction-compound-membrane-hidden-regressions",
        description=FIBONACCI_DESCRIPTION,
        verify_command=(
            "cargo test -p a2_core test_fibonacci; core=$?; "
            "cargo test -p a2_membrane deny_overrides_allow; membrane=$?; "
            "test $core -eq 0 -a $membrane -eq 0"
        ),
        replacements=(
            Replacement(
                "crates/a2_core/src/lib.rs",
                FIBONACCI_BUG_OLD,
                FIBONACCI_BUG_NEW,
            ),
            Replacement(
                "crates/a2_membrane/src/policy.rs",
                MEMBRANE_BUG_OLD,
                MEMBRANE_BUG_NEW,
            ),
        ),
    ),
    "compound-archive-hidden": Fixture(
        name="compound-archive-hidden",
        task_id="self-correction-compound-archive-hidden-regressions",
        description=FIBONACCI_DESCRIPTION,
        verify_command=(
            "cargo test -p a2_core test_fibonacci; core=$?; "
            "cargo test -p a2_archive filters_by_task_and_orders_recent_records; archive=$?; "
            "test $core -eq 0 -a $archive -eq 0"
        ),
        replacements=(
            Replacement(
                "crates/a2_core/src/lib.rs",
                FIBONACCI_BUG_OLD,
                FIBONACCI_BUG_NEW,
            ),
            Replacement(
                "crates/a2_archive/src/store.rs",
                ARCHIVE_BUG_OLD,
                ARCHIVE_BUG_NEW,
            ),
        ),
    ),
    "compound-sensorium-same-crate-hidden": Fixture(
        name="compound-sensorium-same-crate-hidden",
        task_id=SENSORIUM_TASK_ID,
        description=SENSORIUM_DESCRIPTION,
        verify_command=(
            "cargo test -p a2_sensorium high_risk_gets_low_priority; priority=$?; "
            "cargo test -p a2_sensorium long_content_truncated_in_title; title=$?; "
            "test $priority -eq 0 -a $title -eq 0"
        ),
        replacements=(
            Replacement(
                "crates/a2_sensorium/src/ingest.rs",
                SENSORIUM_PRIORITY_BUG_OLD,
                SENSORIUM_PRIORITY_BUG_NEW,
            ),
            Replacement(
                "crates/a2_sensorium/src/ingest.rs",
                SENSORIUM_TRUNCATE_BUG_OLD,
                SENSORIUM_TRUNCATE_BUG_NEW,
            ),
        ),
    ),
}


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
    parser.add_argument(
        "--fixture",
        choices=sorted(FIXTURES),
        default="fibonacci",
        help="Bug fixture to inject.",
    )
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
    parser.add_argument(
        "--disable-anti-repeat",
        action="store_true",
        help=(
            "Ablation mode: pass --disable-anti-repeat-retry to a2ctl run, "
            "leaving candidate verifiers and other retry context enabled."
        ),
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


def latest_lineage_patch_diff(workspace: Path, task_id: str) -> str | None:
    db = workspace / "lineage.sqlite"
    if not db.exists():
        return None

    with sqlite3.connect(db) as connection:
        try:
            row = connection.execute(
                """
                SELECT patch_diff
                FROM lineage_records
                WHERE task_id = ?
                ORDER BY created_at DESC
                LIMIT 1
                """,
                (serialized_task_id(task_id),),
            ).fetchone()
        except sqlite3.Error:
            return None
    return row[0] if row and row[0] else None


def diff_stats(diff: str | None) -> dict[str, Any]:
    touched_files: list[str] = []
    touched_seen: set[str] = set()
    added_lines = 0
    removed_lines = 0

    if diff:
        for line in diff.splitlines():
            if line.startswith("diff --git "):
                parts = line.split()
                if len(parts) >= 4:
                    path = parts[3]
                    if path.startswith("b/"):
                        path = path[2:]
                    if path not in touched_seen:
                        touched_seen.add(path)
                        touched_files.append(path)
            elif line.startswith("+++") and not touched_files:
                path = line.removeprefix("+++ ").strip()
                if path.startswith("b/"):
                    path = path[2:]
                if path != "/dev/null" and path not in touched_seen:
                    touched_seen.add(path)
                    touched_files.append(path)
            elif line.startswith("+") and not line.startswith("+++"):
                added_lines += 1
            elif line.startswith("-") and not line.startswith("---"):
                removed_lines += 1

    return {
        "touched_files": touched_files,
        "touched_file_count": len(touched_files),
        "diff_added_lines": added_lines,
        "diff_removed_lines": removed_lines,
    }


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


def inject_fixture(workspace: Path, fixture: Fixture) -> None:
    for replacement in fixture.replacements:
        path = workspace / replacement.path
        content = path.read_text(encoding="utf-8")
        if replacement.new in content:
            continue
        if replacement.old not in content:
            raise RuntimeError(f"bug fixture target not found in {path}")
        path.write_text(content.replace(replacement.old, replacement.new, 1), encoding="utf-8")


def commit_bug(workspace: Path) -> None:
    git(["add", "-A"], workspace)
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


def task_payload(fixture: Fixture, run_id: str, attempt: int) -> dict[str, Any]:
    return {
        "task_id": fixture.task_id,
        "problem_statement": fixture.description,
        "verification_commands": [
            {
                "command": fixture.verify_command,
                "expect_exit": 0,
            }
        ],
        "category": CATEGORY,
        "fixture": fixture.name,
        "run_id": run_id,
        "attempt": attempt,
    }


def run_a2_attempt(
    workspace: Path,
    provider: str,
    max_tokens: int,
    timeout: int,
    payload: dict[str, Any],
    *,
    disable_anti_repeat: bool,
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
    if disable_anti_repeat:
        command.append("--disable-anti-repeat-retry")
    return run_command(command, workspace, stdin=json.dumps(payload) + "\n", timeout=timeout + 900)


def verify(workspace: Path, fixture: Fixture) -> CommandResult:
    return run_command(fixture.verify_command, workspace, shell=True, timeout=300)


def result_record(
    *,
    payload: dict[str, Any],
    provider: str,
    workspace: Path,
    a2_result: CommandResult | None,
    verify_result: CommandResult,
    lineage_before: int,
    lineage_after: int,
    lineage_reconciled_by_core: bool,
    patch_stats: dict[str, Any],
    anti_repeat_retry_enabled: bool,
) -> dict[str, Any]:
    return {
        "task_id": payload["task_id"],
        "category": payload["category"],
        "fixture": payload.get("fixture"),
        "run_id": payload["run_id"],
        "attempt": payload["attempt"],
        "provider": provider,
        "model": provider,
        "resolved": verify_result.returncode == 0,
        "prior_lineage_present": lineage_before > 0,
        "lineage_records_before": lineage_before,
        "lineage_records_after": lineage_after,
        "lineage_reconciled_by_core": lineage_reconciled_by_core,
        "anti_repeat_retry_enabled": anti_repeat_retry_enabled,
        "ablation": None if anti_repeat_retry_enabled else "anti_repeat_retry_disabled",
        **patch_stats,
        "workspace": str(workspace),
        "a2_returncode": a2_result.returncode if a2_result else None,
        "a2_duration_secs": round(a2_result.duration_secs, 3) if a2_result else 0.0,
        "verify_command": str(verify_result.command),
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
    fixture = FIXTURES[args.fixture]
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
        inject_fixture(workspace, fixture)
        commit_bug(workspace)

        initial = verify(workspace, fixture)
        if initial.returncode == 0:
            raise RuntimeError("bug fixture did not fail before A² attempts")

        attempts = 1 if args.smoke_only else max(args.attempts, 1)
        for attempt in range(1, attempts + 1):
            payload = task_payload(fixture, run_id, attempt)
            lineage_before = lineage_count(workspace, fixture.task_id)
            a2_result = (
                None
                if args.smoke_only
                else run_a2_attempt(
                    workspace,
                    args.provider,
                    args.max_tokens,
                    args.timeout,
                    payload,
                    disable_anti_repeat=args.disable_anti_repeat,
                )
            )
            verified = verify(workspace, fixture)
            patch_stats = diff_stats(latest_lineage_patch_diff(workspace, fixture.task_id))
            lineage_after = lineage_count(workspace, fixture.task_id)
            lineage_reconciled_by_core = a2_result is not None and (
                "[applied and rebuilt:" in a2_result.stderr
                or "[apply/rebuild failed for" in a2_result.stderr
            )
            record = result_record(
                payload=payload,
                provider=args.provider,
                workspace=workspace,
                a2_result=a2_result,
                verify_result=verified,
                lineage_before=lineage_before,
                lineage_after=lineage_after,
                lineage_reconciled_by_core=lineage_reconciled_by_core,
                patch_stats=patch_stats,
                anti_repeat_retry_enabled=not args.disable_anti_repeat,
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
        fixture = FIXTURES["fibonacci"]
        first = task_payload(fixture, "run", 1)
        second = task_payload(fixture, "run", 2)
        self.assertEqual(first["task_id"], second["task_id"])
        self.assertEqual(first["run_id"], second["run_id"])
        self.assertEqual(second["attempt"], 2)

    def test_task_payload_carries_fixture_verifier_command(self) -> None:
        fixture = FIXTURES["compound-hidden"]
        payload = task_payload(fixture, "run", 1)
        self.assertEqual(
            payload["verification_commands"],
            [{"command": fixture.verify_command, "expect_exit": 0}],
        )

    def test_compound_archive_fixture_checks_archive_regression(self) -> None:
        fixture = FIXTURES["compound-archive-hidden"]
        self.assertEqual(fixture.task_id, "self-correction-compound-archive-hidden-regressions")
        self.assertIn("a2_archive", fixture.verify_command)
        self.assertIn("filters_by_task_and_orders_recent_records", fixture.verify_command)
        self.assertEqual(
            [replacement.path for replacement in fixture.replacements],
            ["crates/a2_core/src/lib.rs", "crates/a2_archive/src/store.rs"],
        )

    def test_compound_sensorium_fixture_is_same_crate_multi_bug(self) -> None:
        fixture = FIXTURES["compound-sensorium-same-crate-hidden"]
        self.assertEqual(fixture.task_id, SENSORIUM_TASK_ID)
        self.assertIn("high_risk_gets_low_priority", fixture.verify_command)
        self.assertIn("long_content_truncated_in_title", fixture.verify_command)
        self.assertEqual(
            [replacement.path for replacement in fixture.replacements],
            ["crates/a2_sensorium/src/ingest.rs", "crates/a2_sensorium/src/ingest.rs"],
        )

    def test_result_record_reports_prior_lineage(self) -> None:
        payload = task_payload(FIXTURES["fibonacci"], "run", 2)
        verify_result = CommandResult(FIBONACCI_VERIFY_COMMAND, 0, "ok", "", 1.25)
        record = result_record(
            payload=payload,
            provider="gemini",
            workspace=Path("/tmp/workspace"),
            a2_result=None,
            verify_result=verify_result,
            lineage_before=1,
            lineage_after=2,
            lineage_reconciled_by_core=True,
            patch_stats={
                "touched_files": ["crates/a2_core/src/lib.rs"],
                "touched_file_count": 1,
                "diff_added_lines": 1,
                "diff_removed_lines": 1,
            },
            anti_repeat_retry_enabled=False,
        )
        self.assertTrue(record["resolved"])
        self.assertTrue(record["prior_lineage_present"])
        self.assertEqual(record["lineage_records_after"], 2)
        self.assertTrue(record["lineage_reconciled_by_core"])
        self.assertFalse(record["anti_repeat_retry_enabled"])
        self.assertEqual(record["ablation"], "anti_repeat_retry_disabled")
        self.assertEqual(record["touched_files"], ["crates/a2_core/src/lib.rs"])
        self.assertEqual(record["diff_added_lines"], 1)
        self.assertEqual(record["diff_removed_lines"], 1)

    def test_diff_stats_reports_touched_files_and_line_counts(self) -> None:
        stats = diff_stats(
            """
diff --git a/crates/a2_core/src/lib.rs b/crates/a2_core/src/lib.rs
--- a/crates/a2_core/src/lib.rs
+++ b/crates/a2_core/src/lib.rs
@@ -1,2 +1,2 @@
-old
+new
diff --git a/crates/a2ctl/src/main.rs b/crates/a2ctl/src/main.rs
--- a/crates/a2ctl/src/main.rs
+++ b/crates/a2ctl/src/main.rs
@@ -10,0 +11,2 @@
+first
+second
"""
        )
        self.assertEqual(
            stats["touched_files"],
            ["crates/a2_core/src/lib.rs", "crates/a2ctl/src/main.rs"],
        )
        self.assertEqual(stats["touched_file_count"], 2)
        self.assertEqual(stats["diff_added_lines"], 3)
        self.assertEqual(stats["diff_removed_lines"], 1)


if __name__ == "__main__":
    if sys.argv[1:2] == ["--self-test"]:
        sys.argv = [sys.argv[0]]
        raise SystemExit(unittest.main())
    raise SystemExit(run_benchmark(parse_args(sys.argv[1:])))
