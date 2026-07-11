#!/usr/bin/env python3
"""Bounded verification runner with retained process-cleanup diagnostics.

This is an operator/developer harness for A² remediation Phase 0. It runs a
trusted argv without a shell, gives it a fresh process group, retains bounded
stdout/stderr tails, and always atomically writes a machine-readable report.
"""

from __future__ import annotations

import argparse
from contextlib import nullcontext
import json
import math
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from unittest import mock
from typing import Any, Iterable
import uuid

SCHEMA = "a2.phase0-liveness.v1"
DEFAULT_TAIL_BYTES = 64 * 1024
PROCESS_SNAPSHOT_TIMEOUT_SECS = 3
RELEVANT_EXECUTABLES = {
    "cargo",
    "rustc",
    "python",
    "python3",
    "claude",
    "codex",
    "gemini",
    "opencode",
    "pi",
}


def _utc_now() -> str:
    import datetime

    return datetime.datetime.now(datetime.timezone.utc).isoformat()


def _atomic_write_json(path: Path, value: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    payload = json.dumps(value, indent=2, sort_keys=True) + "\n"
    fd, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as handle:
            handle.write(payload)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary, path)
        directory_fd = os.open(path.parent, os.O_RDONLY)
        try:
            os.fsync(directory_fd)
        finally:
            os.close(directory_fd)
    except BaseException:
        try:
            os.unlink(temporary)
        except FileNotFoundError:
            pass
        raise


def _run_probe(argv: list[str], cwd: Path, timeout: float = 5) -> dict[str, Any]:
    try:
        completed = subprocess.run(
            argv,
            cwd=cwd,
            text=True,
            capture_output=True,
            timeout=timeout,
            check=False,
        )
        return {
            "argv": argv,
            "returncode": completed.returncode,
            "stdout": completed.stdout.strip(),
            "stderr": completed.stderr.strip(),
            "timed_out": False,
        }
    except subprocess.TimeoutExpired as error:
        return {
            "argv": argv,
            "returncode": None,
            "stdout": _decoded(error.stdout).strip(),
            "stderr": _decoded(error.stderr).strip(),
            "timed_out": True,
        }
    except OSError as error:
        return {
            "argv": argv,
            "returncode": None,
            "stdout": "",
            "stderr": "",
            "timed_out": False,
            "spawn_error": f"{type(error).__name__}: {error}",
        }


def _decoded(value: str | bytes | None) -> str:
    if value is None:
        return ""
    if isinstance(value, bytes):
        return value.decode("utf-8", errors="replace")
    return value


def _source_revision(cwd: Path) -> dict[str, Any]:
    root = _run_probe(["git", "rev-parse", "--show-toplevel"], cwd)
    head = _run_probe(["git", "rev-parse", "HEAD"], cwd)
    return {
        "workspace_root": root.get("stdout") or None,
        "head": head.get("stdout") or None,
        "root_probe": root,
        "head_probe": head,
    }


def _process_table(
    timeout: float = PROCESS_SNAPSHOT_TIMEOUT_SECS,
) -> tuple[list[dict[str, Any]], str | None]:
    probe = _run_probe(
        ["ps", "-axo", "pid=,ppid=,pgid=,lstart=,state=,command="],
        Path.cwd(),
        timeout=timeout,
    )
    if probe.get("timed_out") or probe.get("returncode") != 0:
        return [], json.dumps(probe, sort_keys=True)

    rows: list[dict[str, Any]] = []
    for raw in probe["stdout"].splitlines():
        # lstart is five whitespace-separated fields: weekday month day time year.
        parts = raw.strip().split(None, 9)
        if len(parts) != 10:
            continue
        try:
            pid, ppid, pgid = (int(parts[index]) for index in range(3))
        except ValueError:
            continue
        rows.append(
            {
                "pid": pid,
                "ppid": ppid,
                "pgid": pgid,
                "started_at": " ".join(parts[3:8]),
                "state": parts[8],
                "command": parts[9],
            }
        )
    return rows, None


def _command_basename(row: dict[str, Any]) -> str:
    command = str(row.get("command", "")).split(None, 1)[0].replace("\\", "/")
    return Path(command).name.lower()


def _executable_under_target(row: dict[str, Any], target_dir: Path | None) -> bool:
    if target_dir is None:
        return False
    command = str(row.get("command", ""))
    executable_text = command.split(None, 1)[0] if command else ""
    if not executable_text:
        return False
    try:
        Path(executable_text).resolve(strict=False).relative_to(
            target_dir.resolve(strict=False)
        )
        return True
    except (OSError, ValueError):
        return False


def _is_relevant_process(
    row: dict[str, Any], target_dir: Path | None = None
) -> bool:
    basename = _command_basename(row).removesuffix(".exe")
    command = str(row.get("command", ""))
    command_lower = command.lower()
    if (
        basename in RELEVANT_EXECUTABLES
        or basename.startswith("cargo-")
        or basename.startswith("python")
        or _executable_under_target(row, target_dir)
    ):
        return True

    # Provider CLIs are frequently JavaScript entrypoints hosted by node/bun.
    if basename in {"node", "nodejs", "npx", "bun", "deno"}:
        provider_markers = ("claude", "codex", "gemini", "opencode", "pi")
        provider_package_markers = (
            "anthropic-ai/claude-code",
            "claude-code",
            "openai/codex",
            "google-gemini",
            "google/gemini-cli",
            "opencode-ai",
            "pi-coding-agent",
        )
        argument_text = command_lower.split(None, 1)[1] if " " in command_lower else ""
        argument_tokens = argument_text.replace("\\", "/").split()
        if any(
            Path(token.strip("'\"")).name in provider_markers
            or any(f"/{provider}/" in token for provider in provider_markers)
            or any(package in token for package in provider_package_markers)
            for token in argument_tokens
        ):
            return True

    # Compatibility heuristic for conventional Cargo target paths.
    return "/target/" in command_lower or "/target-" in command_lower


def _descendant_pids(rows: list[dict[str, Any]], root_pid: int) -> set[int]:
    descendants = {root_pid}
    changed = True
    while changed:
        changed = False
        for row in rows:
            if row["ppid"] in descendants and row["pid"] not in descendants:
                descendants.add(row["pid"])
                changed = True
    return descendants


def _owned_rows(
    rows: list[dict[str, Any]], root_pid: int, pgid: int, tracked_pids: set[int]
) -> list[dict[str, Any]]:
    descendants = _descendant_pids(rows, root_pid)
    owned = [
        row
        for row in rows
        if row["pgid"] == pgid
        or row["pid"] in tracked_pids
        or row["pid"] in descendants
    ]
    return sorted(owned, key=lambda row: row["pid"])


def _record_observed_processes(
    observed: dict[int, dict[str, Any]], rows: Iterable[dict[str, Any]]
) -> None:
    for row in rows:
        observed.setdefault(row["pid"], row.copy())


def _process_alive(pid: int) -> bool:
    try:
        os.kill(pid, 0)
        return True
    except ProcessLookupError:
        return False
    except PermissionError:
        return True


def _process_identity(row: dict[str, Any]) -> tuple[int, str | None]:
    return row["pid"], row.get("started_at")


def _token_owned_pids(
    token: str,
    baseline_identities: set[tuple[int, str | None]],
    timeout: float = 3,
) -> tuple[set[int], str | None]:
    """Find new processes that inherited this invocation's unguessable token.

    First take a non-environment process snapshot, then inspect environments only
    for identities absent from the pre-launch baseline. This avoids retaining or
    bulk-reading unrelated users' environments and handles PID reuse via lstart.
    """
    started = time.monotonic()
    rows, error = _process_table(timeout=min(timeout, 2.0))
    if error:
        return set(), f"ownership-token candidate scan failed: {error}"
    candidates = [
        row for row in rows if _process_identity(row) not in baseline_identities
    ]
    pids: set[int] = set()
    for row in candidates:
        remaining = timeout - (time.monotonic() - started)
        if remaining <= 0:
            return set(), "ownership-token identity scan timed out"
        matches, identity_error = _pid_has_token(
            row["pid"], token, timeout=min(0.5, remaining)
        )
        if identity_error:
            return set(), identity_error
        if matches:
            pids.add(row["pid"])
    return pids, None


def _pid_has_token(
    pid: int, token: str, timeout: float = 1
) -> tuple[bool, str | None]:
    try:
        completed = subprocess.run(
            ["ps", "eww", "-p", str(pid), "-o", "command="],
            text=True,
            capture_output=True,
            timeout=timeout,
            check=False,
        )
    except subprocess.TimeoutExpired:
        return False, f"ownership-token identity check timed out for pid {pid}"
    except OSError as error:
        return False, f"ownership-token identity check failed for pid {pid}: {error}"
    if completed.returncode not in {0, 1}:
        return False, f"ownership-token identity check exited {completed.returncode} for pid {pid}"
    return f"A2_LIVENESS_RUN_ID={token}" in completed.stdout, None


def _signal_token_processes(
    pids: Iterable[int], token: str, sig: signal.Signals
) -> list[str]:
    actions: list[str] = []
    for pid in sorted(set(pids)):
        # Revalidate identity immediately before signalling to avoid PID-reuse
        # hazards on a shared host.
        matches, identity_error = _pid_has_token(pid, token)
        if identity_error:
            actions.append(f"kill({pid}, {sig.name}):refused_unverified_identity:{identity_error}")
            continue
        if not matches:
            actions.append(f"kill({pid}, {sig.name}):refused_identity_mismatch")
            continue
        try:
            os.kill(pid, sig)
            actions.append(f"kill({pid}, {sig.name}):ownership_token")
        except ProcessLookupError:
            actions.append(f"kill({pid}, {sig.name}):already_gone")
        except PermissionError as error:
            actions.append(f"kill({pid}, {sig.name}):permission_error:{error}")
    return actions


def _group_alive(pgid: int) -> bool:
    try:
        os.killpg(pgid, 0)
        return True
    except ProcessLookupError:
        return False
    except PermissionError:
        return True


def _signal_process_group(
    pgid: int, sig: signal.Signals, *, group_isolated: bool
) -> list[str]:
    """Signal only a group proved to be the fresh child-owned session.

    Bare tracked PIDs are deliberately not signalled: PID reuse on a shared host
    could otherwise kill an unrelated process. An escaped child is a fail-closed
    orphan result requiring operator cleanup, not a license to signal by PID.
    """
    if not group_isolated:
        return [f"killpg({pgid}, {sig.name}):refused_unverified_group"]
    try:
        os.killpg(pgid, sig)
        return [f"killpg({pgid}, {sig.name})"]
    except ProcessLookupError:
        return [f"killpg({pgid}, {sig.name}):already_gone"]
    except PermissionError as error:
        return [f"killpg({pgid}, {sig.name}):permission_error:{error}"]


def _path_within(path: Path, root: Path) -> bool:
    try:
        path.resolve(strict=False).relative_to(root.resolve(strict=False))
        return True
    except (OSError, ValueError):
        return False


def _process_cwd(pid: int, timeout: float = 1) -> tuple[str | None, str | None]:
    proc_cwd = Path(f"/proc/{pid}/cwd")
    try:
        if proc_cwd.exists():
            return str(proc_cwd.resolve(strict=True)), None
    except OSError:
        pass
    try:
        completed = subprocess.run(
            ["lsof", "-a", "-p", str(pid), "-d", "cwd", "-Fn"],
            text=True,
            capture_output=True,
            timeout=timeout,
            check=False,
        )
    except subprocess.TimeoutExpired:
        return None, f"cwd attribution timed out for pid {pid}"
    except OSError as error:
        return None, f"cwd attribution failed for pid {pid}: {error}"
    if completed.returncode != 0:
        if not _process_alive(pid):
            return None, "gone"
        return None, f"cwd attribution exited {completed.returncode} for pid {pid}"
    for line in completed.stdout.splitlines():
        if line.startswith("n") and len(line) > 1:
            return line[1:], None
    if not _process_alive(pid):
        return None, "gone"
    return None, f"cwd attribution missing for pid {pid}"


def _target_state(target_dir: Path) -> dict[str, Any]:
    started = time.monotonic()
    state: dict[str, Any] = {
        "path": str(target_dir),
        "resolved_path": str(target_dir.resolve(strict=False)),
        "exists": target_dir.exists(),
        "probe_duration_secs": None,
        "locks": [],
    }
    try:
        stat = os.statvfs(target_dir if target_dir.exists() else target_dir.parent)
        state["filesystem"] = {
            "block_size": stat.f_frsize,
            "blocks": stat.f_blocks,
            "blocks_available": stat.f_bavail,
            "bytes_available": stat.f_bavail * stat.f_frsize,
        }
        lock_candidates = [
            target_dir / ".cargo-lock",
            target_dir / "debug" / ".cargo-lock",
            target_dir / "release" / ".cargo-lock",
        ]
        for lock in lock_candidates:
            if lock.exists():
                metadata = lock.stat()
                state["locks"].append(
                    {
                        "path": str(lock),
                        "size": metadata.st_size,
                        "mtime_ns": metadata.st_mtime_ns,
                    }
                )
    except OSError as error:
        state["probe_error"] = f"{type(error).__name__}: {error}"
    state["probe_duration_secs"] = round(time.monotonic() - started, 6)
    return state


def _read_tail(path: Path, limit: int) -> dict[str, Any]:
    size = path.stat().st_size
    with path.open("rb") as handle:
        if size > limit:
            handle.seek(size - limit)
        data = handle.read()
    return {
        "bytes": size,
        "tail_bytes": len(data),
        "truncated": size > limit,
        "tail": data.decode("utf-8", errors="replace"),
    }


def _diagnose(
    *,
    timed_out: bool,
    returncode: int | None,
    observed: Iterable[dict[str, Any]],
    preexisting_relevant: Iterable[dict[str, Any]],
    stdout_tail: str,
    stderr_tail: str,
    target_after: dict[str, Any],
    target_dir: Path,
    orphan_rows: list[dict[str, Any]],
) -> dict[str, Any]:
    if orphan_rows:
        return {
            "category": "harness_deadlock_or_orphan_escape",
            "confidence": "high",
            "reasons": ["owned verification processes survived cleanup"],
        }
    if not timed_out:
        return {
            "category": "completed" if returncode == 0 else "command_failure",
            "confidence": "high",
            "reasons": [f"command exited with returncode {returncode}"],
        }

    combined = f"{stdout_tail}\n{stderr_tail}".lower()
    basenames = {_command_basename(row) for row in observed}
    preexisting_cargo = [
        row for row in preexisting_relevant if _command_basename(row) == "cargo"
    ]
    if preexisting_cargo and "blocking waiting for file lock" in combined:
        return {
            "category": "suspected_build_lock_contention",
            "confidence": "medium",
            "reasons": [
                "Cargo explicitly reported waiting for a file lock",
                "pre-existing Cargo process(es) were present",
            ],
        }
    if "rustc" in basenames:
        return {
            "category": "suspected_filesystem_or_target_pathology",
            "confidence": "low",
            "reasons": [
                "rustc was observed before the wall-clock bound; this may also be a healthy build given an undersized timeout",
                f"target probe duration was {target_after.get('probe_duration_secs')} seconds",
            ],
        }
    test_processes = [
        row
        for row in observed
        if (
            _executable_under_target(row, target_dir)
            or "/target/" in str(row.get("command", "")).lower()
        )
        and _command_basename(row) not in {"cargo", "rustc"}
    ]
    if test_processes:
        return {
            "category": "suspected_test_deadlock",
            "confidence": "low",
            "reasons": [
                "a compiled target/test process was observed before the wall-clock bound; this may also be a healthy slow test"
            ],
        }
    return {
        "category": "suspected_harness_or_command_deadlock",
        "confidence": "low",
        "reasons": ["the command timed out without observed compiler or test-child activity"],
    }


def run_bounded(
    *,
    command: list[str],
    cwd: Path,
    timeout_secs: float,
    grace_secs: float,
    report_path: Path,
    target_dir: Path,
    tail_bytes: int = DEFAULT_TAIL_BYTES,
    poll_interval_secs: float = 0.25,
) -> dict[str, Any]:
    if not command:
        raise ValueError("command must not be empty")
    if (
        not math.isfinite(timeout_secs)
        or not math.isfinite(grace_secs)
        or timeout_secs <= 0
        or grace_secs < 0
        or tail_bytes <= 0
    ):
        raise ValueError(
            "timeout and tail limits must be finite and positive; grace may be zero"
        )

    cwd = cwd.resolve()
    target_dir = target_dir if target_dir.is_absolute() else cwd / target_dir
    report_path = report_path if report_path.is_absolute() else cwd / report_path
    invocation_started = time.monotonic()
    ownership_token = uuid.uuid4().hex
    report: dict[str, Any] = {
        "schema": SCHEMA,
        "state": "initializing",
        "passed": False,
        "command": command,
        "cwd": str(cwd),
        "target_dir": str(target_dir),
        "report_path": str(report_path),
        "started_at": _utc_now(),
        "timeout_secs": timeout_secs,
        "grace_secs": grace_secs,
        "tail_limit_bytes": tail_bytes,
        "containment": {
            "process_group": "fresh child-owned session",
            "escaped_descendant_detection": "inherited A2_LIVENESS_RUN_ID ownership token",
            "ownership_token": ownership_token,
        },
        "process_snapshot_errors": [],
    }
    # A recognizable, non-passing artifact exists before any diagnostic probe or
    # child launch. If the harness itself crashes, stale success cannot survive.
    _atomic_write_json(report_path, report)

    before_rows, before_error = _process_table()
    if before_error:
        report["process_snapshot_errors"].append(before_error)
    preexisting_relevant = [
        row for row in before_rows if _is_relevant_process(row, target_dir)
    ]
    baseline_identities = {_process_identity(row) for row in before_rows}
    report.update(
        {
            "source": _source_revision(cwd),
            "target_before": _target_state(target_dir),
            "preexisting_relevant_processes": preexisting_relevant,
        }
    )

    with tempfile.TemporaryDirectory(prefix="a2-phase0-liveness-") as temporary:
        stdout_path = Path(temporary) / "stdout"
        stderr_path = Path(temporary) / "stderr"
        process: subprocess.Popen[bytes] | None = None
        observed: dict[int, dict[str, Any]] = {}
        tracked_pids: set[int] = set()
        cleanup_actions: list[str] = []
        timed_out = False
        timeout_snapshot: list[dict[str, Any]] = []
        last_owned_snapshot: list[dict[str, Any]] = []
        spawn_error: str | None = None
        harness_error: str | None = None
        pgid: int | None = None
        group_isolated = False
        command_started: float | None = None
        deadline: float | None = None
        timeout_enforced_at: float | None = None

        with stdout_path.open("wb") as stdout_file, stderr_path.open("wb") as stderr_file:
            try:
                environment = os.environ.copy()
                environment["CARGO_TARGET_DIR"] = str(target_dir)
                environment["PYTHONUNBUFFERED"] = "1"
                environment["A2_LIVENESS_RUN_ID"] = ownership_token
                process = subprocess.Popen(
                    command,
                    cwd=cwd,
                    env=environment,
                    stdin=subprocess.DEVNULL,
                    stdout=stdout_file,
                    stderr=stderr_file,
                    start_new_session=True,
                )
                command_started = time.monotonic()
                deadline = command_started + timeout_secs
                pgid = os.getpgid(process.pid)
                session_id = os.getsid(process.pid)
                group_isolated = pgid == process.pid and session_id == process.pid
                report.update(
                    {
                        "state": "running",
                        "command_started_at": _utc_now(),
                        "pid": process.pid,
                        "pgid": pgid,
                        "session_id": session_id,
                        "group_isolated": group_isolated,
                    }
                )
                # Do not perform report/filesystem I/O while the child is live:
                # deadline enforcement must not depend on a potentially stalled
                # fsync. The already-written initializing report remains a clear
                # non-passing crash artifact until finalization.
                if not group_isolated:
                    raise RuntimeError(
                        "start_new_session did not create a child-owned session/process group"
                    )

                next_snapshot = 0.0
                while process.poll() is None:
                    now = time.monotonic()
                    # Enforce the command deadline before optional diagnostics so
                    # a wedged `ps` probe cannot postpone process termination.
                    if now >= deadline:
                        timed_out = True
                        timeout_enforced_at = now
                        timeout_snapshot = last_owned_snapshot
                        cleanup_actions.extend(
                            _signal_process_group(
                                pgid, signal.SIGTERM, group_isolated=group_isolated
                            )
                        )
                        break
                    if now >= next_snapshot:
                        rows, error = _process_table(
                            timeout=min(0.25, max(0.05, deadline - now))
                        )
                        if error:
                            report["process_snapshot_errors"].append(error)
                        else:
                            last_owned_snapshot = _owned_rows(
                                rows, process.pid, pgid, tracked_pids
                            )
                            _record_observed_processes(observed, last_owned_snapshot)
                            tracked_pids.update(row["pid"] for row in last_owned_snapshot)
                        next_snapshot = now + 1.0
                    time.sleep(min(poll_interval_secs, max(0.0, deadline - now)))

                # Once the group leader exits, a numeric PGID could eventually
                # be reused on a shared host. Do not signal that number again;
                # surviving descendants are handled below by revalidated token
                # identity. While the original leader remains live, killpg is
                # safe because its PID is still the group ID and cannot be reused.
                group_still_alive = _group_alive(pgid)
                if group_still_alive and not timed_out:
                    cleanup_actions.append("descendants_survived_parent_exit")

                if group_still_alive and timed_out:
                    grace_deadline = time.monotonic() + grace_secs
                    while (
                        time.monotonic() < grace_deadline
                        and _group_alive(pgid)
                        and process.poll() is None
                    ):
                        time.sleep(min(poll_interval_secs, 0.05))
                    if _group_alive(pgid) and process.poll() is None:
                        cleanup_actions.extend(
                            _signal_process_group(
                                pgid, signal.SIGKILL, group_isolated=group_isolated
                            )
                        )

                try:
                    process.wait(timeout=max(0.5, grace_secs + 0.5))
                except subprocess.TimeoutExpired:
                    cleanup_actions.append("root_wait_timeout_after_cleanup")
                    if (
                        group_isolated
                        and process.poll() is None
                        and _group_alive(pgid)
                    ):
                        cleanup_actions.extend(
                            _signal_process_group(
                                pgid, signal.SIGKILL, group_isolated=True
                            )
                        )
                    try:
                        process.wait(timeout=0.5)
                    except subprocess.TimeoutExpired:
                        harness_error = "root process could not be reaped after SIGKILL"
            except OSError as error:
                spawn_error = f"{type(error).__name__}: {error}"
            except BaseException as error:  # retain a report even for harness faults
                harness_error = f"{type(error).__name__}: {error}"
            finally:
                if process is not None and process.poll() is None:
                    if group_isolated and pgid is not None:
                        cleanup_actions.extend(
                            _signal_process_group(
                                pgid, signal.SIGKILL, group_isolated=True
                            )
                        )
                    else:
                        process.kill()
                        cleanup_actions.append("kill(root):unverified_process_group")
                    try:
                        process.wait(timeout=0.5)
                    except subprocess.TimeoutExpired:
                        harness_error = harness_error or "final root reap timed out"

        # Detect and clean descendants that escaped the original process group
        # with setsid(). Identity is revalidated by inherited per-run token
        # immediately before any PID-directed signal.
        token_pids, token_scan_error = _token_owned_pids(
            ownership_token, baseline_identities
        )
        root_pid = process.pid if process is not None else None
        escaped_pids = token_pids - ({root_pid} if root_pid is not None else set())
        if escaped_pids:
            cleanup_actions.append("ownership_token_descendants_survived_process_group")
            cleanup_actions.extend(
                _signal_token_processes(escaped_pids, ownership_token, signal.SIGTERM)
            )
            token_grace_deadline = time.monotonic() + grace_secs
            while time.monotonic() < token_grace_deadline:
                remaining, scan_error = _token_owned_pids(
                    ownership_token,
                    baseline_identities,
                    timeout=max(1.0, min(3.0, grace_secs + 1.0)),
                )
                if scan_error:
                    token_scan_error = scan_error
                    break
                remaining.discard(root_pid)
                if not remaining:
                    break
                time.sleep(min(poll_interval_secs, 0.05))
            remaining, scan_error = _token_owned_pids(
                ownership_token, baseline_identities
            )
            if scan_error:
                token_scan_error = scan_error
            else:
                remaining.discard(root_pid)
                if remaining:
                    cleanup_actions.extend(
                        _signal_token_processes(
                            remaining, ownership_token, signal.SIGKILL
                        )
                    )
                    time.sleep(0.05)
        final_token_pids, final_token_scan_error = _token_owned_pids(
            ownership_token, baseline_identities
        )
        if root_pid is not None:
            final_token_pids.discard(root_pid)
        if final_token_scan_error:
            token_scan_error = final_token_scan_error

        stdout = _read_tail(stdout_path, tail_bytes)
        stderr = _read_tail(stderr_path, tail_bytes)
        after_rows, after_error = _process_table()
        if after_error:
            report["process_snapshot_errors"].append(after_error)
        after_by_pid = {row["pid"]: row for row in after_rows}
        owned_orphans = [
            after_by_pid.get(
                pid,
                {
                    "pid": pid,
                    "ppid": None,
                    "pgid": None,
                    "state": "unknown",
                    "command": "ownership-token descendant missing from final process table",
                },
            )
            for pid in sorted(final_token_pids)
        ]
        new_relevant_candidates = [
            row.copy()
            for row in after_rows
            if _process_identity(row) not in baseline_identities
            and row["pid"] not in final_token_pids
            and not str(row["state"]).startswith("Z")
            and _is_relevant_process(row, target_dir)
        ]
        new_relevant_local: list[dict[str, Any]] = []
        new_relevant_external: list[dict[str, Any]] = []
        new_relevant_unverified: list[dict[str, Any]] = []
        for row in new_relevant_candidates:
            process_cwd, cwd_error = _process_cwd(row["pid"])
            row["cwd"] = process_cwd
            row["cwd_attribution_error"] = None if cwd_error == "gone" else cwd_error
            if cwd_error == "gone":
                continue
            if _executable_under_target(row, target_dir) or (
                process_cwd is not None and _path_within(Path(process_cwd), cwd)
            ):
                new_relevant_local.append(row)
            elif cwd_error is not None:
                # Attribution uncertainty is fail-closed but never authorizes a
                # signal against a possibly unrelated shared-host process.
                new_relevant_unverified.append(row)
            else:
                new_relevant_external.append(row)
        orphan_by_identity = {
            (row["pid"], row.get("started_at")): row
            for row in owned_orphans
            + new_relevant_local
            + new_relevant_unverified
        }
        orphan_rows = list(orphan_by_identity.values())
        group_absent = pgid is None or (group_isolated and not _group_alive(pgid))
        baseline_verified = before_error is None
        snapshot_verified = after_error is None
        token_attribution_verified = token_scan_error is None
        cleanup_complete = (
            baseline_verified
            and snapshot_verified
            and token_attribution_verified
            and group_absent
            and not orphan_rows
            and (process is None or group_isolated)
        )
        returncode = process.returncode if process is not None else None
        target_after = _target_state(target_dir)
        cleanup_result = (
            "unverified"
            if not baseline_verified
            or not snapshot_verified
            or not token_attribution_verified
            else "orphans_remaining"
            if orphan_rows or not group_absent
            else "clean_after_sigkill"
            if any("SIGKILL" in action for action in cleanup_actions)
            else "clean_after_sigterm"
            if cleanup_actions
            else "not_needed"
        )
        command_duration = (
            round(time.monotonic() - command_started, 6)
            if command_started is not None
            else None
        )
        enforcement_delay = (
            round(max(0.0, timeout_enforced_at - deadline), 6)
            if timeout_enforced_at is not None and deadline is not None
            else None
        )
        report.update(
            {
                "state": "completed" if harness_error is None else "harness_error",
                "completed_at": _utc_now(),
                "invocation_duration_secs": round(
                    time.monotonic() - invocation_started, 6
                ),
                "command_duration_secs": command_duration,
                "timeout_enforcement_delay_secs": enforcement_delay,
                "returncode": returncode,
                "timed_out": timed_out,
                "spawn_error": spawn_error,
                "harness_error": harness_error,
                "stdout": stdout,
                "stderr": stderr,
                "target_after": target_after,
                "observed_process_tree": list(observed.values()),
                "process_tree_at_timeout": timeout_snapshot,
                "local_new_relevant_processes": new_relevant_local,
                "external_concurrent_relevant_processes": new_relevant_external,
                "unverified_new_relevant_processes": new_relevant_unverified,
                "cleanup": {
                    "actions": cleanup_actions,
                    "group_absent": group_absent,
                    "baseline_process_snapshot_verified": baseline_verified,
                    "final_process_snapshot_verified": snapshot_verified,
                    "ownership_token_scan_verified": token_attribution_verified,
                    "ownership_token_scan_error": token_scan_error,
                    "orphans": orphan_rows,
                    "complete": cleanup_complete,
                    "result": cleanup_result,
                },
            }
        )
        report["diagnosis"] = _diagnose(
            timed_out=timed_out,
            returncode=returncode,
            observed=observed.values(),
            preexisting_relevant=preexisting_relevant,
            stdout_tail=stdout["tail"],
            stderr_tail=stderr["tail"],
            target_after=target_after,
            target_dir=target_dir,
            orphan_rows=orphan_rows,
        )
        report["passed"] = (
            spawn_error is None
            and harness_error is None
            and not timed_out
            and returncode == 0
            and cleanup_complete
        )
        _atomic_write_json(report_path, report)
        return report


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--cwd", type=Path, default=Path.cwd())
    parser.add_argument("--timeout-secs", type=float, default=600)
    parser.add_argument("--grace-secs", type=float, default=3)
    parser.add_argument("--tail-bytes", type=int, default=DEFAULT_TAIL_BYTES)
    parser.add_argument("--target-dir", type=Path, default=Path("target"))
    parser.add_argument("--report", type=Path)
    parser.add_argument("command", nargs=argparse.REMAINDER)
    return parser


class LivenessTests(unittest.TestCase):
    def run_harness(self, command: list[str], **overrides: Any) -> dict[str, Any]:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            relevance_context = (
                mock.patch(__name__ + "._is_relevant_process", return_value=False)
                if overrides.get("suppress_shared_host_noise", True)
                else nullcontext()
            )
            with relevance_context:
                return run_bounded(
                    command=command,
                    cwd=root,
                    timeout_secs=overrides.get("timeout_secs", 30),
                    grace_secs=overrides.get("grace_secs", 0.2),
                    report_path=root / "report.json",
                    target_dir=root / "target",
                    tail_bytes=overrides.get("tail_bytes", 1024),
                    poll_interval_secs=0.02,
                )

    def test_success_retains_output_and_report(self) -> None:
        report = self.run_harness(
            [sys.executable, "-c", "print('bounded-success')"]
        )
        self.assertTrue(report["passed"])
        self.assertEqual(report["returncode"], 0)
        self.assertIn("bounded-success", report["stdout"]["tail"])
        self.assertEqual(report["cleanup"]["result"], "not_needed")

    @unittest.skipUnless(os.name == "posix", "process-group cleanup requires POSIX")
    def test_timeout_escalates_and_leaves_no_process_group_orphans(self) -> None:
        script = """
import signal, subprocess, sys, time
signal.signal(signal.SIGTERM, signal.SIG_IGN)
subprocess.Popen([sys.executable, '-c', 'import signal,time; signal.signal(signal.SIGTERM, signal.SIG_IGN); time.sleep(60)'])
print('ready', flush=True)
time.sleep(60)
"""
        report = self.run_harness(
            [sys.executable, "-c", script], timeout_secs=1.5, grace_secs=0.1
        )
        self.assertTrue(report["timed_out"])
        self.assertFalse(report["passed"])
        self.assertEqual(report["cleanup"]["result"], "clean_after_sigkill")
        self.assertEqual(report["cleanup"]["orphans"], [])
        self.assertTrue(any("SIGKILL" in action for action in report["cleanup"]["actions"]))

    def test_output_is_tail_bounded(self) -> None:
        report = self.run_harness(
            [sys.executable, "-c", "print('x' * 4096)"], tail_bytes=128
        )
        self.assertTrue(report["stdout"]["truncated"])
        self.assertLessEqual(report["stdout"]["tail_bytes"], 128)
        self.assertGreater(report["stdout"]["bytes"], 128)

    def test_interpreter_hosted_verification_processes_are_relevant(self) -> None:
        python_row = {
            "pid": 10,
            "ppid": 1,
            "pgid": 10,
            "state": "S",
            "command": "/Frameworks/Python.app/Contents/MacOS/Python -c pass",
        }
        self.assertEqual(_command_basename(python_row), "python")
        self.assertTrue(_is_relevant_process(python_row))
        python_exe = {**python_row, "command": r"C:\\Python311\\Python.EXE -c pass"}
        self.assertTrue(_is_relevant_process(python_exe))
        versioned_python = {**python_row, "command": "/usr/bin/python3.12 -c pass"}
        self.assertTrue(_is_relevant_process(versioned_python))
        node_provider = {
            **python_row,
            "command": "/opt/homebrew/bin/node /opt/tools/opencode/dist/cli.js",
        }
        self.assertTrue(_is_relevant_process(node_provider))
        claude_package = {
            **python_row,
            "command": "/opt/homebrew/bin/node /opt/npm/@anthropic-ai/claude-code/cli.js",
        }
        self.assertTrue(_is_relevant_process(claude_package))
        pi_package = {
            **python_row,
            "command": "/opt/homebrew/bin/node /opt/npm/pi-coding-agent/dist/cli.js",
        }
        self.assertTrue(_is_relevant_process(pi_package))
        gemini_package = {
            **python_row,
            "command": "/opt/homebrew/bin/node /opt/npm/@google/gemini-cli/dist/index.js",
        }
        self.assertTrue(_is_relevant_process(gemini_package))
        custom_test = {
            **python_row,
            "command": "/tmp/custom-cargo-output/debug/deps/a2_test-1234 --nocapture",
        }
        self.assertTrue(
            _is_relevant_process(custom_test, Path("/tmp/custom-cargo-output"))
        )

    def test_diagnosis_categories_are_distinct(self) -> None:
        cargo = {"pid": 1, "ppid": 0, "pgid": 1, "state": "S", "command": "cargo test"}
        rustc = {"pid": 2, "ppid": 1, "pgid": 1, "state": "R", "command": "rustc --crate-name x"}
        test = {"pid": 3, "ppid": 1, "pgid": 1, "state": "S", "command": "/tmp/custom-target/debug/deps/example-test"}
        common = {
            "timed_out": True,
            "returncode": None,
            "stdout_tail": "",
            "stderr_tail": "",
            "target_after": {"probe_duration_secs": 0.01},
            "target_dir": Path("/tmp/custom-target"),
            "orphan_rows": [],
        }
        lock_common = {**common, "stderr_tail": "Blocking waiting for file lock"}
        self.assertEqual(
            _diagnose(
                observed=[cargo], preexisting_relevant=[cargo], **lock_common
            )["category"],
            "suspected_build_lock_contention",
        )
        self.assertEqual(
            _diagnose(observed=[cargo, rustc], preexisting_relevant=[], **common)["category"],
            "suspected_filesystem_or_target_pathology",
        )
        self.assertEqual(
            _diagnose(observed=[cargo, test], preexisting_relevant=[], **common)["category"],
            "suspected_test_deadlock",
        )
        self.assertEqual(
            _diagnose(observed=[cargo], preexisting_relevant=[], **common)["category"],
            "suspected_harness_or_command_deadlock",
        )

    @unittest.skipUnless(os.name == "posix", "process-group cleanup requires POSIX")
    def test_successful_parent_cannot_leave_same_group_child(self) -> None:
        script = """
import subprocess, sys
child = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(60)'])
print(child.pid, flush=True)
"""
        started = time.monotonic()
        report = self.run_harness(
            [sys.executable, "-c", script], timeout_secs=30, grace_secs=0.2
        )
        duration = time.monotonic() - started
        child_pid = int(report["stdout"]["tail"].strip())
        self.assertEqual(report["returncode"], 0)
        self.assertTrue(report["passed"])
        self.assertTrue(report["cleanup"]["complete"])
        self.assertTrue(report["cleanup"]["group_absent"])
        self.assertIn("descendants_survived_parent_exit", report["cleanup"]["actions"])
        self.assertFalse(_process_alive(child_pid))
        self.assertLess(duration, 35)

    @unittest.skipUnless(os.name == "posix", "escaped-child cleanup requires POSIX")
    def test_successful_parent_cannot_hide_escaped_session_child(self) -> None:
        script = """
import subprocess, sys
child = subprocess.Popen(
    [sys.executable, '-c', 'import signal,time; signal.signal(signal.SIGTERM, signal.SIG_IGN); time.sleep(60)'],
    start_new_session=True,
)
print(child.pid, flush=True)
"""
        report = self.run_harness(
            [sys.executable, "-c", script], timeout_secs=30, grace_secs=0.1
        )
        child_pid = int(report["stdout"]["tail"].strip())
        self.assertEqual(report["returncode"], 0)
        self.assertTrue(report["passed"])
        self.assertTrue(report["cleanup"]["ownership_token_scan_verified"])
        self.assertIn(
            "ownership_token_descendants_survived_process_group",
            report["cleanup"]["actions"],
        )
        self.assertTrue(
            any("ownership_token" in action for action in report["cleanup"]["actions"])
        )
        self.assertFalse(_process_alive(child_pid))

    @unittest.skipUnless(os.name == "posix", "escaped-child detection requires POSIX")
    def test_environment_scrubbing_escape_fails_closed_as_unattributed_orphan(self) -> None:
        script = """
import subprocess, sys
child = subprocess.Popen(
    [sys.executable, '-c', 'import time; time.sleep(60)'],
    start_new_session=True,
    env={},
)
print(child.pid, flush=True)
"""
        report = self.run_harness(
            [sys.executable, "-c", script],
            timeout_secs=30,
            grace_secs=0.1,
            suppress_shared_host_noise=False,
        )
        child_pid = int(report["stdout"]["tail"].strip())
        try:
            self.assertEqual(report["returncode"], 0)
            self.assertFalse(report["passed"])
            self.assertFalse(report["cleanup"]["complete"])
            self.assertTrue(
                any(
                    row["pid"] == child_pid
                    for row in report["local_new_relevant_processes"]
                )
            )
            self.assertTrue(
                any(row["pid"] == child_pid for row in report["cleanup"]["orphans"])
            )
        finally:
            try:
                os.kill(child_pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            deadline = time.monotonic() + 3
            while time.monotonic() < deadline:
                rows, _ = _process_table()
                row = next((item for item in rows if item["pid"] == child_pid), None)
                if row is None or str(row["state"]).startswith("Z"):
                    break
                time.sleep(0.05)

    def test_failed_final_process_snapshot_fails_closed(self) -> None:
        with mock.patch(__name__ + "._process_table", return_value=([], "ps unavailable")):
            report = self.run_harness([sys.executable, "-c", "pass"])
        self.assertFalse(report["passed"])
        self.assertFalse(report["cleanup"]["complete"])
        self.assertEqual(report["cleanup"]["result"], "unverified")

    def test_non_finite_timeout_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with self.assertRaises(ValueError):
                run_bounded(
                    command=[sys.executable, "-c", "pass"],
                    cwd=root,
                    timeout_secs=float("nan"),
                    grace_secs=0,
                    report_path=root / "report.json",
                    target_dir=root / "target",
                )


def main(argv: list[str] | None = None) -> int:
    args = _parser().parse_args(argv)
    if args.self_test:
        suite = unittest.defaultTestLoader.loadTestsFromTestCase(LivenessTests)
        return 0 if unittest.TextTestRunner(verbosity=2).run(suite).wasSuccessful() else 1
    command = args.command
    if command and command[0] == "--":
        command = command[1:]
    if not command:
        print("error: command is required after --", file=sys.stderr)
        return 2
    if args.report is None:
        print("error: --report is required so diagnostics cannot disappear", file=sys.stderr)
        return 2
    report = run_bounded(
        command=command,
        cwd=args.cwd,
        timeout_secs=args.timeout_secs,
        grace_secs=args.grace_secs,
        report_path=args.report,
        target_dir=args.target_dir,
        tail_bytes=args.tail_bytes,
    )
    print(json.dumps(report, indent=2, sort_keys=True))
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
