#!/usr/bin/env python3
"""Smoke-test A² network-policy primitives and fail-closed launch behavior.

These checks are intentionally not benchmark evidence. The default smoke proves a
spawned child process can be run under an OS-level no-network sandbox on hosts
that provide macOS ``sandbox-exec``. ``--a2ctl-run-smoke`` separately checks
the real ``a2ctl run`` restricted-policy boundary without silently starting a
live provider: on hosts where A² can route the selected provider through
``/usr/bin/sandbox-exec`` it stops before launch;
on hosts without that runtime it exercises the fail-closed launch refusal.
``--allowlist-smoke`` exercises a synthetic localhost-only sandbox allowlist
primitive, not real model-provider API allowlisting. A real Senior SWE Bench run
still needs a sandbox/provider allowlist wired into every coding-agent/provider
launch path and recorded in row-level evidence.
"""

from __future__ import annotations

import argparse
import hashlib
import ipaddress
import json
import os
import shutil
import socket
import subprocess
import sys
import tempfile
import threading
import unittest
from pathlib import Path
from typing import Any

DENY_NETWORK_PROFILE = """(version 1)
(allow default)
(deny network*)
"""

A2CTL_RUN_SMOKE_TASK = "A2 network fail closed smoke task\n"


def local_allowlist_profile(allowed_port: int) -> str:
    return (
        "(version 1)\n"
        "(allow default)\n"
        "(deny network*)\n"
        f'(allow network-outbound (remote tcp "localhost:{allowed_port}"))\n'
    )


def provider_allowlist_profile(allowed_hosts: list[str], *, port: int = 443) -> str:
    """Return a deny-by-default sandbox profile with exact provider host allows.

    This is profile-shape support for future provider allowlist enforcement. It
    does not run a live provider call and does not prove benchmark isolation by
    itself.
    """
    if not allowed_hosts:
        raise ValueError("provider allowlist profile requires at least one allowed host")
    if type(port) is not int or port <= 0 or port > 65535:
        raise ValueError("provider allowlist profile requires a valid TCP port")

    lines = ["(version 1)", "(allow default)", "(deny network*)"]
    for host in allowed_hosts:
        if not isinstance(host, str):
            raise ValueError("provider allowlist hosts must be strings")
        normalized = host.strip().lower().rstrip(".")
        if not normalized or normalized in {"*", "0.0.0.0", "::"}:
            raise ValueError("provider allowlist hosts must be exact host names")
        if (
            normalized == "github.com"
            or normalized.endswith(".github.com")
            or normalized == "githubusercontent.com"
            or normalized.endswith(".githubusercontent.com")
            or normalized == "github.io"
            or normalized.endswith(".github.io")
        ):
            raise ValueError("provider allowlist profile cannot allow public solution hosts")
        if any(character.isspace() for character in normalized) or "/" in normalized or ":" in normalized:
            raise ValueError("provider allowlist hosts must not contain URL syntax")
        try:
            ipaddress.ip_address(normalized)
            raise ValueError("provider allowlist hosts must be DNS names, not IP literals")
        except ValueError as error:
            if "must be DNS names" in str(error):
                raise
        labels = normalized.split(".")
        if len(labels) < 2:
            raise ValueError("provider allowlist hosts must be fully qualified provider host names")
        if normalized == "localhost" or normalized.endswith(".localhost"):
            raise ValueError("provider allowlist hosts must not be local hosts")
        if normalized in {"example.com", "example.net", "example.org"} or normalized.endswith(
            (".example", ".example.com", ".example.net", ".example.org", ".invalid", ".test")
        ):
            raise ValueError("provider allowlist hosts must not be synthetic/example hosts")
        if any(
            not label
            or label.startswith("-")
            or label.endswith("-")
            or not all(character.isascii() and (character.isalnum() or character == "-") for character in label)
            for label in labels
        ):
            raise ValueError("provider allowlist hosts must be valid DNS names")
        lines.append(f'(allow network-outbound (remote tcp "{normalized}:{port}"))')
    return "\n".join(lines) + "\n"


def sha256_text(value: str) -> str:
    return hashlib.sha256(value.encode("utf-8")).hexdigest()


def sandbox_profile_metadata(profile_path: Path) -> dict[str, Any]:
    profile_text = profile_path.read_text(encoding="utf-8")
    return {
        "engine": "sandbox-exec",
        "profile_path": str(profile_path),
        "profile_path_is_absolute": profile_path.is_absolute(),
        "profile_path_lifetime": "ephemeral_tempfile_removed_after_smoke",
        "durable_audit_fields": ["profile_sha256", "profile_lines"],
        "profile_sha256": sha256_text(profile_text),
        "profile_lines": profile_text.splitlines(),
    }


def command_profile_arg(command: list[str]) -> str | None:
    for idx, arg in enumerate(command):
        if arg == "-f" and idx + 1 < len(command):
            return command[idx + 1]
    return None


def smoke_result_profile_audit_ok(
    result: dict[str, Any],
    *,
    expected_profile_text: str = DENY_NETWORK_PROFILE,
    probe_names: tuple[str, ...] = ("local_probe", "network_probe"),
) -> bool:
    profile = result.get("sandbox_profile")
    if not isinstance(profile, dict):
        return False
    profile_path = profile.get("profile_path")
    if not isinstance(profile_path, str) or not profile_path:
        return False
    if not profile.get("profile_path_is_absolute") or not Path(profile_path).is_absolute():
        return False
    if profile.get("profile_path_lifetime") != "ephemeral_tempfile_removed_after_smoke":
        return False
    if profile.get("durable_audit_fields") != ["profile_sha256", "profile_lines"]:
        return False
    if profile.get("profile_sha256") != sha256_text(expected_profile_text):
        return False
    if profile.get("profile_lines") != expected_profile_text.splitlines():
        return False
    for probe_name in probe_names:
        probe = result.get(probe_name)
        if not isinstance(probe, dict):
            return False
        command = probe.get("command")
        if not isinstance(command, list) or command_profile_arg(command) != profile_path:
            return False
    return True


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


def sandboxed_tcp_probe_command(sandbox: str, profile: Path, host: str, port: int) -> list[str]:
    return [
        sandbox,
        "-f",
        str(profile),
        sys.executable,
        "-c",
        (
            "import socket; "
            f"socket.create_connection(({host!r}, {port}), timeout=1); "
            "print('connected')"
        ),
    ]


def probe_failure_kind(probe: subprocess.CompletedProcess[str]) -> str:
    if probe.returncode == 0:
        return "connected"
    output = f"{probe.stdout}\n{probe.stderr}"
    if "Operation not permitted" in output or "PermissionError" in output:
        return "policy_denied"
    if "nodename nor servname" in output or "gaierror" in output:
        return "dns_unresolved_or_denied"
    if "timed out" in output or "TimeoutError" in output:
        return "timeout_or_unreachable"
    return "failed"


def sandboxed_tcp_probe(
    *,
    sandbox: str,
    profile: Path,
    cwd: Path,
    host: str,
    port: int,
) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        sandboxed_tcp_probe_command(sandbox, profile, host, port),
        cwd=cwd,
        text=True,
        capture_output=True,
        timeout=10,
    )


def run_smoke() -> dict[str, Any]:
    sandbox = sandbox_exec()
    with tempfile.TemporaryDirectory(prefix="a2-network-policy-smoke-") as tmp:
        tmp_path = Path(tmp)
        profile = tmp_path / "deny-network.sb"
        profile.write_text(DENY_NETWORK_PROFILE, encoding="utf-8")
        profile_metadata = sandbox_profile_metadata(profile)

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

        result = {
            "complete": local_ok and network_blocked,
            "sandbox_binary": sandbox,
            "sandbox_profile": profile_metadata,
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
        result["sandbox_profile_audit"] = {
            "description": "sandbox profile metadata matches the -f profile path used by both sandbox-exec probes",
            "passed": smoke_result_profile_audit_ok(result),
        }
        result["complete"] = result["complete"] and result["sandbox_profile_audit"]["passed"]
        return result


def run_allowlist_smoke() -> dict[str, Any]:
    sandbox = sandbox_exec()
    with tempfile.TemporaryDirectory(prefix="a2-network-policy-allowlist-smoke-") as tmp:
        tmp_path = Path(tmp)
        allowed_listener, allowed_port, allowed_thread, allowed_observed = start_local_tcp_probe()
        blocked_listener, blocked_port, blocked_thread, blocked_observed = start_local_tcp_probe()
        profile_text = local_allowlist_profile(allowed_port)
        profile = tmp_path / "allow-one-localhost-port.sb"
        profile.write_text(profile_text, encoding="utf-8")
        profile_metadata = sandbox_profile_metadata(profile)

        try:
            allowed_probe = sandboxed_tcp_probe(
                sandbox=sandbox,
                profile=profile,
                cwd=tmp_path,
                host="127.0.0.1",
                port=allowed_port,
            )
            blocked_probe = sandboxed_tcp_probe(
                sandbox=sandbox,
                profile=profile,
                cwd=tmp_path,
                host="127.0.0.1",
                port=blocked_port,
            )
            public_solution_probe = sandboxed_tcp_probe(
                sandbox=sandbox,
                profile=profile,
                cwd=tmp_path,
                host="github.com",
                port=443,
            )
        finally:
            allowed_listener.close()
            blocked_listener.close()
            allowed_thread.join(timeout=0.5)
            blocked_thread.join(timeout=0.5)

        allowed_ok = allowed_probe.returncode == 0 and allowed_observed["accepted"]
        blocked_failure_kind = probe_failure_kind(blocked_probe)
        blocked_ok = (
            blocked_probe.returncode != 0
            and blocked_failure_kind == "policy_denied"
            and not blocked_observed["accepted"]
        )
        public_solution_failure_kind = probe_failure_kind(public_solution_probe)
        public_solution_negative_control_ok = public_solution_failure_kind in {
            "policy_denied",
            "dns_unresolved_or_denied",
            "timeout_or_unreachable",
        }
        allowed_command = sandboxed_tcp_probe_command(sandbox, profile, "127.0.0.1", allowed_port)
        blocked_command = sandboxed_tcp_probe_command(sandbox, profile, "127.0.0.1", blocked_port)
        public_solution_command = sandboxed_tcp_probe_command(sandbox, profile, "github.com", 443)
        result = {
            "complete": allowed_ok and blocked_ok and public_solution_negative_control_ok,
            "description": "sandbox-exec synthetic localhost allowlist primitive smoke",
            "causal_note": "the non-allowlisted localhost listener is the policy-denial control because it is known reachable outside the allowlist; the public solution host probe is supporting negative evidence and records DNS/offline/policy failure details rather than proving causality by itself",
            "sandbox_binary": sandbox,
            "sandbox_profile": profile_metadata,
            "allowed_probe": {
                "description": "sandboxed child can reach the explicitly allowlisted synthetic localhost endpoint",
                "command": allowed_command,
                "cwd": str(tmp_path),
                "target": {"host": "127.0.0.1", "port": allowed_port},
                "returncode": allowed_probe.returncode,
                "stdout": allowed_probe.stdout,
                "stderr": allowed_probe.stderr,
                "listener_accepted_connection": allowed_observed["accepted"],
                "passed": allowed_ok,
            },
            "blocked_probe": {
                "description": "sandboxed child cannot reach a non-allowlisted localhost endpoint",
                "command": blocked_command,
                "cwd": str(tmp_path),
                "target": {"host": "127.0.0.1", "port": blocked_port},
                "returncode": blocked_probe.returncode,
                "stdout": blocked_probe.stdout,
                "stderr": blocked_probe.stderr,
                "failure_kind": blocked_failure_kind,
                "listener_accepted_connection": blocked_observed["accepted"],
                "passed": blocked_ok,
            },
            "public_solution_probe": {
                "description": "sandboxed child did not reach a public solution host outside the synthetic allowlist",
                "command": public_solution_command,
                "cwd": str(tmp_path),
                "target": {"host": "github.com", "port": 443},
                "returncode": public_solution_probe.returncode,
                "stdout": public_solution_probe.stdout,
                "stderr": public_solution_probe.stderr,
                "failure_kind": public_solution_failure_kind,
                "causal_note": "not used alone to prove policy denial because DNS/offline failures can look similar; pair with blocked_probe listener_accepted_connection=false",
                "passed": public_solution_negative_control_ok,
            },
        }
        result["sandbox_profile_audit"] = {
            "description": "sandbox profile metadata matches the -f profile path used by all allowlist probes",
            "passed": smoke_result_profile_audit_ok(
                result,
                expected_profile_text=profile_text,
                probe_names=("allowed_probe", "blocked_probe", "public_solution_probe"),
            ),
        }
        result["complete"] = result["complete"] and result["sandbox_profile_audit"]["passed"]
        return result


def repo_root() -> Path:
    return Path(__file__).resolve().parent.parent


def parse_restricted_network_policy_arg(network_policy: str) -> tuple[bool, str]:
    trimmed = network_policy.strip()
    normalized = trimmed.lower()
    if normalized == "isolated":
        return True, "Isolated"
    for prefix in ("allowlist:", "allow-list:"):
        if normalized.startswith(prefix):
            endpoints = [endpoint.strip() for endpoint in trimmed[len(prefix) :].split(",")]
            if any(endpoints):
                return True, "AllowList"
            return False, "invalid empty allowlist network policy"
    return False, "non-restricted network policy"


def is_restricted_network_policy_arg(network_policy: str) -> bool:
    return parse_restricted_network_policy_arg(network_policy)[0]


def expected_catalyst_network_policy_label(network_policy: str) -> str:
    restricted, label = parse_restricted_network_policy_arg(network_policy)
    return label if restricted else network_policy


def exact_sandbox_exec_available() -> bool:
    return sys.platform == "darwin" and os.path.isfile("/usr/bin/sandbox-exec") and os.access(
        "/usr/bin/sandbox-exec", os.X_OK
    )


def run_a2ctl_launch_gate_smoke(provider: str, network_policy: str) -> dict[str, Any]:
    binary_name = provider.split("/", 1)[0]
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
        network_policy,
        "--max-tokens",
        "10",
        "--timeout",
        "30",
    ]
    restricted, policy_label = parse_restricted_network_policy_arg(network_policy)
    if not restricted:
        return {
            "complete": False,
            "description": "a2ctl run restricted-policy boundary smoke",
            "command": command,
            "network_policy": network_policy,
            "provider_binary": None,
            "returncode": None,
            "stdout": "",
            "stderr": f"refusing to run restricted-policy boundary smoke with {policy_label} `{network_policy}`",
            "passed": False,
        }

    provider_binary = shutil.which(binary_name)
    if provider_binary is None:
        return {
            "complete": False,
            "description": "a2ctl run restricted-policy boundary smoke",
            "command": command,
            "network_policy": network_policy,
            "provider_binary": None,
            "returncode": None,
            "stdout": "",
            "stderr": f"provider binary `{binary_name}` not found on PATH",
            "passed": False,
        }

    sandbox_exec_available = exact_sandbox_exec_available()
    if sandbox_exec_available:
        return {
            "complete": True,
            "description": "a2ctl run restricted-policy boundary smoke",
            "command": command,
            "network_policy": network_policy,
            "provider_binary": provider_binary,
            "sandbox_exec_program": "/usr/bin/sandbox-exec",
            "sandbox_exec_available": True,
            "provider_launch_executed": False,
            "returncode": None,
            "stdout": "",
            "stderr": (
                "restricted provider launch not executed: /usr/bin/sandbox-exec is available, "
                "so this default smoke stops before starting a live provider; use boundary audit "
                "commands for source-level sandbox wrapping proof"
            ),
            "passed": True,
        }

    process = subprocess.run(
        command,
        cwd=repo_root(),
        input=A2CTL_RUN_SMOKE_TASK,
        text=True,
        capture_output=True,
        timeout=180,
    )
    expected_policy_label = expected_catalyst_network_policy_label(network_policy)
    expected_catalyst_message = f"network_policy={expected_policy_label} prevents launching provider `{binary_name}`"
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
        "description": "a2ctl run restricted-policy boundary smoke",
        "command": command,
        "network_policy": network_policy,
        "provider_binary": provider_binary,
        "sandbox_exec_program": "/usr/bin/sandbox-exec",
        "sandbox_exec_available": sandbox_exec_available,
        "provider_launch_executed": True,
        "returncode": process.returncode,
        "stdout": process.stdout,
        "stderr": process.stderr,
        "expected_policy_label": expected_policy_label,
        "expected_catalyst_message": expected_catalyst_message,
        "expected_cli_stderr_substring": expected_cli_message,
        "passed": passed,
    }


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true", help="run the selected smoke and print PASS/FAIL")
    parser.add_argument(
        "--a2ctl-run-smoke",
        action="store_true",
        help="check a2ctl run restricted-policy boundary; by default avoids live provider launch when /usr/bin/sandbox-exec is available",
    )
    parser.add_argument("--allowlist-smoke", action="store_true", help="exercise sandbox-exec synthetic localhost allowlist primitive with positive and negative TCP probes")
    parser.add_argument("--provider", default="opencode", help="provider/model for --a2ctl-run-smoke (default: opencode)")
    parser.add_argument(
        "--network-policy",
        default="isolated",
        help="restricted network policy for --a2ctl-run-smoke (default: isolated; example: allowlist:https://api.openai.com); non-restricted values are refused before provider launch",
    )
    parser.add_argument("--json", action="store_true", help="print the full smoke result as JSON")
    return parser.parse_args(argv)


class NetworkPolicySmokeTests(unittest.TestCase):
    def test_sandbox_profile_metadata_hashes_exact_profile_file_text(self) -> None:
        with tempfile.TemporaryDirectory(prefix="a2-network-policy-smoke-test-") as tmp:
            profile = Path(tmp) / "deny-network.sb"
            profile_text = "(version 1)\n(allow default)\n(deny network*)\n"
            profile.write_text(profile_text, encoding="utf-8")

            metadata = sandbox_profile_metadata(profile)

            self.assertEqual(metadata["engine"], "sandbox-exec")
            self.assertEqual(metadata["profile_path"], str(profile))
            self.assertTrue(metadata["profile_path_is_absolute"])
            self.assertTrue(Path(metadata["profile_path"]).is_absolute())
            self.assertEqual(metadata["profile_path_lifetime"], "ephemeral_tempfile_removed_after_smoke")
            self.assertEqual(metadata["durable_audit_fields"], ["profile_sha256", "profile_lines"])
            self.assertEqual(metadata["profile_sha256"], sha256_text(profile_text))
            self.assertEqual(
                metadata["profile_lines"],
                ["(version 1)", "(allow default)", "(deny network*)"],
            )

    def test_sandbox_profile_metadata_changes_when_profile_file_changes(self) -> None:
        with tempfile.TemporaryDirectory(prefix="a2-network-policy-smoke-test-") as tmp:
            profile = Path(tmp) / "deny-network.sb"
            profile.write_text(DENY_NETWORK_PROFILE, encoding="utf-8")
            original = sandbox_profile_metadata(profile)
            profile.write_text(DENY_NETWORK_PROFILE + "; audit marker\n", encoding="utf-8")

            changed = sandbox_profile_metadata(profile)

            self.assertNotEqual(original["profile_sha256"], changed["profile_sha256"])

    def test_smoke_result_profile_audit_requires_profile_commands_and_hash(self) -> None:
        profile_path = "/tmp/deny-network.sb"
        result = {
            "sandbox_profile": {
                "profile_path": profile_path,
                "profile_path_is_absolute": True,
                "profile_path_lifetime": "ephemeral_tempfile_removed_after_smoke",
                "durable_audit_fields": ["profile_sha256", "profile_lines"],
                "profile_sha256": sha256_text(DENY_NETWORK_PROFILE),
                "profile_lines": DENY_NETWORK_PROFILE.splitlines(),
            },
            "local_probe": {"command": ["sandbox-exec", "-f", profile_path, "python3"]},
            "network_probe": {"command": ["sandbox-exec", "-f", profile_path, "python3"]},
        }
        self.assertTrue(smoke_result_profile_audit_ok(result))

        result["network_probe"] = {"command": ["sandbox-exec", "-f", "/tmp/other.sb", "python3"]}
        self.assertFalse(smoke_result_profile_audit_ok(result))

        result["sandbox_profile"]["profile_path"] = "relative.sb"
        result["sandbox_profile"]["profile_path_is_absolute"] = False
        result["local_probe"] = {"command": ["sandbox-exec", "-f", "relative.sb", "python3"]}
        result["network_probe"] = {"command": ["sandbox-exec", "-f", "relative.sb", "python3"]}
        self.assertFalse(smoke_result_profile_audit_ok(result))

    def test_smoke_result_profile_audit_rejects_wrong_hash(self) -> None:
        profile_path = "/tmp/deny-network.sb"
        result = {
            "sandbox_profile": {
                "profile_path": profile_path,
                "profile_path_is_absolute": True,
                "profile_path_lifetime": "ephemeral_tempfile_removed_after_smoke",
                "durable_audit_fields": ["profile_sha256", "profile_lines"],
                "profile_sha256": sha256_text("(version 1)\n"),
                "profile_lines": DENY_NETWORK_PROFILE.splitlines(),
            },
            "local_probe": {"command": ["sandbox-exec", "-f", profile_path, "python3"]},
            "network_probe": {"command": ["sandbox-exec", "-f", profile_path, "python3"]},
        }
        self.assertFalse(smoke_result_profile_audit_ok(result))

    def test_local_allowlist_profile_allows_one_localhost_port_after_default_deny(self) -> None:
        profile = local_allowlist_profile(12345)

        self.assertIn("(deny network*)", profile)
        self.assertIn('(allow network-outbound (remote tcp "localhost:12345"))', profile)
        self.assertNotIn("github.com", profile)

    def test_provider_allowlist_profile_allows_exact_provider_hosts_after_default_deny(self) -> None:
        profile = provider_allowlist_profile(["API.OpenAI.Com", "api.anthropic.com"])

        self.assertIn("(deny network*)", profile)
        self.assertIn('(allow network-outbound (remote tcp "api.openai.com:443"))', profile)
        self.assertIn('(allow network-outbound (remote tcp "api.anthropic.com:443"))', profile)
        self.assertNotIn("github.com", profile)
        self.assertNotIn('"*:443"', profile)
        self.assertNotIn("(allow network*)", profile)
        self.assertNotIn("(allow network-outbound)", profile)

    def test_provider_allowlist_profile_rejects_broad_or_solution_hosts(self) -> None:
        for host in [
            "*",
            "github.com",
            "raw.githubusercontent.com",
            "gist.githubusercontent.com",
            "github.io",
            "user.github.io",
            "api.openai.com/v1",
            "https://api.openai.com",
            "api openai com",
            "localhost",
            "internal",
            "com",
            "api.example-provider.invalid",
            "provider.test",
            "192.168.0.10",
            "",
        ]:
            with self.subTest(host=host):
                with self.assertRaises(ValueError):
                    provider_allowlist_profile([host])

    def test_provider_allowlist_profile_rejects_bool_or_invalid_ports(self) -> None:
        for port in [True, False, 0, -1, 65536]:
            with self.subTest(port=port):
                with self.assertRaises(ValueError):
                    provider_allowlist_profile(["api.openai.com"], port=port)  # type: ignore[arg-type]

    def test_network_policy_arg_classification_matches_cli_rendering(self) -> None:
        self.assertTrue(is_restricted_network_policy_arg("isolated"))
        self.assertTrue(is_restricted_network_policy_arg("IsoLaTeD"))
        self.assertTrue(is_restricted_network_policy_arg("allowlist:https://api.openai.com"))
        self.assertTrue(is_restricted_network_policy_arg("allow-list:https://api.openai.com"))
        self.assertTrue(is_restricted_network_policy_arg("  allowlist:https://api.openai.com  "))
        self.assertFalse(is_restricted_network_policy_arg("open"))
        self.assertFalse(is_restricted_network_policy_arg("allowlist:"))
        self.assertEqual(
            parse_restricted_network_policy_arg("allowlist:"),
            (False, "invalid empty allowlist network policy"),
        )
        self.assertEqual(expected_catalyst_network_policy_label("isolated"), "Isolated")
        self.assertEqual(expected_catalyst_network_policy_label("IsoLaTeD"), "Isolated")
        self.assertEqual(
            expected_catalyst_network_policy_label("allowlist:https://api.openai.com"),
            "AllowList",
        )
        self.assertEqual(
            expected_catalyst_network_policy_label("allow-list:https://api.openai.com"),
            "AllowList",
        )

    def test_a2ctl_boundary_smoke_command_preserves_allowlist_policy_when_sandbox_exec_unavailable(self) -> None:
        import unittest.mock

        completed = subprocess.CompletedProcess(
            args=[],
            returncode=1,
            stdout="network_policy=AllowList prevents launching provider `opencode`\n",
            stderr="restricted network policy blocked provider launch: no candidate patch produced\n",
        )
        with unittest.mock.patch("shutil.which", return_value="/usr/bin/opencode"), unittest.mock.patch(
            __name__ + ".exact_sandbox_exec_available", return_value=False
        ), unittest.mock.patch("subprocess.run", return_value=completed) as run:
            result = run_a2ctl_launch_gate_smoke(
                "opencode",
                "allowlist:https://api.openai.com",
            )

        self.assertTrue(result["complete"])
        self.assertTrue(result["provider_launch_executed"])
        self.assertFalse(result["sandbox_exec_available"])
        self.assertEqual(result["network_policy"], "allowlist:https://api.openai.com")
        self.assertEqual(result["expected_policy_label"], "AllowList")
        command = run.call_args.args[0]
        self.assertIn("--network-policy", command)
        self.assertEqual(
            command[command.index("--network-policy") + 1],
            "allowlist:https://api.openai.com",
        )

    def test_a2ctl_boundary_smoke_stops_before_live_provider_when_sandbox_exec_available(self) -> None:
        import unittest.mock

        with unittest.mock.patch("shutil.which", return_value="/usr/bin/opencode"), unittest.mock.patch(
            __name__ + ".exact_sandbox_exec_available", return_value=True
        ), unittest.mock.patch("subprocess.run") as run:
            result = run_a2ctl_launch_gate_smoke("opencode", "isolated")

        self.assertTrue(result["complete"])
        self.assertFalse(result["provider_launch_executed"])
        self.assertTrue(result["sandbox_exec_available"])
        self.assertIn("not executed", result["stderr"])
        run.assert_not_called()

    def test_a2ctl_launch_gate_smoke_reports_missing_provider_binary_without_launch(self) -> None:
        import unittest.mock

        with unittest.mock.patch("shutil.which", return_value=None), unittest.mock.patch(
            "subprocess.run"
        ) as run:
            result = run_a2ctl_launch_gate_smoke("opencode", "isolated")

        self.assertFalse(result["complete"])
        self.assertEqual(result["network_policy"], "isolated")
        run.assert_not_called()

    def test_a2ctl_launch_gate_smoke_refuses_open_policy_without_launch(self) -> None:
        import unittest.mock

        with unittest.mock.patch("shutil.which") as which, unittest.mock.patch(
            "subprocess.run"
        ) as run:
            result = run_a2ctl_launch_gate_smoke("opencode", "open")

        self.assertFalse(result["complete"])
        self.assertEqual(result["network_policy"], "open")
        self.assertIn("non-restricted network policy", result["stderr"])
        which.assert_not_called()
        run.assert_not_called()

    def test_provider_allowlist_profile_matches_fresh_evidence_contract_shape(self) -> None:
        from bench.self_correction import validate_sandbox_provider_allowlist_evidence
        from bench.self_correction_demo import validate_fresh_sandbox_provider_allowlist_evidence

        profile = provider_allowlist_profile(["api.openai.com"])
        profile_lines = profile.splitlines()
        evidence = {
            "status": "enforced",
            "enforcement_layer": "sandbox-exec-profile-shape-test",
            "launch_boundary": "unit-test-only",
            "benchmark_network_policy": "Isolated",
            "provider_endpoint_allowlist_enforced": True,
            "public_solution_egress_blocked": True,
            "allowed_provider_endpoints": ["https://api.openai.com"],
            "blocked_solution_hosts": ["github.com", "githubusercontent.com", "github.io"],
            "sandbox_profile_sha256": sha256_text(profile),
            "sandbox_profile_lines": profile_lines,
        }

        validate_sandbox_provider_allowlist_evidence(evidence)
        validate_fresh_sandbox_provider_allowlist_evidence(
            {"audited_sandbox_provider_allowlist_evidence": evidence},
            index=1,
        )
        self.assertEqual(sha256_text(profile), sha256_text("\n".join(profile_lines) + "\n"))
        self.assertIn("(deny network*)", profile)
        self.assertIn("api.openai.com", profile)
        self.assertNotIn("github.com", profile)

    def test_smoke_result_profile_audit_accepts_custom_allowlist_probes(self) -> None:
        profile_path = "/tmp/allow-one-localhost-port.sb"
        profile_text = local_allowlist_profile(12345)
        result = {
            "sandbox_profile": {
                "profile_path": profile_path,
                "profile_path_is_absolute": True,
                "profile_path_lifetime": "ephemeral_tempfile_removed_after_smoke",
                "durable_audit_fields": ["profile_sha256", "profile_lines"],
                "profile_sha256": sha256_text(profile_text),
                "profile_lines": profile_text.splitlines(),
            },
            "allowed_probe": {"command": ["sandbox-exec", "-f", profile_path, "python3"]},
            "blocked_probe": {"command": ["sandbox-exec", "-f", profile_path, "python3"]},
            "public_solution_probe": {"command": ["sandbox-exec", "-f", profile_path, "python3"]},
        }

        self.assertTrue(
            smoke_result_profile_audit_ok(
                result,
                expected_profile_text=profile_text,
                probe_names=("allowed_probe", "blocked_probe", "public_solution_probe"),
            )
        )
        result.pop("public_solution_probe")
        self.assertFalse(
            smoke_result_profile_audit_ok(
                result,
                expected_profile_text=profile_text,
                probe_names=("allowed_probe", "blocked_probe", "public_solution_probe"),
            )
        )


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    if args.a2ctl_run_smoke and args.allowlist_smoke:
        raise SystemExit("--a2ctl-run-smoke and --allowlist-smoke are mutually exclusive")
    if args.a2ctl_run_smoke:
        result = run_a2ctl_launch_gate_smoke(args.provider, args.network_policy)
        pass_message = "PASS a2ctl run restricted-policy boundary smoke"
        fail_message = "FAIL a2ctl run restricted-policy boundary smoke"
    elif args.allowlist_smoke:
        result = run_allowlist_smoke()
        pass_message = "PASS network policy allowlist smoke: allowlisted localhost endpoint was reachable and non-allowlisted controls failed"
        fail_message = "FAIL network policy allowlist smoke"
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
