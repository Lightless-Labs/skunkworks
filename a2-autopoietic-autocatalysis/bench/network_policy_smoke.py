#!/usr/bin/env python3
"""Smoke-test the host primitive A² can use for benchmark egress denial.

This is intentionally not benchmark evidence. It proves a spawned child process can
be run under an OS-level no-network sandbox on hosts that provide macOS
``sandbox-exec``. A real Senior SWE Bench run still needs this kind of primitive
wired into the coding-agent/provider launch path with any required provider
allowlist.
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
        local_probe = subprocess.run(
            [
                sandbox,
                "-f",
                str(profile),
                sys.executable,
                "-c",
                "from pathlib import Path; Path('local-write.txt').write_text('ok', encoding='utf-8')",
            ],
            cwd=tmp_path,
            text=True,
            capture_output=True,
            timeout=10,
        )

        listener, port, thread, observed = start_local_tcp_probe()
        try:
            network_probe = subprocess.run(
                [
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
                ],
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
                "returncode": local_probe.returncode,
                "stdout": local_probe.stdout,
                "stderr": local_probe.stderr,
                "passed": local_ok,
            },
            "network_probe": {
                "description": "sandboxed child cannot open a TCP connection to a local listener",
                "returncode": network_probe.returncode,
                "stdout": network_probe.stdout,
                "stderr": network_probe.stderr,
                "listener_accepted_connection": observed["accepted"],
                "passed": network_blocked,
            },
        }


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true", help="run the smoke and print PASS/FAIL")
    parser.add_argument("--json", action="store_true", help="print the full smoke result as JSON")
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    result = run_smoke()
    if args.json:
        print(json.dumps(result, indent=2, sort_keys=True))
    elif args.self_test:
        if result["complete"]:
            print("PASS network policy smoke: sandboxed child process had network egress denied")
        else:
            print("FAIL network policy smoke")
            print(json.dumps(result, indent=2, sort_keys=True))
    else:
        print(json.dumps(result, indent=2, sort_keys=True))

    return 0 if result["complete"] else 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
