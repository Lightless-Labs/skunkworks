#!/usr/bin/env python3
"""Audit child-agent network-boundary prerequisites for benchmark integrity.

This check is not benchmark evidence and does not prove public internet/GitHub
reachability. It makes the current child-agent launch boundary reproducible:
where Pi subagent/Foundry child processes are spawned, whether the example Pi
sandbox hook exists, and whether the sandbox runtime package is available on this
host. Use --require-sandbox-runtime when a future benchmark path must fail closed
unless a sandbox runtime is installed.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path
from typing import Any

PI_PACKAGE = Path("/opt/homebrew/lib/node_modules/@earendil-works/pi-coding-agent")
PI_SUBAGENT = PI_PACKAGE / "examples/extensions/subagent/index.ts"
PI_SANDBOX = PI_PACKAGE / "examples/extensions/sandbox/index.ts"
FOUNDRY_REPO = Path("/Users/thomas/.pi/agent/git/github.com/Lightless-Labs/foundry")
FOUNDRY_TEAM = FOUNDRY_REPO / "extensions/pi-foundry-team/index.ts"
SANDBOX_RUNTIME_PACKAGE = "@anthropic-ai/sandbox-runtime"


def run(command: list[str], cwd: Path | None = None) -> dict[str, Any]:
    process = subprocess.run(command, cwd=cwd, text=True, capture_output=True, timeout=30)
    return {
        "command": command,
        "cwd": str(cwd) if cwd else None,
        "returncode": process.returncode,
        "stdout": process.stdout.strip(),
        "stderr": process.stderr.strip(),
    }


def read_json(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text(encoding="utf-8"))


def find_line(path: Path, needle: str) -> dict[str, Any]:
    if not path.exists():
        return {"path": str(path), "needle": needle, "found": False, "line": None, "text": None}
    for index, line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        if needle in line:
            return {
                "path": str(path),
                "needle": needle,
                "found": True,
                "line": index,
                "text": line.strip(),
            }
    return {"path": str(path), "needle": needle, "found": False, "line": None, "text": None}


def npm_global_package_path(package: str) -> dict[str, Any]:
    root = run(["npm", "root", "-g"])
    if root["returncode"] != 0 or not root["stdout"]:
        return {"available": False, "npm_root": root, "path": None, "package_json": None}
    package_path = Path(root["stdout"]) / package
    package_json = package_path / "package.json"
    if package_json.exists():
        version = read_json(package_json).get("version")
        return {
            "available": True,
            "npm_root": root["stdout"],
            "path": str(package_path),
            "package_json": str(package_json),
            "version": version,
        }
    return {
        "available": False,
        "npm_root": root["stdout"],
        "path": str(package_path),
        "package_json": str(package_json),
    }


def audit() -> dict[str, Any]:
    pi_package_json = PI_PACKAGE / "package.json"
    pi_version = read_json(pi_package_json).get("version") if pi_package_json.exists() else None
    foundry_head = run(["git", "rev-parse", "HEAD"], cwd=FOUNDRY_REPO) if FOUNDRY_REPO.exists() else None
    foundry_status = run(["git", "status", "--short", "--branch"], cwd=FOUNDRY_REPO) if FOUNDRY_REPO.exists() else None
    sandbox_runtime = npm_global_package_path(SANDBOX_RUNTIME_PACKAGE)

    subagent_checks = [
        find_line(PI_SUBAGENT, "async function runSingleAgent"),
        find_line(PI_SUBAGENT, "function getPiInvocation"),
        find_line(PI_SUBAGENT, "spawn(invocation.command, invocation.args"),
    ]
    foundry_checks = [
        find_line(FOUNDRY_TEAM, "async function runDispatch"),
        find_line(FOUNDRY_TEAM, "function piInvocation"),
        find_line(FOUNDRY_TEAM, "spawn(invocation.command, invocation.args"),
    ]
    sandbox_checks = [
        find_line(PI_SANDBOX, "SandboxManager.wrapWithSandbox"),
        find_line(PI_SANDBOX, "SandboxManager.initialize"),
        find_line(PI_SANDBOX, "network:"),
    ]

    launch_boundaries_found = all(item["found"] for item in subagent_checks + foundry_checks)
    sandbox_example_found = all(item["found"] for item in sandbox_checks)

    return {
        "schema": "a2.agent-network-boundary-audit.v1",
        "not_benchmark_evidence": True,
        "complete": launch_boundaries_found and sandbox_example_found,
        "pi_package": {
            "path": str(PI_PACKAGE),
            "version": pi_version,
            "subagent_extension": str(PI_SUBAGENT),
            "sandbox_extension": str(PI_SANDBOX),
        },
        "foundry": {
            "repo": str(FOUNDRY_REPO),
            "head": foundry_head["stdout"] if foundry_head else None,
            "status": foundry_status["stdout"].splitlines() if foundry_status else None,
            "team_extension": str(FOUNDRY_TEAM),
        },
        "subagent_launch_boundary": subagent_checks,
        "foundry_team_launch_boundary": foundry_checks,
        "sandbox_example": sandbox_checks,
        "sandbox_runtime": sandbox_runtime,
        "conclusion": (
            "Child pi launch boundaries are identifiable, but sandbox runtime is not available globally; "
            "benchmark child-agent network isolation remains unenforced until a sandbox/provider allowlist is wired at these spawn points."
            if not sandbox_runtime["available"]
            else "Child pi launch boundaries and a global sandbox runtime are present; next step is to wire and verify runtime enforcement at the spawn points."
        ),
    }


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--json", action="store_true", help="print full audit JSON")
    parser.add_argument("--self-test", action="store_true", help="run audit and print PASS/FAIL")
    parser.add_argument(
        "--require-sandbox-runtime",
        action="store_true",
        help="fail unless @anthropic-ai/sandbox-runtime is available globally",
    )
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    result = audit()
    if args.require_sandbox_runtime and not result["sandbox_runtime"]["available"]:
        if args.json:
            print(json.dumps(result, indent=2, sort_keys=True))
        else:
            print("FAIL sandbox runtime availability: @anthropic-ai/sandbox-runtime not installed globally")
        return 1
    if args.json:
        print(json.dumps(result, indent=2, sort_keys=True))
    elif args.self_test:
        if result["complete"]:
            runtime = "available" if result["sandbox_runtime"]["available"] else "missing"
            print(f"PASS agent network boundary audit: launch points found; sandbox runtime {runtime}")
        else:
            print("FAIL agent network boundary audit")
            print(json.dumps(result, indent=2, sort_keys=True))
    else:
        print(json.dumps(result, indent=2, sort_keys=True))
    return 0 if result["complete"] else 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
