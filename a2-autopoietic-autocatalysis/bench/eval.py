#!/usr/bin/env python3
"""Lightweight benchmark evaluator.

Reads a task JSON object from stdin, runs its setup script and test command in
the provided repo path, then emits a single JSON result object to stdout.
"""

from __future__ import annotations

import json
import os
import signal
import subprocess
import sys
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

TIMEOUT_SECS = 60
MAX_MEMORY_BYTES = 4 * 1024 * 1024 * 1024
MAX_FILE_BYTES = 64 * 1024 * 1024
MAX_OPEN_FILES = 256


@dataclass
class CommandResult:
    command: str
    returncode: int
    stdout: str
    stderr: str
    timed_out: bool = False


def _limit_resources() -> None:
    try:
        import resource
    except ImportError:
        return

    try:
        os.setsid()
    except OSError:
        pass

    limits = [
        (getattr(resource, "RLIMIT_CPU", None), (TIMEOUT_SECS, TIMEOUT_SECS + 1)),
        (getattr(resource, "RLIMIT_AS", None), (MAX_MEMORY_BYTES, MAX_MEMORY_BYTES)),
        (getattr(resource, "RLIMIT_FSIZE", None), (MAX_FILE_BYTES, MAX_FILE_BYTES)),
        (getattr(resource, "RLIMIT_NOFILE", None), (MAX_OPEN_FILES, MAX_OPEN_FILES)),
        (getattr(resource, "RLIMIT_CORE", None), (0, 0)),
    ]

    for resource_type, value in limits:
        if resource_type is None:
            continue
        try:
            resource.setrlimit(resource_type, value)
        except (OSError, ValueError):
            continue


def run_command(command: str, cwd: Path) -> CommandResult:
    process = subprocess.Popen(
        command,
        cwd=str(cwd),
        shell=True,
        executable="/bin/bash",
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        preexec_fn=_limit_resources if os.name != "nt" else None,
        env={
            **os.environ,
            "PYTHONUNBUFFERED": "1",
        },
    )

    try:
        stdout, stderr = process.communicate(timeout=TIMEOUT_SECS)
        return CommandResult(
            command=command,
            returncode=process.returncode,
            stdout=stdout,
            stderr=stderr,
        )
    except subprocess.TimeoutExpired:
        if os.name != "nt":
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except OSError:
                process.kill()
        else:
            process.kill()

        stdout, stderr = process.communicate()
        stderr = f"{stderr}\nCommand timed out after {TIMEOUT_SECS}s.".strip()
        return CommandResult(
            command=command,
            returncode=124,
            stdout=stdout,
            stderr=stderr,
            timed_out=True,
        )


def format_output(label: str, result: CommandResult) -> tuple[str, str]:
    stdout = f"$ {label}: {result.command}\n{result.stdout}".rstrip()
    stderr = f"$ {label}: {result.command}\n{result.stderr}".rstrip()
    return stdout, stderr


def read_task() -> dict[str, Any]:
    raw = sys.stdin.read()
    if not raw.strip():
        raise ValueError("expected a task JSON object on stdin")

    payload = json.loads(raw)
    if not isinstance(payload, dict):
        raise ValueError("task payload must be a JSON object")

    for field in ("problem_statement", "setup_script", "test_command", "repo_path"):
        if not payload.get(field):
            raise ValueError(f"task payload is missing required field: {field}")

    return payload


def main() -> int:
    try:
        task = read_task()
    except Exception as exc:  # noqa: BLE001
        print(
            json.dumps(
                {
                    "resolved": False,
                    "stdout": "",
                    "stderr": str(exc),
                    "returncode": 1,
                }
            )
        )
        return 1

    repo_path = Path(task["repo_path"]).expanduser()
    if not repo_path.is_absolute():
        repo_path = Path.cwd() / repo_path
    repo_path.mkdir(parents=True, exist_ok=True)

    setup_result = run_command(task["setup_script"], repo_path)
    setup_stdout, setup_stderr = format_output("setup", setup_result)

    passthrough = {
        key: value
        for key, value in task.items()
        if key not in {"problem_statement", "setup_script", "test_command", "repo_path"}
    }

    if setup_result.returncode != 0:
        output = {
            **passthrough,
            "resolved": False,
            "stdout": setup_stdout,
            "stderr": setup_stderr,
            "returncode": setup_result.returncode,
            "evaluated_at": datetime.now(timezone.utc).isoformat(),
        }
        print(json.dumps(output))
        return 0

    test_result = run_command(task["test_command"], repo_path)
    test_stdout, test_stderr = format_output("test", test_result)

    output = {
        **passthrough,
        "resolved": test_result.returncode == 0,
        "stdout": "\n\n".join(part for part in (setup_stdout, test_stdout) if part),
        "stderr": "\n\n".join(part for part in (setup_stderr, test_stderr) if part),
        "returncode": test_result.returncode,
        "evaluated_at": datetime.now(timezone.utc).isoformat(),
    }
    print(json.dumps(output))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
