#!/usr/bin/env python3
"""Smoke-test A² network-policy primitives and fail-closed launch behavior.

These checks are intentionally not benchmark evidence. The default smoke proves a
spawned child process can be run under an OS-level no-network sandbox on hosts
that provide macOS ``sandbox-exec``. ``--a2ctl-run-smoke`` separately exercises
the real ``a2ctl run --network-policy isolated`` path and expects the current
fail-closed launch gate to refuse provider launch with a nonzero exit. A real
Senior SWE Bench run still needs a sandbox/provider allowlist wired into the
coding-agent/provider launch path.
"""

from __future__ import annotations

import argparse
import json
import shutil
import socket
import subprocess
import sys
import tempfile
import threading
from pathlib import Path
from typing import Any

DENY_NETWORK_PROFILE = """(version 1)
(allow default)
(deny network*)
"""

A2CTL_RUN_SMOKE_TASK = "A2 network fail closed smoke task\n"


def sandbox_exec() -> str:
    binary = shutil.which("sandbox-exec")
    if not binary:
        raise RuntimeError("sandbox-exec was not found on PATH; cannot prove OS-level egress denial")
    return binary


def start_local_tcp_probe() -> tuple[socket.socket, int, threading.Thread, dict[str, bool]]:
    listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    listener.bind(("127.0.0.1", 0))
    listener.listen(1)
    listener.settimeout(2.0)
    port = listener.getsockname()[1]
    observed = {"accepted": False}

    def accept_once() -> None:
        try:
            conn, _addr = listener.accept()
        except OSError:
            return
        observed["accepted"] = True
        conn.close()

    thread = threading.Thread(target=accept_once, daemon=True)
    thread.start()
    return listener, port, thread, observed


def run_smoke() -> dict[str, Any]:
    sandbox = sandbox_exec()
    with tempfile.TemporaryDirectory(prefix="a2-network-policy-smoke-") as tmp:
        tmp_path = Path(tmp)
        profile = tmp_path / "deny-network.sb"
        profile.write_text(DENY_NETWORK_PROFILE, encoding="utf-8")

        local_file = tmp_path / "local-write.txt"
        local_command = [
            sandbox,
            "-f",
            str(profile),
            sys.executable,
            "-c",
            "from pathlib import Path; Path('local-write.txt').write_text('ok', encoding='utf-8')",
        ]
        local_probe = subprocess.run(
            local_command,
            cwd=tmp_path,
            text=True,
            capture_output=True,
            timeout=10,
        )

        listener, port, thread, observed = start_local_tcp_probe()
        network_target = {"host": "127.0.0.1", "port": port}
        network_command = [
            sandbox,
            "-f",
            str(profile),
            sys.executable,
            "-c",
            (
                "import socket; "
                f"socket.create_connection(('127.0.0.1', {port}), timeout=1); "
                "print('unexpectedly connected')"
            ),
        ]
        try:
            network_probe = subprocess.run(
                network_command,
                cwd=tmp_path,
                text=True,
                capture_output=True,
                timeout=10,
            )
        finally:
            listener.close()
            thread.join(timeout=0.5)

        local_ok = local_probe.returncode == 0 and local_file.read_text(encoding="utf-8") == "ok"
        network_blocked = network_probe.returncode != 0 and not observed["accepted"]

        return {
            "complete": local_ok and network_blocked,
            "sandbox_binary": sandbox,
            "sandbox_profile": DENY_NETWORK_PROFILE.strip().splitlines(),
            "local_probe": {
                "description": "sandboxed child can still run local filesystem work",
                "command": local_command,
                "cwd": str(tmp_path),
                "returncode": local_probe.returncode,
                "stdout": local_probe.stdout,
                "stderr": local_probe.stderr,
                "passed": local_ok,
            },
            "network_probe": {
                "description": "sandboxed child cannot open a TCP connection to a local listener",
                "command": network_command,
                "cwd": str(tmp_path),
                "target": network_target,
                "returncode": network_probe.returncode,
                "stdout": network_probe.stdout,
                "stderr": network_probe.stderr,
                "listener_accepted_connection": observed["accepted"],
                "passed": network_blocked,
            },
        }


def repo_root() -> Path:
    return Path(__file__).resolve().parent.parent


def run_a2ctl_launch_gate_smoke(provider: str) -> dict[str, Any]:
    binary_name = provider.split("/", 1)[0]
    provider_binary = shutil.which(binary_name)
    command = [
        "cargo",
        "run",
        "-q",
        "-p",
        "a2ctl",
        "--",
        "run",
        "--provider",
        provider,
        "--network-policy",
        "isolated",
        "--max-tokens",
        "10",
        "--timeout",
        "5",
    ]
    if provider_binary is None:
        return {
            "complete": False,
            "description": "a2ctl run restricted-policy launch-gate smoke",
            "command": command,
            "provider_binary": None,
            "returncode": None,
            "stdout": "",
            "stderr": f"provider binary `{binary_name}` not found on PATH",
            "passed": False,
        }

    process = subprocess.run(
        command,
        cwd=repo_root(),
        input=A2CTL_RUN_SMOKE_TASK,
        text=True,
        capture_output=True,
        timeout=180,
    )
    expected_catalyst_message = f"network_policy=Isolated prevents launching provider `{binary_name}`"
    expected_cli_message = "restricted network policy blocked provider launch"
    combined_output = process.stdout + "\n" + process.stderr
    passed = (
        process.returncode != 0
        and expected_catalyst_message in combined_output
        and expected_cli_message in process.stderr
        and "no candidate patch produced" in process.stderr
    )
    return {
        "complete": passed,
        "description": "a2ctl run restricted-policy launch-gate smoke",
        "command": command,
        "provider_binary": provider_binary,
        "returncode": process.returncode,
        "stdout": process.stdout,
        "stderr": process.stderr,
        "expected_catalyst_message": expected_catalyst_message,
        "expected_cli_stderr_substring": expected_cli_message,
        "passed": passed,
    }


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true", help="run the selected smoke and print PASS/FAIL")
    parser.add_argument("--a2ctl-run-smoke", action="store_true", help="exercise a2ctl run --network-policy isolated and require fail-closed nonzero exit")
    parser.add_argument("--provider", default="opencode", help="provider/model for --a2ctl-run-smoke (default: opencode)")
    parser.add_argument("--json", action="store_true", help="print the full smoke result as JSON")
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    if args.a2ctl_run_smoke:
        result = run_a2ctl_launch_gate_smoke(args.provider)
        pass_message = "PASS a2ctl run launch-gate smoke: restricted policy blocked provider launch with nonzero exit"
        fail_message = "FAIL a2ctl run launch-gate smoke"
    else:
        result = run_smoke()
        pass_message = "PASS network policy smoke: sandboxed child process had network egress denied"
        fail_message = "FAIL network policy smoke"

    if args.json:
        print(json.dumps(result, indent=2, sort_keys=True))
    elif args.self_test:
        if result["complete"]:
            print(pass_message)
        else:
            print(fail_message)
            print(json.dumps(result, indent=2, sort_keys=True))
    else:
        print(json.dumps(result, indent=2, sort_keys=True))

    return 0 if result["complete"] else 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
