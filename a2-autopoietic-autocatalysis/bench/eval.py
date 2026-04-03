#!/usr/bin/env python3
"""Lightweight evaluation script for A² benchmark tasks.

Reads a task JSON from stdin, runs setup + test, returns result JSON.

Usage:
    echo '{"setup_script": "echo hello", "test_command": "cargo test -p a2_core", "timeout": 60}' | python3 bench/eval.py
"""

import json
import subprocess
import sys
import os

def run_with_timeout(command, cwd=None, timeout=60):
    """Run a command with timeout, return (success, stdout, stderr, returncode)."""
    try:
        result = subprocess.run(
            command,
            shell=True,
            cwd=cwd,
            capture_output=True,
            text=True,
            timeout=timeout,
        )
        return result.returncode == 0, result.stdout, result.stderr, result.returncode
    except subprocess.TimeoutExpired:
        return False, "", f"timeout after {timeout}s", 124
    except Exception as e:
        return False, "", str(e), 1

def evaluate(task):
    cwd = task.get("cwd", os.getcwd())
    timeout = task.get("timeout", 60)

    # Run setup if provided
    setup = task.get("setup_script", "")
    if setup:
        ok, stdout, stderr, rc = run_with_timeout(setup, cwd=cwd, timeout=timeout)
        if not ok:
            return {"resolved": False, "phase": "setup", "stdout": stdout, "stderr": stderr, "returncode": rc}

    # Run test command
    test_cmd = task.get("test_command", "")
    if not test_cmd:
        return {"resolved": False, "phase": "test", "error": "no test_command provided"}

    ok, stdout, stderr, rc = run_with_timeout(test_cmd, cwd=cwd, timeout=timeout)
    return {"resolved": ok, "phase": "test", "stdout": stdout[-2000:], "stderr": stderr[-2000:], "returncode": rc}

if __name__ == "__main__":
    task = json.loads(sys.stdin.read())
    result = evaluate(task)
    print(json.dumps(result))
