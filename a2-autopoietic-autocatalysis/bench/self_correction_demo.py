#!/usr/bin/env python3
"""Run A²'s auditable self-correction demo gates.

This wrapper has two intentionally separate modes:

* ``verify-archive`` re-scores a durable archived JSONL artifact and proves that it
  contains a failed-attempt -> retry-context -> verified-promotion trajectory.
* ``fresh`` runs the self-correction harness to regenerate a new JSONL artifact,
  then immediately applies the same ``--require-demo`` scorer gate to that output.

The default mode is archive verification because a fresh provider run can be slow
and may consume paid quota. Use ``fresh --preflight-only`` for local no-network
checks before running it, or ``fresh --print-only`` to inspect command wiring only.
"""

from __future__ import annotations

import argparse
import contextlib
import hashlib
import importlib.util
import io
import ipaddress
import json
import re
import shutil
import shlex
import subprocess
import sys
import tempfile
import unittest
import urllib.parse
from pathlib import Path
from unittest import mock


DEFAULT_ARCHIVE = Path(
    "docs/benchmark-results/self-correction/"
    "a2-archive-same-crate-opencode-minimax-m3-20260615T165316Z.jsonl"
)
DEFAULT_ARCHIVE_EVIDENCE = Path(
    "docs/benchmark-results/self-correction/"
    "a2-archive-same-crate-opencode-minimax-m3-20260615T165316Z.demo-evidence.json"
)
DEFAULT_FIXTURE = "compound-archive-same-crate-hidden"
DEFAULT_PROVIDER = "opencode/minimax-coding-plan/MiniMax-M3"
FRESH_PREFLIGHT_BENCHMARK_NETWORK_POLICY = "Isolated"
FRESH_PREFLIGHT_RESTRICTED_NETWORK_BEHAVIOR = (
    "fail_closed_provider_launch_until_audited_sandbox_provider_allowlist"
)
FRESH_PREFLIGHT_SANDBOX_PROVIDER_ALLOWLIST_ENFORCED = False
FRESH_PREFLIGHT_SANDBOX_PROVIDER_ALLOWLIST_STATUS = "not_implemented"
FRESH_PREFLIGHT_AGENT_NETWORK_BOUNDARY_PRECONDITION_EXECUTED = False
FRESH_PREFLIGHT_AGENT_NETWORK_BOUNDARY_PRECONDITION_STATUS = "not_executed_in_preflight"
FRESH_REQUIRED_SANDBOX_PROVIDER_ALLOWLIST_ENFORCED = True
FRESH_REQUIRED_SANDBOX_PROVIDER_ALLOWLIST_STATUS = "enforced"
FRESH_REQUIRED_SANDBOX_PROVIDER_ALLOWLIST_EVIDENCE_FIELD = (
    "audited_sandbox_provider_allowlist_evidence"
)
AGENT_NETWORK_BOUNDARY_INVENTORY_COMMAND = [
    "python3",
    "bench/agent_network_boundary_check.py",
    "--self-test",
]
AGENT_NETWORK_BOUNDARY_INVENTORY_JSON_COMMAND = [
    "python3",
    "bench/agent_network_boundary_check.py",
    "--json",
]
AGENT_NETWORK_BOUNDARY_PRECONDITION_COMMAND = [
    "python3",
    "bench/agent_network_boundary_check.py",
    "--require-sandbox-runtime",
]
SENIOR_SWE_BENCH_SOURCE = "senior-swe-bench"
SENIOR_SWE_BENCH_PROVENANCE_FIELDS = (
    "senior_swe_bench_export_sha256",
    "senior_swe_bench_export_row_index",
)
TEST_SANDBOX_PROFILE_LINES = [
    "(version 1)",
    "(allow default)",
    "(deny network*)",
    '(allow network-outbound (remote tcp "api.openai.com:443"))',
]
TEST_SANDBOX_PROFILE_SHA256 = hashlib.sha256(
    ("\n".join(TEST_SANDBOX_PROFILE_LINES) + "\n").encode("utf-8")
).hexdigest()
HOST_PATH_MARKERS = ("/Users", "/tmp", "/var/folders")
EXPECTED_DEMO_REQUIREMENTS = [
    "failed_first_attempt",
    "archived_verifier_failure_evidence",
    "retry_context_from_failure_evidence",
    "later_passing_attempt",
    "lineage_trajectory_recorded",
    "verifier_gated_germline_promotion",
]
HANDOFF_TEST_COUNTS_PATTERN = re.compile(
    r"\| Tests \| (?P<rust>\d+) Rust \+ "
    r"(?P<self_correction>\d+) self-correction Python \+ "
    r"(?P<scoring>\d+) scoring Python \+ "
    r"(?P<demo_wrapper>\d+) demo-wrapper Python tests \|"
)
LATEST_VERIFICATION_COUNTS_PATTERN = re.compile(
    r"`python3 bench/self_correction_demo\.py --self-test` ran "
    r"(?P<demo_wrapper>\d+) tests OK.*"
    r"`python3 bench/self_correction_score\.py --self-test` ran "
    r"(?P<scoring>\d+) tests OK.*"
    r"`python3 bench/self_correction\.py --self-test` ran "
    r"(?P<self_correction>\d+) tests OK"
)
TEST_COUNT_SUMMARY_PATTERN = re.compile(
    r"\d+ Rust \+ \d+ self-correction Python \+ \d+ scoring Python \+ "
    r"\d+ demo-wrapper Python tests"
)
SELF_TEST_COUNT_PATTERNS = {
    "self_correction": re.compile(
        r"(`python3 bench/self_correction\.py --self-test` ran )\d+( tests OK)"
    ),
    "scoring": re.compile(
        r"(`python3 bench/self_correction_score\.py --self-test` ran )\d+( tests OK)"
    ),
    "demo_wrapper": re.compile(
        r"(`python3 bench/self_correction_demo\.py --self-test` ran )\d+( tests OK)"
    ),
}


def repo_root() -> Path:
    return Path(__file__).resolve().parents[1]


def unittest_count_for_script(script: str) -> int:
    script_path = repo_root() / script
    module_name = f"_a2_count_{script_path.stem}"
    spec = importlib.util.spec_from_file_location(module_name, script_path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"could not import {script} to count unittest cases")
    previous = sys.modules.get(module_name)
    module = importlib.util.module_from_spec(spec)
    sys.modules[module_name] = module
    try:
        spec.loader.exec_module(module)
        return unittest.defaultTestLoader.loadTestsFromModule(module).countTestCases()
    finally:
        if previous is None:
            sys.modules.pop(module_name, None)
        else:
            sys.modules[module_name] = previous


def current_module_self_test_count() -> int:
    return unittest.defaultTestLoader.loadTestsFromModule(sys.modules[__name__]).countTestCases()


def python_test_counts_from_match(match: re.Match[str]) -> dict[str, int]:
    return {
        "self_correction": int(match.group("self_correction")),
        "scoring": int(match.group("scoring")),
        "demo_wrapper": int(match.group("demo_wrapper")),
    }


def handoff_current_test_counts_match() -> re.Match[str]:
    handoff = repo_root() / "docs/HANDOFF.md"
    for line in handoff.read_text(encoding="utf-8").splitlines():
        match = HANDOFF_TEST_COUNTS_PATTERN.fullmatch(line.strip())
        if match:
            return match
    raise RuntimeError("docs/HANDOFF.md Current Numbers test-count row was not found")


def handoff_current_python_test_counts() -> dict[str, int]:
    return python_test_counts_from_match(handoff_current_test_counts_match())


def handoff_current_rust_test_count() -> int:
    return int(handoff_current_test_counts_match().group("rust"))


def rust_test_count_from_cargo_test_list_output(output: str) -> int:
    return sum(1 for line in output.splitlines() if line.rstrip().endswith(": test"))


def cargo_rust_test_count() -> int:
    result = subprocess.run(
        ["cargo", "test", "--", "--list"],
        cwd=repo_root(),
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=True,
    )
    return rust_test_count_from_cargo_test_list_output(result.stdout)


def latest_verification_python_test_counts(path: Path) -> dict[str, int]:
    for line in path.read_text(encoding="utf-8").splitlines():
        match = LATEST_VERIFICATION_COUNTS_PATTERN.search(line)
        if match:
            return python_test_counts_from_match(match)
    raise RuntimeError(f"{path} latest verification self-test counts were not found")


def documented_counts_summary(rust_count: int, python_counts: dict[str, int]) -> str:
    return (
        f"{rust_count} Rust + {python_counts['self_correction']} self-correction Python + "
        f"{python_counts['scoring']} scoring Python + "
        f"{python_counts['demo_wrapper']} demo-wrapper Python tests"
    )


def replace_count_markers_in_line(
    line: str,
    *,
    rust_count: int,
    python_counts: dict[str, int],
) -> tuple[str, int]:
    replacements = 0
    line, count = TEST_COUNT_SUMMARY_PATTERN.subn(
        documented_counts_summary(rust_count, python_counts), line
    )
    replacements += count
    for key, pattern in SELF_TEST_COUNT_PATTERNS.items():
        line, count = pattern.subn(rf"\g<1>{python_counts[key]}\g<2>", line)
        replacements += count
    return line, replacements


def replace_documented_counts(
    text: str,
    *,
    rust_count: int,
    python_counts: dict[str, int],
) -> tuple[str, int]:
    lines = text.splitlines(keepends=True)
    replacements = 0
    current_row_seen = False
    latest_line_seen = False
    for index, line in enumerate(lines):
        stripped = line.strip()
        if HANDOFF_TEST_COUNTS_PATTERN.fullmatch(stripped):
            if current_row_seen:
                raise RuntimeError("multiple Current Numbers test-count rows found")
            line_ending = "\n" if line.endswith("\n") else ""
            lines[index] = f"| Tests | {documented_counts_summary(rust_count, python_counts)} |{line_ending}"
            current_row_seen = True
            replacements += 1
            continue
        if LATEST_VERIFICATION_COUNTS_PATTERN.search(line):
            if latest_line_seen:
                raise RuntimeError("multiple latest verification count lines found")
            lines[index], count = replace_count_markers_in_line(
                line,
                rust_count=rust_count,
                python_counts=python_counts,
            )
            latest_line_seen = True
            replacements += count
    if not current_row_seen and not latest_line_seen:
        raise RuntimeError("no documented count markers found")
    return "".join(lines), replacements


def display_command(command: list[str]) -> str:
    root = repo_root()
    display: list[str] = []
    for part in command:
        try:
            path = Path(part)
            if path.is_absolute() and path.is_relative_to(root):
                display.append(str(path.relative_to(root)))
                continue
        except ValueError:
            pass
        display.append(part)
    return shlex.join(display)


def score_command(logfile: Path, evidence_json: Path | None = None) -> list[str]:
    root = repo_root()
    command = [
        str(root / "bench/self_correction_score.py"),
        "--require-demo",
        "--trajectories",
    ]
    if evidence_json is not None:
        command.extend(["--demo-evidence-json", str(evidence_json)])
    command.append(str(logfile))
    return command


REGENERATION_VOLATILE_EVIDENCE_KEYS = frozenset(
    {
        "created_at",
        "created_at_utc",
        "evidence_json_path",
        "evidence_output_path",
        "generated_at",
        "generated_at_utc",
    }
)


def normalized_evidence_for_regeneration(value: object, *, _depth: int = 0) -> object:
    """Normalize evidence JSON before comparing clean-room regeneration output.

    This intentionally preserves the source artifact path and row selectors. Only
    top-level evidence-output bookkeeping/timestamps are ignored so future
    harmless emission metadata cannot mask stale causal-chain proof, while nested
    row/proof fields remain semantically checked.
    """
    if isinstance(value, dict):
        return {
            key: normalized_evidence_for_regeneration(value[key], _depth=_depth + 1)
            for key in sorted(value)
            if _depth > 0 or key not in REGENERATION_VOLATILE_EVIDENCE_KEYS
        }
    if isinstance(value, list):
        return [normalized_evidence_for_regeneration(item, _depth=_depth + 1) for item in value]
    return value


def canonical_json_sha256(value: object) -> str:
    payload = json.dumps(value, sort_keys=True, separators=(",", ":")).encode("utf-8")
    return hashlib.sha256(payload).hexdigest()


def normalized_evidence_sha256(path: Path) -> str:
    return canonical_json_sha256(normalized_evidence_for_regeneration(load_evidence_json(path)))


def require_existing_normalized_evidence_sha256(path: Path) -> str:
    resolved = repo_path(path)
    if not resolved.exists() or resolved.stat().st_size == 0:
        raise RuntimeError(
            "checked-in demo evidence JSON must exist and be non-empty before verify-archive scoring: "
            f"{path}"
        )
    return normalized_evidence_sha256(path)


def require_checked_in_evidence_unchanged(path: Path, original_sha256: str | None) -> None:
    if original_sha256 is None:
        return
    current_sha256 = normalized_evidence_sha256(path)
    if current_sha256 != original_sha256:
        raise RuntimeError(
            "verify-archive changed the normalized checked-in demo evidence JSON: "
            f"before_sha256={original_sha256} after_sha256={current_sha256}. "
            "Review and commit the regenerated evidence before treating it as archived proof."
        )


def verify_archive_evidence_regeneration(archive: Path, evidence_json: Path) -> None:
    with tempfile.TemporaryDirectory(prefix="a2-archive-evidence-regeneration-") as tmpdir:
        regenerated_evidence = Path(tmpdir) / "regenerated.demo-evidence.json"
        if regenerated_evidence.exists():
            raise RuntimeError(
                "clean-room demo evidence regeneration output unexpectedly preexists: "
                f"{regenerated_evidence}"
            )
        result = run_command(score_command(archive, regenerated_evidence), print_only=False)
        if result != 0:
            raise RuntimeError(
                "clean-room demo evidence regeneration scorer failed before producing comparable output"
            )
        if not regenerated_evidence.exists() or regenerated_evidence.stat().st_size == 0:
            raise RuntimeError(
                "clean-room demo evidence regeneration did not create a non-empty evidence JSON"
            )
        expected = normalized_evidence_for_regeneration(load_evidence_json(evidence_json))
        regenerated = normalized_evidence_for_regeneration(load_evidence_json(regenerated_evidence))
        expected_sha = canonical_json_sha256(expected)
        regenerated_sha = canonical_json_sha256(regenerated)
        if expected != regenerated:
            raise RuntimeError(
                "clean-room demo evidence regeneration produced different evidence JSON: "
                f"checked_in_sha256={expected_sha} regenerated_sha256={regenerated_sha}"
            )
        print(
            "PASS clean-room evidence regeneration: temp output was absent before scoring; "
            f"normalized SHA-256 matches checked-in evidence ({expected_sha})"
        )


def fresh_contract_command(args: argparse.Namespace, evidence_json: Path) -> list[str]:
    root = repo_root()
    command = [
        str(root / "bench/self_correction_demo.py"),
        "verify-evidence-contract",
        "--evidence-json",
        str(evidence_json),
        "--reference-evidence-json",
        str(DEFAULT_ARCHIVE_EVIDENCE),
        "--fresh-run-id",
        args.run_id,
        "--max-tokens",
        str(args.max_tokens),
        "--timeout",
        str(args.timeout),
        "--require-current-head",
    ]
    if args.allow_dirty_source:
        command.append("--allow-dirty-source")
    return command


def default_fresh_evidence_path(results: Path) -> Path:
    if results.suffix == ".jsonl":
        return results.with_suffix(".demo-evidence.json")
    return Path(f"{results}.demo-evidence.json")


def repo_path(path: Path) -> Path:
    return path if path.is_absolute() else repo_root() / path


def repo_relative_path_for_git(path: Path, *, label: str) -> str:
    resolved = repo_path(path).resolve(strict=False)
    root = repo_root().resolve(strict=False)
    try:
        return resolved.relative_to(root).as_posix()
    except ValueError as exc:
        raise RuntimeError(f"{label} is outside the repository: {path}") from exc


def require_git_tracked_path(path: Path, *, label: str) -> None:
    relative = repo_relative_path_for_git(path, label=label)
    tracked_paths = set(git_output(["ls-files", "--", relative]).splitlines())
    if relative not in tracked_paths:
        raise RuntimeError(
            f"{label} is not git-tracked: {path}. "
            "Reproducible archived demo evidence must use git-tracked artifact files."
        )


def ensure_output_path_empty(path: Path, *, label: str) -> None:
    resolved = repo_path(path)
    if resolved.exists() and resolved.stat().st_size > 0:
        raise RuntimeError(
            f"fresh demo {label} path already contains data: {path}. "
            "Use a unique path or remove/truncate the file first."
        )


def ensure_fresh_results_path(results: Path) -> None:
    ensure_output_path_empty(results, label="results")


def ensure_fresh_evidence_path(evidence_json: Path) -> None:
    ensure_output_path_empty(evidence_json, label="evidence")


def provider_binary_name(provider: str) -> str:
    family = provider.split("/", 1)[0]
    return {
        "opencode": "opencode",
        "pi": "pi",
        "claude": "claude",
        "codex": "codex",
        "gemini": "gemini",
    }.get(family, family)


def ensure_provider_binary(provider: str) -> None:
    binary = provider_binary_name(provider)
    if shutil.which(binary) is None:
        raise RuntimeError(
            f"fresh demo provider binary {binary!r} for provider {provider!r} was not found in PATH"
        )


def opencode_auth_path() -> Path:
    return Path.home() / ".local/share/opencode/auth.json"


def ensure_opencode_provider_config(provider: str, *, auth_path: Path | None = None) -> None:
    parts = provider.split("/")
    if len(parts) < 2:
        return
    configured_provider = parts[1]
    auth_path = auth_path or opencode_auth_path()
    if not auth_path.exists():
        raise RuntimeError(
            f"fresh demo opencode credentials file was not found: {auth_path}"
        )
    try:
        auth = json.loads(auth_path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        raise RuntimeError(
            f"fresh demo opencode credentials file is invalid JSON: {auth_path}: {exc}"
        ) from exc
    if not isinstance(auth, dict) or configured_provider not in auth:
        raise RuntimeError(
            "fresh demo opencode credentials do not include provider "
            f"{configured_provider!r} in {auth_path}"
        )


def ensure_provider_config(provider: str) -> None:
    family = provider.split("/", 1)[0]
    if family == "opencode":
        ensure_opencode_provider_config(provider)


def git_output(args: list[str]) -> str:
    result = subprocess.run(
        ["git", *args],
        cwd=repo_root(),
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    if result.returncode != 0:
        raise RuntimeError(f"could not run git {' '.join(args)}: {result.stderr.strip()}")
    return result.stdout.strip()


def current_source_metadata() -> dict[str, object]:
    branch = git_output(["branch", "--show-current"])
    return {
        "source_head": git_output(["rev-parse", "HEAD"]),
        "source_head_short": git_output(["rev-parse", "--short", "HEAD"]),
        "source_branch": branch or "(detached)",
        "source_dirty": bool(git_output(["status", "--porcelain", "--", "."])),
    }


def ensure_clean_source() -> None:
    if current_source_metadata()["source_dirty"]:
        raise RuntimeError(
            "fresh demo source tree is dirty; commit/stash changes or pass --allow-dirty-source"
        )


def ensure_fresh_output_paths_empty(args: argparse.Namespace, evidence_json: Path) -> None:
    ensure_fresh_results_path(args.results)
    ensure_fresh_evidence_path(evidence_json)


def fresh_provider_preflight_after_output_paths(args: argparse.Namespace) -> None:
    ensure_provider_binary(args.provider)
    ensure_provider_config(args.provider)
    if not args.allow_dirty_source:
        ensure_clean_source()


def fresh_preflight(args: argparse.Namespace, evidence_json: Path) -> None:
    ensure_fresh_output_paths_empty(args, evidence_json)
    fresh_provider_preflight_after_output_paths(args)


def ensure_agent_network_boundary_precondition_ready() -> None:
    result = subprocess.run(
        AGENT_NETWORK_BOUNDARY_PRECONDITION_COMMAND,
        cwd=repo_root(),
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        detail_parts = []
        if result.stdout.strip():
            detail_parts.append(f"stdout={result.stdout.strip()!r}")
        if result.stderr.strip():
            detail_parts.append(f"stderr={result.stderr.strip()!r}")
        details = "; ".join(detail_parts)
        if details:
            details = f" ({details})"
        raise RuntimeError(
            "fresh provider-backed runs are blocked because the agent network boundary "
            "precondition failed closed before provider launch; command="
            f"{display_command(AGENT_NETWORK_BOUNDARY_PRECONDITION_COMMAND)!r}{details}"
        )


def ensure_fresh_sandbox_provider_allowlist_ready() -> None:
    if not FRESH_PREFLIGHT_SANDBOX_PROVIDER_ALLOWLIST_ENFORCED:
        raise RuntimeError(
            "fresh provider-backed runs are blocked because no audited sandbox/provider "
            "allowlist is enforced yet; use --preflight-only for readiness checks, "
            "or wire and verify sandbox/provider-allowlist enforcement before "
            "running --confirm-provider-run"
        )
    if FRESH_PREFLIGHT_SANDBOX_PROVIDER_ALLOWLIST_STATUS != "enforced":
        raise RuntimeError(
            "fresh provider-backed runs require audited sandbox/provider allowlist "
            "status=enforced; current status is "
            f"{FRESH_PREFLIGHT_SANDBOX_PROVIDER_ALLOWLIST_STATUS}"
        )


def provider_config_checked(provider: str) -> bool:
    parts = provider.split("/")
    return len(parts) >= 2 and parts[0] == "opencode"


def paths_alias(left: Path, right: Path) -> bool:
    return repo_path(left).resolve(strict=False) == repo_path(right).resolve(strict=False)


def ensure_preflight_report_path(report: Path, *, results: Path, evidence_json: Path) -> None:
    if paths_alias(report, results):
        raise RuntimeError(
            "fresh demo preflight report path must be distinct from results path: "
            f"{report}"
        )
    if paths_alias(report, evidence_json):
        raise RuntimeError(
            "fresh demo preflight report path must be distinct from evidence path: "
            f"{report}"
        )
    ensure_output_path_empty(report, label="preflight report")


def ensure_preflight_boundary_inventory_path(
    path: Path,
    *,
    results: Path,
    evidence_json: Path,
    preflight_report_json: Path | None,
) -> None:
    if paths_alias(path, results):
        raise RuntimeError(
            "fresh demo boundary inventory path must be distinct from results path: "
            f"{path}"
        )
    if paths_alias(path, evidence_json):
        raise RuntimeError(
            "fresh demo boundary inventory path must be distinct from evidence path: "
            f"{path}"
        )
    if preflight_report_json is not None and paths_alias(path, preflight_report_json):
        raise RuntimeError(
            "fresh demo boundary inventory path must be distinct from preflight report path: "
            f"{path}"
        )
    ensure_output_path_empty(path, label="boundary inventory")


def run_agent_network_boundary_inventory_json(path: Path) -> dict[str, object]:
    result = subprocess.run(
        AGENT_NETWORK_BOUNDARY_INVENTORY_JSON_COMMAND,
        cwd=repo_root(),
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        detail_parts = []
        if result.stdout.strip():
            detail_parts.append(f"stdout={result.stdout.strip()!r}")
        if result.stderr.strip():
            detail_parts.append(f"stderr={result.stderr.strip()!r}")
        details = "; ".join(detail_parts)
        if details:
            details = f" ({details})"
        raise RuntimeError(
            "agent network boundary inventory JSON command failed during fresh preflight; "
            f"command={display_command(AGENT_NETWORK_BOUNDARY_INVENTORY_JSON_COMMAND)!r}{details}"
        )
    try:
        inventory = json.loads(result.stdout)
    except json.JSONDecodeError as exc:
        raise RuntimeError(
            "agent network boundary inventory JSON command produced invalid JSON"
        ) from exc
    if not isinstance(inventory, dict):
        raise RuntimeError("agent network boundary inventory JSON is not an object")
    resolved = repo_path(path)
    resolved.parent.mkdir(parents=True, exist_ok=True)
    resolved.write_text(json.dumps(inventory, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    a2_boundary = inventory.get("a2_owned_provider_launch_boundary")
    sandbox_runtime = inventory.get("sandbox_runtime")
    return {
        "path": str(path),
        "command": display_command(AGENT_NETWORK_BOUNDARY_INVENTORY_JSON_COMMAND),
        "status": "recorded",
        "creates_loop_evidence": False,
        "proves_runtime_sandbox_enforcement": False,
        "a2_owned_fail_closed": bool(
            isinstance(a2_boundary, dict)
            and a2_boundary.get("fail_closed_restricted_policies") is True
        ),
        "a2_owned_sandbox_enforced": bool(
            isinstance(a2_boundary, dict)
            and a2_boundary.get("sandbox_enforced_for_restricted_policies") is True
        ),
        "sandbox_runtime_available": bool(
            isinstance(sandbox_runtime, dict) and sandbox_runtime.get("available") is True
        ),
        "launch_sandbox_enforced": bool(inventory.get("launch_sandbox_enforced") is True),
    }


def fresh_preflight_report(
    args: argparse.Namespace,
    evidence_json: Path,
    *,
    boundary_inventory: dict[str, object] | None = None,
) -> dict[str, object]:
    config_checked = provider_config_checked(args.provider)
    source_metadata = current_source_metadata()
    boundary_inventory_path = getattr(args, "preflight_boundary_inventory_json", None)
    boundary_inventory_created = boundary_inventory is not None
    return {
        "mode": "fresh_preflight",
        "creates_loop_evidence": False,
        "provider_backed_benchmark_executed": False,
        "results_created": False,
        "evidence_json_created": False,
        "fresh_provenance_contract_executed": False,
        "live_provider_auth_quota_model_checked": False,
        "results": str(args.results),
        "evidence_json": str(evidence_json),
        "preflight_report_json": str(args.preflight_report_json),
        "boundary_inventory_created": boundary_inventory_created,
        "boundary_inventory_json": str(boundary_inventory_path) if boundary_inventory_path else None,
        "boundary_inventory": boundary_inventory,
        "fixture": args.fixture,
        "provider": args.provider,
        "run_id": args.run_id,
        "runs": args.runs,
        "attempts": args.attempts,
        "max_tokens": args.max_tokens,
        "timeout_secs": args.timeout,
        "source_metadata": source_metadata,
        "checks": {
            "results_path_empty": True,
            "evidence_path_empty": True,
            "preflight_report_path_empty": True,
            "preflight_report_path_distinct_from_results": True,
            "preflight_report_path_distinct_from_evidence": True,
            "provider_binary": provider_binary_name(args.provider),
            "provider_binary_present": True,
            "local_provider_config_checked": config_checked,
            "local_provider_config_present_when_supported": True if config_checked else None,
            "source_clean_required": not args.allow_dirty_source,
            "source_clean": None if args.allow_dirty_source else source_metadata["source_dirty"] is False,
            "source_clean_checked_before_output_creation": None
            if args.allow_dirty_source
            else True,
            "dirty_source_allowed": args.allow_dirty_source,
            "benchmark_task_network_policy": FRESH_PREFLIGHT_BENCHMARK_NETWORK_POLICY,
            "restricted_network_policy_current_behavior": FRESH_PREFLIGHT_RESTRICTED_NETWORK_BEHAVIOR,
            "audited_sandbox_provider_allowlist_enforced": FRESH_PREFLIGHT_SANDBOX_PROVIDER_ALLOWLIST_ENFORCED,
            "audited_sandbox_provider_allowlist_status": FRESH_PREFLIGHT_SANDBOX_PROVIDER_ALLOWLIST_STATUS,
            "agent_network_boundary_precondition_required": True,
            "agent_network_boundary_precondition_executed": FRESH_PREFLIGHT_AGENT_NETWORK_BOUNDARY_PRECONDITION_EXECUTED,
            "agent_network_boundary_precondition_status": FRESH_PREFLIGHT_AGENT_NETWORK_BOUNDARY_PRECONDITION_STATUS,
            "agent_network_boundary_inventory_json_requested": boundary_inventory_path is not None,
            "agent_network_boundary_inventory_json_executed": boundary_inventory_created,
            "agent_network_boundary_inventory_json_status": "recorded" if boundary_inventory_created else "not_requested",
        },
        "commands": {
            "agent_network_boundary_inventory": display_command(AGENT_NETWORK_BOUNDARY_INVENTORY_COMMAND),
            "agent_network_boundary_inventory_json": display_command(AGENT_NETWORK_BOUNDARY_INVENTORY_JSON_COMMAND),
            "agent_network_boundary_precondition": display_command(AGENT_NETWORK_BOUNDARY_PRECONDITION_COMMAND),
            "harness": display_command(fresh_command(args)),
            "validation": fresh_validation_summary(args),
            "scorer": display_command(score_command(args.results, evidence_json)),
            "fresh_provenance_contract": display_command(
                fresh_contract_command(args, evidence_json)
            ),
        },
        "notes": [
            "No provider-backed benchmark was executed by this preflight.",
            "No results JSONL, demo-evidence JSON, or fresh provenance contract result was created by this preflight; the named results/evidence paths are future outputs only.",
            "Live provider auth, quota, and model availability are not verified until the fresh run executes.",
            "Clean-source readiness and source revision metadata are checked before fresh results/evidence files are created; newly generated rows record that pre-run source state, and the new artifacts must then be archived deliberately.",
            "Benchmark task payloads request network_policy=Isolated; current provider-backed runs under restricted policy are expected to fail closed until an audited sandbox/provider allowlist exists.",
            "No audited sandbox/provider allowlist is enforced for fresh provider-backed demo execution yet; this report records status=not_implemented rather than treating preflight as sandbox evidence.",
            "This preflight records the agent network boundary precondition command but does not execute it; the confirmed fresh wrapper runs it before provider launch and it is expected to fail closed until sandbox runtime support and launch wrappers are wired.",
            "Optional --preflight-boundary-inventory-json records the source-boundary --json audit for operators, but that inventory is still readiness/gap evidence only and does not prove runtime sandbox enforcement or loop behavior.",
            "This report is readiness evidence only; it is not loop evidence and contains no failed-attempt/retry/promotion proof.",
        ],
    }


def write_fresh_preflight_report(
    path: Path,
    report: dict[str, object],
    *,
    results: Path,
    evidence_json: Path,
) -> None:
    ensure_preflight_report_path(path, results=results, evidence_json=evidence_json)
    resolved = repo_path(path)
    resolved.parent.mkdir(parents=True, exist_ok=True)
    resolved.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def load_preflight_report(path: Path) -> dict[str, object]:
    resolved = repo_path(path)
    try:
        data = json.loads(resolved.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        raise RuntimeError(f"invalid fresh preflight report JSON: {path}: {exc}") from exc
    if not isinstance(data, dict):
        raise RuntimeError(f"fresh preflight report is not a JSON object: {path}")
    return data


def verify_fresh_preflight_report(path: Path, *, require_current_head: bool = False) -> None:
    report = load_preflight_report(path)
    if report.get("mode") != "fresh_preflight":
        raise RuntimeError("fresh preflight report mode is not fresh_preflight")
    for key in [
        "creates_loop_evidence",
        "provider_backed_benchmark_executed",
        "results_created",
        "evidence_json_created",
        "fresh_provenance_contract_executed",
        "live_provider_auth_quota_model_checked",
    ]:
        if report.get(key) is not False:
            raise RuntimeError(f"fresh preflight report {key} must be false")
    boundary_inventory_created = report.get("boundary_inventory_created")
    boundary_inventory = report.get("boundary_inventory")
    if boundary_inventory_created is True:
        if not isinstance(boundary_inventory, dict):
            raise RuntimeError(
                "fresh preflight report boundary_inventory_created=true but lacks boundary_inventory"
            )
        if boundary_inventory.get("creates_loop_evidence") is not False:
            raise RuntimeError("fresh preflight boundary inventory must not claim loop evidence")
        if boundary_inventory.get("proves_runtime_sandbox_enforcement") is not False:
            raise RuntimeError(
                "fresh preflight boundary inventory must not claim runtime sandbox enforcement"
            )
        declared_inventory_path = report.get("boundary_inventory_json")
        if isinstance(declared_inventory_path, str) and declared_inventory_path:
            resolved_inventory = repo_path(Path(declared_inventory_path))
            if not resolved_inventory.exists() or resolved_inventory.stat().st_size == 0:
                raise RuntimeError(
                    "fresh preflight report declares a boundary inventory artifact, but the path is missing or empty: "
                    f"{declared_inventory_path}"
                )
    elif boundary_inventory_created is False and boundary_inventory is not None:
        raise RuntimeError(
            "fresh preflight report has boundary_inventory despite boundary_inventory_created=false"
        )
    checks = report.get("checks")
    current_report_shape = any(
        key in report for key in ["checks", "results", "evidence_json", "preflight_report_json"]
    )
    if checks is None and not current_report_shape:
        benchmark_task_network_policy = "legacy report: not recorded"
        restricted_network_policy_current_behavior = "legacy report: not recorded"
        sandbox_provider_allowlist_enforced: object = "legacy report: not recorded"
        sandbox_provider_allowlist_status = "legacy report: not recorded"
        agent_network_boundary_precondition_required: object = "legacy report: not recorded"
        agent_network_boundary_precondition_executed: object = "legacy report: not recorded"
        agent_network_boundary_precondition_status = "legacy report: not recorded"
    else:
        if not isinstance(checks, dict):
            raise RuntimeError("fresh preflight report lacks checks")
        if checks.get("benchmark_task_network_policy") != FRESH_PREFLIGHT_BENCHMARK_NETWORK_POLICY:
            raise RuntimeError(
                "fresh preflight report checks.benchmark_task_network_policy must be "
                f"{FRESH_PREFLIGHT_BENCHMARK_NETWORK_POLICY}"
            )
        if checks.get("restricted_network_policy_current_behavior") != FRESH_PREFLIGHT_RESTRICTED_NETWORK_BEHAVIOR:
            raise RuntimeError(
                "fresh preflight report checks.restricted_network_policy_current_behavior must record "
                f"{FRESH_PREFLIGHT_RESTRICTED_NETWORK_BEHAVIOR}"
            )
        if (
            checks.get("audited_sandbox_provider_allowlist_enforced")
            is not FRESH_PREFLIGHT_SANDBOX_PROVIDER_ALLOWLIST_ENFORCED
        ):
            raise RuntimeError(
                "fresh preflight report checks.audited_sandbox_provider_allowlist_enforced "
                f"must be {FRESH_PREFLIGHT_SANDBOX_PROVIDER_ALLOWLIST_ENFORCED} until an audited sandbox/provider allowlist is wired"
            )
        if (
            checks.get("audited_sandbox_provider_allowlist_status")
            != FRESH_PREFLIGHT_SANDBOX_PROVIDER_ALLOWLIST_STATUS
        ):
            raise RuntimeError(
                "fresh preflight report checks.audited_sandbox_provider_allowlist_status must record "
                f"{FRESH_PREFLIGHT_SANDBOX_PROVIDER_ALLOWLIST_STATUS}"
            )
        if checks.get("agent_network_boundary_precondition_required") is not True:
            raise RuntimeError(
                "fresh preflight report checks.agent_network_boundary_precondition_required must be true"
            )
        if (
            checks.get("agent_network_boundary_precondition_executed")
            is not FRESH_PREFLIGHT_AGENT_NETWORK_BOUNDARY_PRECONDITION_EXECUTED
        ):
            raise RuntimeError(
                "fresh preflight report checks.agent_network_boundary_precondition_executed "
                "must be false because preflight records the command but does not run the host-dependent precondition"
            )
        if (
            checks.get("agent_network_boundary_precondition_status")
            != FRESH_PREFLIGHT_AGENT_NETWORK_BOUNDARY_PRECONDITION_STATUS
        ):
            raise RuntimeError(
                "fresh preflight report checks.agent_network_boundary_precondition_status must record "
                f"{FRESH_PREFLIGHT_AGENT_NETWORK_BOUNDARY_PRECONDITION_STATUS}"
            )
        benchmark_task_network_policy = checks["benchmark_task_network_policy"]
        restricted_network_policy_current_behavior = checks[
            "restricted_network_policy_current_behavior"
        ]
        sandbox_provider_allowlist_enforced = checks[
            "audited_sandbox_provider_allowlist_enforced"
        ]
        sandbox_provider_allowlist_status = checks[
            "audited_sandbox_provider_allowlist_status"
        ]
        agent_network_boundary_precondition_required = checks[
            "agent_network_boundary_precondition_required"
        ]
        agent_network_boundary_precondition_executed = checks[
            "agent_network_boundary_precondition_executed"
        ]
        agent_network_boundary_precondition_status = checks[
            "agent_network_boundary_precondition_status"
        ]
    for key, label in [("results", "results"), ("evidence_json", "evidence")]:
        declared_path = report.get(key)
        if isinstance(declared_path, str) and declared_path:
            resolved = repo_path(Path(declared_path))
            if resolved.exists() and resolved.stat().st_size > 0:
                raise RuntimeError(
                    f"fresh preflight report declared {label}_created=false, but the "
                    f"declared {label} path now contains data: {declared_path}. "
                    "Generate a new just-in-time preflight report before treating readiness as current."
                )
    source_metadata = report.get("source_metadata")
    if not isinstance(source_metadata, dict):
        raise RuntimeError("fresh preflight report lacks source_metadata")
    source_head = source_metadata.get("source_head")
    if not isinstance(source_head, str) or not re.fullmatch(r"[0-9a-f]{40}", source_head):
        raise RuntimeError("fresh preflight report source_head must be a 40-character hex git commit")
    source_dirty = source_metadata.get("source_dirty")
    if not isinstance(source_dirty, bool):
        raise RuntimeError("fresh preflight report source_dirty must be boolean")
    current = current_source_metadata()
    current_head = current["source_head"]
    current_dirty = current["source_dirty"]
    print("Fresh preflight report check")
    print(f"  report: {path}")
    print(f"  report_source_head: {source_head}")
    print(f"  current_head: {current_head}")
    print(f"  report_source_dirty: {source_dirty}")
    print(f"  current_source_dirty: {current_dirty}")
    print(f"  benchmark_task_network_policy: {benchmark_task_network_policy}")
    print(
        "  restricted_network_policy_current_behavior: "
        f"{restricted_network_policy_current_behavior}"
    )
    print(
        "  audited_sandbox_provider_allowlist_enforced: "
        f"{sandbox_provider_allowlist_enforced}"
    )
    print(
        "  audited_sandbox_provider_allowlist_status: "
        f"{sandbox_provider_allowlist_status}"
    )
    print(
        "  agent_network_boundary_precondition_required: "
        f"{agent_network_boundary_precondition_required}"
    )
    print(
        "  agent_network_boundary_precondition_executed: "
        f"{agent_network_boundary_precondition_executed}"
    )
    print(
        "  agent_network_boundary_precondition_status: "
        f"{agent_network_boundary_precondition_status}"
    )
    print("  readiness only: no provider-backed benchmark/results/evidence/contract/live-auth check ran")
    print("  not loop evidence: no failed-attempt/retry/promotion proof")
    if require_current_head and current_head != source_head:
        raise RuntimeError(
            "fresh preflight report source_head differs from current HEAD; rerun preflight "
            "or the confirmed fresh provider-backed command from the intended HEAD"
        )
    if require_current_head and current_dirty != source_dirty:
        raise RuntimeError(
            "fresh preflight report source_dirty differs from current source state; rerun preflight"
        )
    if current_head == source_head and current_dirty == source_dirty:
        print("  PASS source snapshot matches current HEAD/state")
    else:
        print("  STALE source snapshot differs from current HEAD/state")


def load_jsonl(path: Path) -> list[dict[str, object]]:
    resolved = repo_path(path)
    if not resolved.exists():
        raise RuntimeError(f"fresh demo results file was not created: {path}")
    rows: list[dict[str, object]] = []
    with resolved.open(encoding="utf-8") as handle:
        for line_number, line in enumerate(handle, start=1):
            if not line.strip():
                continue
            try:
                row = json.loads(line)
            except json.JSONDecodeError as exc:
                raise RuntimeError(
                    f"invalid JSONL in fresh demo results at line {line_number}: {exc}"
                ) from exc
            if not isinstance(row, dict):
                raise RuntimeError(
                    f"fresh demo results line {line_number} is not a JSON object"
                )
            rows.append(row)
    return rows


def run_id_matches(row_run_id: object, expected: str) -> bool:
    if not isinstance(row_run_id, str):
        return False
    if row_run_id == expected:
        return True
    prefix = f"{expected}-"
    suffix = row_run_id.removeprefix(prefix)
    return row_run_id.startswith(prefix) and suffix.isdecimal()


def validate_fresh_rows_for_host_path_markers(
    rows: list[dict[str, object]], *, source_label: str
) -> None:
    for index, row in enumerate(rows, start=1):
        serialized = json.dumps(row, sort_keys=True)
        leaked = [marker for marker in HOST_PATH_MARKERS if marker in serialized]
        if leaked:
            raise RuntimeError(
                f"fresh demo row {index} contains host-specific path marker(s) "
                f"in {source_label}: " + ", ".join(leaked)
            )


def validate_fresh_sandbox_provider_allowlist_evidence(
    row: dict[str, object], *, index: int
) -> None:
    evidence = row.get(FRESH_REQUIRED_SANDBOX_PROVIDER_ALLOWLIST_EVIDENCE_FIELD)
    if not isinstance(evidence, dict):
        raise RuntimeError(
            "fresh demo row "
            f"{index} records audited sandbox/provider allowlist enforcement without "
            f"{FRESH_REQUIRED_SANDBOX_PROVIDER_ALLOWLIST_EVIDENCE_FIELD} evidence"
        )
    for key in ("status", "enforcement_layer", "launch_boundary"):
        if not isinstance(evidence.get(key), str) or not evidence.get(key):
            raise RuntimeError(
                f"fresh demo row {index} sandbox/provider allowlist evidence lacks {key}"
            )
    if evidence.get("status") != FRESH_REQUIRED_SANDBOX_PROVIDER_ALLOWLIST_STATUS:
        raise RuntimeError(
            f"fresh demo row {index} sandbox/provider allowlist evidence status must be "
            f"{FRESH_REQUIRED_SANDBOX_PROVIDER_ALLOWLIST_STATUS!r}"
        )
    if evidence.get("benchmark_network_policy") != FRESH_PREFLIGHT_BENCHMARK_NETWORK_POLICY:
        raise RuntimeError(
            f"fresh demo row {index} sandbox/provider allowlist evidence must record "
            f"benchmark_network_policy={FRESH_PREFLIGHT_BENCHMARK_NETWORK_POLICY!r}"
        )
    for key in ("provider_endpoint_allowlist_enforced", "public_solution_egress_blocked"):
        if evidence.get(key) is not True:
            raise RuntimeError(
                f"fresh demo row {index} sandbox/provider allowlist evidence must record {key}=true"
            )
    allowed_endpoints = evidence.get("allowed_provider_endpoints")
    if not isinstance(allowed_endpoints, list) or not allowed_endpoints or not all(
        isinstance(endpoint, str) and endpoint for endpoint in allowed_endpoints
    ):
        raise RuntimeError(
            f"fresh demo row {index} sandbox/provider allowlist evidence must record allowed_provider_endpoints"
        )
    blocked_hosts = evidence.get("blocked_solution_hosts")
    if not isinstance(blocked_hosts, list) or "github.com" not in blocked_hosts:
        raise RuntimeError(
            f"fresh demo row {index} sandbox/provider allowlist evidence must record github.com as blocked"
        )
    blocked_host_set = {host.lower() for host in blocked_hosts if isinstance(host, str)}
    allowed_endpoint_hosts: list[str] = []
    for endpoint in allowed_endpoints:
        parsed = urllib.parse.urlparse(endpoint)
        try:
            endpoint_host = (parsed.hostname or "").lower()
        except ValueError:
            endpoint_host = ""
        if parsed.scheme != "https" or not endpoint_host:
            raise RuntimeError(
                f"fresh demo row {index} sandbox/provider allowlist evidence must record https provider endpoints"
            )
        if endpoint_host in blocked_host_set or any(
            endpoint_host.endswith(f".{blocked_host}") for blocked_host in blocked_host_set
        ):
            raise RuntimeError(
                f"fresh demo row {index} sandbox/provider allowlist evidence allows blocked solution host {endpoint_host}"
            )
        if provider_endpoint_host_is_malformed(endpoint_host) or provider_endpoint_host_is_synthetic_or_local(endpoint_host):
            raise RuntimeError(
                f"fresh demo row {index} sandbox/provider allowlist evidence must record real provider endpoints, not synthetic/local endpoint {endpoint_host}"
            )
        allowed_endpoint_hosts.append(endpoint_host)
    sandbox_sha = evidence.get("sandbox_profile_sha256")
    sandbox_runtime = evidence.get("sandbox_runtime")
    has_profile_sha = isinstance(sandbox_sha, str) and re.fullmatch(r"[0-9a-f]{64}", sandbox_sha)
    has_runtime = isinstance(sandbox_runtime, str) and bool(sandbox_runtime)
    if not has_profile_sha and not has_runtime:
        raise RuntimeError(
            f"fresh demo row {index} sandbox/provider allowlist evidence must record durable sandbox runtime or profile hash"
        )
    if has_profile_sha:
        profile_lines = evidence.get("sandbox_profile_lines")
        if not isinstance(profile_lines, list) or not profile_lines or not all(
            isinstance(line, str) for line in profile_lines
        ):
            raise RuntimeError(
                f"fresh demo row {index} sandbox/provider allowlist evidence with sandbox_profile_sha256 must record sandbox_profile_lines"
            )
        profile_hash = hashlib.sha256(("\n".join(profile_lines) + "\n").encode("utf-8")).hexdigest()
        if profile_hash != sandbox_sha:
            raise RuntimeError(
                f"fresh demo row {index} sandbox/provider allowlist evidence sandbox_profile_lines must match sandbox_profile_sha256"
            )
        validate_fresh_sandbox_profile_lines(
            profile_lines,
            allowed_endpoint_hosts=allowed_endpoint_hosts,
            blocked_host_set=blocked_host_set,
            index=index,
        )


def sandbox_profile_active_line(line: str) -> str:
    # macOS sandbox profiles are Scheme-like; `;` comments are common, and
    # some generated/audited profiles also carry shell-style `#` comments.
    # Comments are audit notes, not executable allow/deny rules.
    return line.split(";", 1)[0].split("#", 1)[0].strip()


def validate_fresh_sandbox_profile_lines(
    profile_lines: list[str], *, allowed_endpoint_hosts: list[str], blocked_host_set: set[str], index: int
) -> None:
    active_lines = [line for line in (sandbox_profile_active_line(line) for line in profile_lines) if line]
    active_profile_text = "\n".join(active_lines).lower()
    if "(deny network" not in active_profile_text:
        raise RuntimeError(
            f"fresh demo row {index} sandbox/provider allowlist evidence sandbox_profile_lines must deny network by default"
        )
    for host in allowed_endpoint_hosts:
        if host not in active_profile_text:
            raise RuntimeError(
                f"fresh demo row {index} sandbox/provider allowlist evidence sandbox_profile_lines must name allowed provider endpoint hosts"
            )
    for line in active_lines:
        lowered = line.lower()
        if "allow" in lowered and any(blocked_host in lowered for blocked_host in blocked_host_set):
            raise RuntimeError(
                f"fresh demo row {index} sandbox/provider allowlist evidence sandbox_profile_lines cannot allow blocked solution hosts"
            )


def provider_endpoint_host_is_malformed(host: str) -> bool:
    if any(character.isspace() for character in host):
        return True
    try:
        ipaddress.ip_address(host)
        return False
    except ValueError:
        pass
    host = host.rstrip(".")
    if not host or len(host) > 253:
        return True
    labels = host.split(".")
    return any(
        not label
        or len(label) > 63
        or label.startswith("-")
        or label.endswith("-")
        or not all(character.isascii() and (character.isalnum() or character == "-") for character in label)
        for label in labels
    )


def provider_endpoint_host_is_synthetic_or_local(host: str) -> bool:
    if host == "localhost" or host.endswith(".localhost"):
        return True
    if host in {"example.com", "example.net", "example.org"}:
        return True
    if host.endswith((".example", ".example.com", ".example.net", ".example.org", ".invalid", ".test")):
        return True
    try:
        address = ipaddress.ip_address(host)
    except ValueError:
        return False
    return (
        address.is_loopback
        or address.is_private
        or address.is_link_local
        or address.is_reserved
        or address.is_unspecified
    )


def validate_senior_swe_bench_fresh_provenance(row: dict[str, object], *, index: int) -> None:
    benchmark_source = row.get("benchmark_source")
    has_senior_swe_fields = any(field in row for field in SENIOR_SWE_BENCH_PROVENANCE_FIELDS)
    if benchmark_source != SENIOR_SWE_BENCH_SOURCE:
        if has_senior_swe_fields:
            raise RuntimeError(
                f"fresh demo row {index} records Senior SWE Bench export provenance fields "
                f"without benchmark_source={SENIOR_SWE_BENCH_SOURCE!r}"
            )
        return

    export_sha = row.get("senior_swe_bench_export_sha256")
    if not isinstance(export_sha, str) or len(export_sha) != 64 or not all(
        character in "0123456789abcdef" for character in export_sha.lower()
    ):
        raise RuntimeError(
            f"fresh demo row {index} records benchmark_source={SENIOR_SWE_BENCH_SOURCE!r} "
            "without a 64-character senior_swe_bench_export_sha256"
        )
    export_row_index = row.get("senior_swe_bench_export_row_index")
    if (
        not isinstance(export_row_index, int)
        or isinstance(export_row_index, bool)
        or export_row_index < 1
    ):
        raise RuntimeError(
            f"fresh demo row {index} records benchmark_source={SENIOR_SWE_BENCH_SOURCE!r} "
            "without a positive integer senior_swe_bench_export_row_index"
        )


def validate_fresh_rows(
    rows: list[dict[str, object]],
    *,
    run_id: str,
    max_tokens: int,
    timeout_secs: int,
    allow_dirty_source: bool,
    source_label: str,
) -> None:
    if not rows:
        raise RuntimeError(f"fresh demo results file has no rows: {source_label}")

    mismatched = [
        row.get("run_id") for row in rows if not run_id_matches(row.get("run_id"), run_id)
    ]
    if mismatched:
        raise RuntimeError(
            "fresh demo results contain rows outside the requested run_id "
            f"{run_id!r}: {mismatched[:3]}"
        )

    validate_fresh_rows_for_host_path_markers(rows, source_label=source_label)

    expected_source_head: str | None = None
    expected_source_head_short: str | None = None
    expected_source_branch: str | None = None
    expected_source_dirty: bool | None = None
    for index, row in enumerate(rows, start=1):
        missing = [
            key
            for key in (
                "source_head",
                "source_head_short",
                "source_branch",
                "source_dirty",
                "max_tokens",
                "timeout_secs",
                "no_external_solution_search",
                "network_policy",
                "audited_sandbox_provider_allowlist_enforced",
                "audited_sandbox_provider_allowlist_status",
            )
            if key not in row
        ]
        if missing:
            raise RuntimeError(
                f"fresh demo row {index} is missing audit field(s): {', '.join(missing)}"
            )
        source_head = row.get("source_head")
        source_head_short = row.get("source_head_short")
        source_branch = row.get("source_branch")
        source_dirty = row.get("source_dirty")
        no_external_solution_search = row.get("no_external_solution_search")
        network_policy = row.get("network_policy")
        sandbox_provider_allowlist_enforced = row.get(
            "audited_sandbox_provider_allowlist_enforced"
        )
        sandbox_provider_allowlist_status = row.get(
            "audited_sandbox_provider_allowlist_status"
        )
        if not isinstance(source_head, str) or len(source_head) not in (40, 64):
            raise RuntimeError(
                f"fresh demo row {index} records invalid source_head={source_head!r}"
            )
        if not all(character in "0123456789abcdef" for character in source_head.lower()):
            raise RuntimeError(
                f"fresh demo row {index} records non-hex source_head={source_head!r}"
            )
        if (
            not isinstance(source_head_short, str)
            or not source_head_short
            or not source_head.startswith(source_head_short)
        ):
            raise RuntimeError(
                f"fresh demo row {index} records source_head_short={source_head_short!r} "
                f"that does not prefix source_head"
            )
        if not isinstance(source_branch, str) or not source_branch:
            raise RuntimeError(
                f"fresh demo row {index} records invalid source_branch={source_branch!r}"
            )
        if source_dirty is not True and source_dirty is not False:
            raise RuntimeError(
                f"fresh demo row {index} records non-boolean source_dirty={source_dirty!r}"
            )
        if expected_source_head is None:
            expected_source_head = source_head
            expected_source_head_short = source_head_short
            expected_source_branch = source_branch
            expected_source_dirty = source_dirty
        elif (
            source_head != expected_source_head
            or source_head_short != expected_source_head_short
            or source_branch != expected_source_branch
            or source_dirty is not expected_source_dirty
        ):
            raise RuntimeError(
                f"fresh demo row {index} source metadata differs from earlier rows; "
                "fresh artifacts must come from one source revision and branch"
            )
        if not allow_dirty_source and row.get("source_dirty") is not False:
            raise RuntimeError(
                f"fresh demo row {index} was produced from dirty source: "
                f"source_dirty={row.get('source_dirty')!r}"
            )
        if row.get("max_tokens") != max_tokens:
            raise RuntimeError(
                f"fresh demo row {index} records max_tokens={row.get('max_tokens')!r}; "
                f"expected {max_tokens}"
            )
        if row.get("timeout_secs") != timeout_secs:
            raise RuntimeError(
                f"fresh demo row {index} records timeout_secs={row.get('timeout_secs')!r}; "
                f"expected {timeout_secs}"
            )
        if no_external_solution_search is not True:
            raise RuntimeError(
                f"fresh demo row {index} does not record no_external_solution_search=true; "
                "fresh provider-backed benchmark evidence must audit the no-GitHub solution-search guard"
            )
        if network_policy != "Isolated":
            raise RuntimeError(
                f"fresh demo row {index} records network_policy={network_policy!r}; "
                "fresh provider-backed benchmark evidence must record the fail-closed benchmark agent network policy"
            )
        if (
            sandbox_provider_allowlist_enforced
            is not FRESH_REQUIRED_SANDBOX_PROVIDER_ALLOWLIST_ENFORCED
        ):
            raise RuntimeError(
                "fresh demo row "
                f"{index} records audited_sandbox_provider_allowlist_enforced="
                f"{sandbox_provider_allowlist_enforced!r}; fresh provider-backed benchmark evidence "
                "must record audited sandbox/provider allowlist enforcement"
            )
        if sandbox_provider_allowlist_status != FRESH_REQUIRED_SANDBOX_PROVIDER_ALLOWLIST_STATUS:
            raise RuntimeError(
                "fresh demo row "
                f"{index} records audited_sandbox_provider_allowlist_status="
                f"{sandbox_provider_allowlist_status!r}; fresh provider-backed benchmark evidence "
                f"must record status={FRESH_REQUIRED_SANDBOX_PROVIDER_ALLOWLIST_STATUS!r}"
            )
        validate_fresh_sandbox_provider_allowlist_evidence(row, index=index)
        validate_senior_swe_bench_fresh_provenance(row, index=index)
        if row_has_verifier_gated_promotion(row) and not promotion_artifact_matches_row(row):
            raise RuntimeError(
                f"fresh demo row {index} has verifier-gated promotion without a matching promotion artifact"
            )


def validate_fresh_results(args: argparse.Namespace) -> None:
    validate_fresh_rows(
        load_jsonl(args.results),
        run_id=args.run_id,
        max_tokens=args.max_tokens,
        timeout_secs=args.timeout,
        allow_dirty_source=args.allow_dirty_source,
        source_label=str(args.results),
    )


def fresh_validation_summary(args: argparse.Namespace) -> str:
    dirty_requirement = "source_dirty=false" if not args.allow_dirty_source else "source_dirty may be true"
    return (
        "# would validate fresh results before scoring: "
        "JSONL exists and is non-empty; "
        f"all rows match run_id {args.run_id!r} or numeric suffixed variants; "
        "all rows share one source revision/branch/dirty-state and source_head_short prefixes source_head; "
        "no host-specific path markers are present; "
        "no_external_solution_search=true and network_policy=Isolated are recorded for every row; "
        "Senior SWE Bench rows, when present, include export SHA-256 and row-index provenance; "
        "audited_sandbox_provider_allowlist_enforced=true, "
        "audited_sandbox_provider_allowlist_status='enforced', and durable "
        "audited sandbox/provider allowlist evidence are recorded for every row; "
        f"required provenance fields are present; {dirty_requirement}; "
        f"max_tokens={args.max_tokens}; timeout_secs={args.timeout}"
    )


def fresh_preflight_summary(args: argparse.Namespace) -> str:
    source_check = (
        "source is clean before output creation"
        if not args.allow_dirty_source
        else "dirty source allowed"
    )
    return (
        "# preflight checked local prerequisites: empty results/evidence paths; "
        f"provider binary {provider_binary_name(args.provider)!r} present; "
        f"local provider config present when supported; {source_check}; "
        "benchmark task payloads request network_policy=Isolated; "
        "audited sandbox/provider-allowlist execution is not implemented/enforced yet. "
        "Live provider auth, quota, and model availability are not verified until the fresh run executes."
    )


def fresh_command(args: argparse.Namespace) -> list[str]:
    root = repo_root()
    command = [
        str(root / "bench/self_correction.py"),
        "--fixture",
        args.fixture,
        "--provider",
        args.provider,
        "--runs",
        str(args.runs),
        "--attempts",
        str(args.attempts),
        "--max-tokens",
        str(args.max_tokens),
        "--timeout",
        str(args.timeout),
        "--results",
        str(args.results),
    ]
    if args.run_id:
        command.extend(["--run-id", args.run_id])
    if not args.allow_dirty_source:
        command.append("--require-clean-source")
    if args.keep_workspace:
        command.append("--keep-workspace")
    return command


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def load_evidence_json(path: Path) -> dict[str, object]:
    resolved = repo_path(path)
    try:
        data = json.loads(resolved.read_text(encoding="utf-8"))
    except FileNotFoundError as exc:
        raise RuntimeError(f"demo evidence JSON was not found: {path}") from exc
    except json.JSONDecodeError as exc:
        raise RuntimeError(f"demo evidence JSON is invalid JSON: {path}: {exc}") from exc
    if not isinstance(data, dict):
        raise RuntimeError(f"demo evidence JSON root must be an object: {path}")
    return data


def verify_fresh_evidence_targets_results(evidence_json: Path, results: Path) -> None:
    evidence = load_evidence_json(evidence_json)
    artifact = evidence.get("artifact")
    if not isinstance(artifact, str) or not artifact:
        raise RuntimeError("fresh demo evidence JSON does not record its source artifact")
    if repo_path(Path(artifact)).resolve(strict=False) != repo_path(results).resolve(strict=False):
        raise RuntimeError(
            "fresh demo evidence JSON points at a different artifact than the requested results path"
        )
    artifact_sha256 = evidence.get("artifact_sha256")
    if not isinstance(artifact_sha256, str) or len(artifact_sha256) != 64:
        raise RuntimeError("fresh demo evidence JSON requires a 64-character artifact_sha256")
    actual_sha256 = sha256_file(repo_path(results))
    if artifact_sha256 != actual_sha256:
        raise RuntimeError(
            "fresh demo evidence artifact_sha256 does not match the requested results bytes"
        )


def require_mapping(value: object, *, label: str) -> dict[str, object]:
    if not isinstance(value, dict):
        raise RuntimeError(f"demo evidence contract expected object at {label}")
    return value


def require_sequence(value: object, *, label: str) -> list[object]:
    if not isinstance(value, list):
        raise RuntimeError(f"demo evidence contract expected array at {label}")
    return value


NORMALIZED_EVIDENCE_FIELDS = [
    "run_id",
    "task_id",
    "attempt",
    "resolved",
    "prior_lineage_present",
    "a2_returncode",
    "verify_returncode",
    "verify_command",
    "touched_files",
    "diff_added_lines",
    "diff_removed_lines",
    "lineage_records_before",
    "lineage_records_after",
    "lineage_reconciled_by_core",
    "verifier_failure_evidence_present",
    "verifier_failure_evidence_structured_present",
    "promotion_evidence_present",
    "promotion_structured_present",
    "promotion_verifier_gated",
    "promotion_structured_evidence_present",
    "promotion_lineage_reconciled_by_core",
    "promotion_verify_returncode",
]


def load_jsonl_rows(path: Path) -> list[dict[str, object]]:
    rows: list[dict[str, object]] = []
    try:
        with path.open(encoding="utf-8") as handle:
            for line_number, line in enumerate(handle, start=1):
                if not line.strip():
                    continue
                try:
                    row = json.loads(line)
                except json.JSONDecodeError as exc:
                    raise RuntimeError(
                        f"demo evidence contract artifact is invalid JSONL at line {line_number}: {path}: {exc}"
                    ) from exc
                if not isinstance(row, dict):
                    raise RuntimeError(
                        f"demo evidence contract artifact row {line_number} is not an object"
                    )
                rows.append(row)
    except FileNotFoundError as exc:
        raise RuntimeError(f"demo evidence contract artifact was not found: {path}") from exc
    if not rows:
        raise RuntimeError("demo evidence contract artifact contains no JSONL rows")
    return rows


def optional_int_value(value: object) -> int | None:
    if value is None or isinstance(value, bool):
        return None
    try:
        return int(value)  # type: ignore[arg-type]
    except (TypeError, ValueError):
        return None


def optional_bool_value(value: object) -> bool | None:
    if value is True or value is False:
        return value
    return None


def optional_string_value(value: object) -> str | None:
    if isinstance(value, str) and value:
        return value
    return None


def optional_positive_int_value(value: object) -> int | None:
    if isinstance(value, int) and not isinstance(value, bool) and value > 0:
        return value
    return None


def artifact_promotion(row: dict[str, object]) -> dict[str, object]:
    promotion = row.get("promotion")
    return promotion if isinstance(promotion, dict) else {}


def artifact_has_promotion_evidence(row: dict[str, object]) -> bool:
    promotion = artifact_promotion(row)
    if isinstance(row.get("promotion"), dict):
        return promotion.get("verifier_gated") is True and promotion.get("evidence_present") is True
    if "promotion_evidence_present" in row:
        return row["promotion_evidence_present"] is True
    output = "\n".join(str(row.get(key) or "") for key in ("stdout", "stderr")).lower()
    return "promote_germline" in output or "[applied and rebuilt:" in output


def artifact_promotion_artifact(row: dict[str, object]) -> dict[str, object] | None:
    artifact = artifact_promotion(row).get("artifact")
    return artifact if isinstance(artifact, dict) else None


def promotion_artifact_matches_row(row: dict[str, object]) -> bool:
    artifact = artifact_promotion_artifact(row)
    if artifact is None:
        return False
    selector = artifact.get("selector")
    return (
        artifact.get("kind") == "self_correction_jsonl_row"
        and isinstance(artifact.get("path"), str)
        and bool(str(artifact.get("path")).strip())
        and isinstance(selector, dict)
        and selector.get("run_id") == row.get("run_id")
        and selector.get("task_id") == row.get("task_id")
        and selector.get("attempt") == row.get("attempt")
        and artifact.get("lineage_records_after") == row.get("lineage_records_after")
        and artifact.get("verify_returncode") == row.get("verify_returncode")
        and artifact.get("verify_command") == row.get("verify_command")
    )


def row_has_verifier_gated_promotion(row: dict[str, object]) -> bool:
    return (
        row.get("resolved") is True
        and row.get("verify_returncode") == 0
        and row.get("lineage_reconciled_by_core") is True
        and artifact_has_promotion_evidence(row)
    )


def normalized_artifact_row(row: dict[str, object]) -> dict[str, object]:
    promotion = artifact_promotion(row)
    normalized = {
        "run_id": str(row.get("run_id") or ""),
        "task_id": str(row.get("task_id") or ""),
        "attempt": max(int(row.get("attempt") or 1), 1),
        "resolved": row.get("resolved") is True,
        "prior_lineage_present": row.get("prior_lineage_present") is True,
        "a2_returncode": optional_int_value(row.get("a2_returncode")),
        "verify_returncode": optional_int_value(row.get("verify_returncode")),
        "verify_command": str(row["verify_command"]) if row.get("verify_command") else None,
        "touched_files": [str(path) for path in row.get("touched_files", [])]
        if isinstance(row.get("touched_files"), list)
        else [],
        "diff_added_lines": optional_int_value(row.get("diff_added_lines")),
        "diff_removed_lines": optional_int_value(row.get("diff_removed_lines")),
        "lineage_records_before": optional_int_value(row.get("lineage_records_before")),
        "lineage_records_after": optional_int_value(row.get("lineage_records_after")),
        "lineage_reconciled_by_core": optional_bool_value(row.get("lineage_reconciled_by_core")),
        "verifier_failure_evidence_present": row.get("verifier_failure_evidence_present") is True
        if "verifier_failure_evidence_present" in row
        else None,
        "verifier_failure_evidence_structured_present": "verifier_failure_evidence_present" in row,
        "promotion_evidence_present": artifact_has_promotion_evidence(row),
        "promotion_structured_present": isinstance(row.get("promotion"), dict),
        "promotion_verifier_gated": optional_bool_value(promotion.get("verifier_gated")),
        "promotion_structured_evidence_present": optional_bool_value(promotion.get("evidence_present")),
        "promotion_lineage_reconciled_by_core": optional_bool_value(
            promotion.get("lineage_reconciled_by_core")
        ),
        "promotion_verify_returncode": optional_int_value(promotion.get("verify_returncode")),
    }
    for key in (
        "no_external_solution_search",
        "audited_sandbox_provider_allowlist_enforced",
    ):
        value = optional_bool_value(row.get(key))
        if value is not None:
            normalized[key] = value
    for key in (
        "network_policy",
        "benchmark_source",
        "senior_swe_bench_export_sha256",
        "audited_sandbox_provider_allowlist_status",
    ):
        value = optional_string_value(row.get(key))
        if value is not None:
            normalized[key] = value
    row_index = optional_positive_int_value(row.get("senior_swe_bench_export_row_index"))
    if row_index is not None:
        normalized["senior_swe_bench_export_row_index"] = row_index
    audit_evidence = row.get("audited_sandbox_provider_allowlist_evidence")
    if isinstance(audit_evidence, dict):
        normalized["audited_sandbox_provider_allowlist_evidence"] = audit_evidence
    if "source_head" in row:
        normalized["source_head"] = row.get("source_head")
        normalized["source_head_short"] = row.get("source_head_short")
        normalized["source_branch"] = row.get("source_branch")
        normalized["source_dirty"] = row.get("source_dirty")
    return normalized


def selector_tuple(selector: dict[str, object], *, label: str) -> tuple[str, str, int]:
    run_id = selector.get("run_id")
    task_id = selector.get("task_id")
    attempt = selector.get("attempt")
    if (
        not isinstance(run_id, str)
        or not isinstance(task_id, str)
        or not isinstance(attempt, int)
        or isinstance(attempt, bool)
    ):
        raise RuntimeError(f"demo evidence contract selector lacks run_id/task_id/attempt at {label}")
    return (run_id, task_id, attempt)


def artifact_rows_by_selector(
    rows: list[dict[str, object]],
) -> dict[tuple[str, str, int], dict[str, object]]:
    indexed: dict[tuple[str, str, int], dict[str, object]] = {}
    for row_index, row in enumerate(rows):
        key = selector_tuple(row, label=f"artifact row {row_index}")
        if key in indexed:
            raise RuntimeError(f"demo evidence contract artifact has duplicate row selector: {key}")
        indexed[key] = row
    return indexed


def require_artifact_row(
    index: dict[tuple[str, str, int], dict[str, object]],
    selector: dict[str, object],
    *,
    label: str,
) -> dict[str, object]:
    key = selector_tuple(selector, label=label)
    try:
        return index[key]
    except KeyError as exc:
        raise RuntimeError(f"demo evidence contract selector missing from artifact: {key}") from exc


def strict_int(value: object, *, label: str) -> int:
    if type(value) is int:
        return value
    raise RuntimeError(f"demo evidence contract {label} must be an integer")


def strict_json_equal(left: object, right: object) -> bool:
    return json.dumps(left, sort_keys=True, separators=(",", ":")) == json.dumps(
        right,
        sort_keys=True,
        separators=(",", ":"),
    )


def require_embedded_row_matches_artifact(
    step: dict[str, object],
    artifact_row: dict[str, object],
    *,
    label: str,
) -> dict[str, object]:
    embedded = require_mapping(step.get("evidence_row"), label=f"{label}.evidence_row")
    expected = normalized_artifact_row(artifact_row)
    if not strict_json_equal(embedded, expected):
        raise RuntimeError(f"demo evidence contract embedded row differs from artifact at {label}")
    return embedded


def validate_evidence_source_metadata(
    evidence: dict[str, object],
    rows: list[dict[str, object]],
) -> None:
    source_metadata = evidence.get("source_metadata")
    if source_metadata is None:
        return
    metadata = require_mapping(source_metadata, label="source_metadata")
    source_head = metadata.get("source_head")
    source_head_short = metadata.get("source_head_short")
    source_branch = metadata.get("source_branch")
    source_dirty = metadata.get("source_dirty")
    if not isinstance(source_head, str) or len(source_head) not in (40, 64):
        raise RuntimeError("demo evidence contract source_metadata.source_head is invalid")
    if not all(character in "0123456789abcdef" for character in source_head.lower()):
        raise RuntimeError("demo evidence contract source_metadata.source_head is non-hex")
    if (
        not isinstance(source_head_short, str)
        or not source_head_short
        or not source_head.startswith(source_head_short)
    ):
        raise RuntimeError("demo evidence contract source_metadata.source_head_short does not prefix source_head")
    if not isinstance(source_branch, str) or not source_branch:
        raise RuntimeError("demo evidence contract source_metadata.source_branch is invalid")
    if source_dirty is not True and source_dirty is not False:
        raise RuntimeError("demo evidence contract source_metadata.source_dirty must be boolean")
    for row_index, row in enumerate(rows, start=1):
        for key, expected in metadata.items():
            if row.get(key) != expected:
                raise RuntimeError(
                    f"demo evidence contract source_metadata differs from artifact row {row_index}: {key}"
                )


def require_fresh_evidence_source_head_matches_current(evidence: dict[str, object]) -> None:
    metadata = require_mapping(evidence.get("source_metadata"), label="source_metadata")
    source_head = metadata.get("source_head")
    source_head_short = metadata.get("source_head_short")
    current = current_source_metadata()
    current_head = current["source_head"]
    if source_head != current_head:
        raise RuntimeError(
            "fresh demo evidence source_metadata.source_head differs from current HEAD; "
            "rerun the confirmed fresh provider-backed command from the intended HEAD"
        )
    if not isinstance(source_head_short, str) or not current_head.startswith(source_head_short):
        raise RuntimeError(
            "fresh demo evidence source_metadata.source_head_short does not prefix current HEAD"
        )


def validate_demo_evidence_contract(
    evidence: dict[str, object],
    reference: dict[str, object],
    *,
    evidence_label: str,
    require_git_tracked_artifact: bool = False,
) -> None:
    """Validate a demo evidence JSON against the archived proof contract.

    This is a local artifact-shape check for archived or freshly generated scorer
    evidence. It does not run a provider and does not prove live provider access;
    it ensures a produced evidence JSON preserves the six-step loop proof shape.
    """

    reference_requirements = require_sequence(
        reference.get("requirements"), label="reference.requirements"
    )
    if reference_requirements != EXPECTED_DEMO_REQUIREMENTS:
        raise RuntimeError(
            "demo evidence contract reference does not define the expected six-step proof"
        )
    if evidence.get("requirements") != EXPECTED_DEMO_REQUIREMENTS:
        raise RuntimeError(
            "demo evidence contract requirements differ from the expected six-step proof"
        )
    if evidence.get("complete") is not True:
        raise RuntimeError(f"demo evidence contract is incomplete: {evidence_label}")
    artifact = evidence.get("artifact")
    if not isinstance(artifact, str) or not artifact:
        raise RuntimeError("demo evidence contract requires an artifact path")
    artifact_path = repo_path(Path(artifact))
    if not artifact_path.exists():
        raise RuntimeError(f"demo evidence contract artifact was not found: {artifact}")
    if require_git_tracked_artifact:
        require_git_tracked_path(Path(artifact), label="demo evidence contract artifact")
    artifact_sha256 = evidence.get("artifact_sha256")
    if not isinstance(artifact_sha256, str) or len(artifact_sha256) != 64:
        raise RuntimeError("demo evidence contract requires a 64-character artifact_sha256")
    if artifact_sha256 != sha256_file(artifact_path):
        raise RuntimeError("demo evidence contract artifact_sha256 does not match artifact bytes")
    artifact_rows = load_jsonl_rows(artifact_path)
    validate_evidence_source_metadata(evidence, artifact_rows)
    artifact_index = artifact_rows_by_selector(artifact_rows)
    serialized = json.dumps(evidence, sort_keys=True)
    leaked = [marker for marker in HOST_PATH_MARKERS if marker in serialized]
    if leaked:
        raise RuntimeError(
            "demo evidence contract contains host-specific path marker(s): "
            + ", ".join(leaked)
        )
    demos = require_sequence(evidence.get("demos"), label="demos")
    if not demos:
        raise RuntimeError("demo evidence contract requires at least one demo")
    for demo_index, demo_value in enumerate(demos):
        demo = require_mapping(demo_value, label=f"demos[{demo_index}]")
        chain = require_sequence(
            demo.get("causal_chain"), label=f"demos[{demo_index}].causal_chain"
        )
        requirements = [
            require_mapping(step, label=f"demos[{demo_index}].causal_chain[{step_index}]").get(
                "requirement"
            )
            for step_index, step in enumerate(chain)
        ]
        if requirements != reference_requirements:
            raise RuntimeError(
                f"demo evidence contract causal chain differs in demo {demo_index}"
            )
        for step_index, step_value in enumerate(chain):
            step = require_mapping(
                step_value, label=f"demos[{demo_index}].causal_chain[{step_index}]"
            )
            if step.get("status") != "proved":
                raise RuntimeError(
                    f"demo evidence contract step {step.get('requirement')!r} is not proved"
                )
        failed_step = require_mapping(
            chain[reference_requirements.index("failed_first_attempt")],
            label=f"demos[{demo_index}].failed_first_attempt",
        )
        failed_selector = require_mapping(
            failed_step.get("selector"),
            label=f"demos[{demo_index}].failed_first_attempt.selector",
        )
        run_id, task_id, failed_attempt = selector_tuple(
            failed_selector, label=f"demos[{demo_index}].failed_first_attempt.selector"
        )
        if failed_attempt != 1:
            raise RuntimeError("demo evidence contract first failure must be attempt 1")
        failed_row = require_artifact_row(
            artifact_index,
            failed_selector,
            label=f"demos[{demo_index}].failed_first_attempt.selector",
        )
        failed_embedded_row = require_embedded_row_matches_artifact(
            failed_step,
            failed_row,
            label=f"demos[{demo_index}].failed_first_attempt",
        )
        failed_fields = require_mapping(
            failed_step.get("fields"),
            label=f"demos[{demo_index}].failed_first_attempt.fields",
        )
        if failed_fields.get("resolved") is not False:
            raise RuntimeError("demo evidence contract first attempt is not failed")
        if failed_embedded_row.get("resolved") is not False:
            raise RuntimeError("demo evidence contract first attempt artifact row is not failed")
        failed_verify = strict_int(
            failed_fields.get("verify_returncode"),
            label="failed_first_attempt.fields.verify_returncode",
        )
        if failed_verify == 0:
            raise RuntimeError("demo evidence contract first attempt lacks verifier failure")
        failed_embedded_verify = strict_int(
            failed_embedded_row.get("verify_returncode"),
            label="failed_first_attempt.evidence_row.verify_returncode",
        )
        if failed_embedded_verify != failed_verify:
            raise RuntimeError("demo evidence contract failed verifier status differs from artifact")
        failed_command = failed_fields.get("verify_command")
        if not isinstance(failed_command, str) or not failed_command.strip():
            raise RuntimeError("demo evidence contract first attempt lacks verifier command")
        if failed_embedded_row.get("verify_command") != failed_command:
            raise RuntimeError("demo evidence contract failed verifier command differs from artifact")
        archived_step = require_mapping(
            chain[reference_requirements.index("archived_verifier_failure_evidence")],
            label=f"demos[{demo_index}].archived_verifier_failure_evidence",
        )
        archived_selector = require_mapping(
            archived_step.get("selector"),
            label=f"demos[{demo_index}].archived_verifier_failure_evidence.selector",
        )
        if archived_selector != failed_selector:
            raise RuntimeError("demo evidence contract archived failure selector differs from failed attempt")
        archived_embedded_row = require_embedded_row_matches_artifact(
            archived_step,
            failed_row,
            label=f"demos[{demo_index}].archived_verifier_failure_evidence",
        )
        archived_fields = require_mapping(
            archived_step.get("fields"),
            label=f"demos[{demo_index}].archived_verifier_failure_evidence.fields",
        )
        if archived_fields.get("lineage_advanced") is not True:
            raise RuntimeError("demo evidence contract failure evidence did not advance lineage")
        archived_field_before = strict_int(
            archived_fields.get("lineage_records_before"),
            label="archived_verifier_failure_evidence.fields.lineage_records_before",
        )
        archived_field_after = strict_int(
            archived_fields.get("lineage_records_after"),
            label="archived_verifier_failure_evidence.fields.lineage_records_after",
        )
        archived_before = strict_int(
            archived_embedded_row.get("lineage_records_before"),
            label="archived_verifier_failure_evidence.evidence_row.lineage_records_before",
        )
        archived_after = strict_int(
            archived_embedded_row.get("lineage_records_after"),
            label="archived_verifier_failure_evidence.evidence_row.lineage_records_after",
        )
        if archived_before != archived_field_before:
            raise RuntimeError("demo evidence contract archived lineage start differs from artifact")
        if archived_after != archived_field_after:
            raise RuntimeError("demo evidence contract archived lineage end differs from artifact")
        if archived_after <= archived_before:
            raise RuntimeError("demo evidence contract failure evidence did not advance lineage")
        retry_step = require_mapping(
            chain[reference_requirements.index("retry_context_from_failure_evidence")],
            label=f"demos[{demo_index}].retry_context_from_failure_evidence",
        )
        retry_selectors = require_sequence(
            retry_step.get("selectors"),
            label=f"demos[{demo_index}].retry_context_from_failure_evidence.selectors",
        )
        retry_fields = require_sequence(
            retry_step.get("fields"),
            label=f"demos[{demo_index}].retry_context_from_failure_evidence.fields",
        )
        retry_embedded_rows = require_sequence(
            retry_step.get("evidence_rows"),
            label=f"demos[{demo_index}].retry_context_from_failure_evidence.evidence_rows",
        )
        if not retry_fields or len(retry_fields) != len(retry_selectors):
            raise RuntimeError("demo evidence contract requires paired retry selectors and fields")
        if len(retry_embedded_rows) != len(retry_selectors):
            raise RuntimeError("demo evidence contract requires paired retry selectors and evidence rows")
        failed_lineage_after = strict_int(
            archived_fields.get("lineage_records_after"),
            label="archived_verifier_failure_evidence.fields.lineage_records_after",
        )
        if retry_step.get("archived_failure_selector") != failed_selector:
            raise RuntimeError(
                "demo evidence contract retry summary is not tied to archived failure selector"
            )
        if retry_step.get("archived_failure_artifact_sha256") != artifact_sha256:
            raise RuntimeError(
                "demo evidence contract retry summary is not tied to archived failure artifact hash"
            )
        retry_summary_failed_after = strict_int(
            retry_step.get("failed_lineage_records_after"),
            label="retry_context_from_failure_evidence.failed_lineage_records_after",
        )
        if retry_summary_failed_after != failed_lineage_after:
            raise RuntimeError(
                "demo evidence contract retry summary does not carry failed lineage boundary"
            )
        retry_attempts: set[int] = set()
        for field_index, field_value in enumerate(retry_fields):
            retry_selector = require_mapping(
                retry_selectors[field_index],
                label=f"demos[{demo_index}].retry_context_from_failure_evidence.selectors[{field_index}]",
            )
            if retry_selector.get("run_id") != run_id or retry_selector.get("task_id") != task_id:
                raise RuntimeError("demo evidence contract retry selector differs from failed run/task")
            retry_attempt = strict_int(
                retry_selector.get("attempt"),
                label=f"retry_context_from_failure_evidence.selectors[{field_index}].attempt",
            )
            if retry_attempt <= failed_attempt:
                raise RuntimeError("demo evidence contract retry attempt does not follow failure")
            retry_attempts.add(retry_attempt)
            retry_row = require_artifact_row(
                artifact_index,
                retry_selector,
                label=f"demos[{demo_index}].retry_context_from_failure_evidence.selectors[{field_index}]",
            )
            retry_embedded_row = require_mapping(
                retry_embedded_rows[field_index],
                label=f"demos[{demo_index}].retry_context_from_failure_evidence.evidence_rows[{field_index}]",
            )
            if not strict_json_equal(retry_embedded_row, normalized_artifact_row(retry_row)):
                raise RuntimeError("demo evidence contract embedded retry row differs from artifact")
            field = require_mapping(
                field_value,
                label=f"demos[{demo_index}].retry_context_from_failure_evidence.fields[{field_index}]",
            )
            if field.get("failed_attempt_selector") != failed_selector:
                raise RuntimeError("demo evidence contract retry is not tied to failed selector")
            retry_failed_returncode = strict_int(
                field.get("failed_verify_returncode"),
                label=f"retry_context_from_failure_evidence.fields[{field_index}].failed_verify_returncode",
            )
            if retry_failed_returncode != failed_verify:
                raise RuntimeError("demo evidence contract retry does not carry failed verifier status")
            if field.get("failed_verify_command") != failed_fields.get("verify_command"):
                raise RuntimeError("demo evidence contract retry does not carry failed verifier command")
            retry_failed_after = strict_int(
                field.get("failed_lineage_records_after"),
                label=f"retry_context_from_failure_evidence.fields[{field_index}].failed_lineage_records_after",
            )
            if retry_failed_after != failed_lineage_after:
                raise RuntimeError("demo evidence contract retry does not carry failed lineage boundary")
            lineage_before = strict_int(
                field.get("lineage_records_before"),
                label=f"retry_context_from_failure_evidence.fields[{field_index}].lineage_records_before",
            )
            if lineage_before < failed_lineage_after:
                raise RuntimeError("demo evidence contract retry lineage predates archived failure")
            if retry_embedded_row.get("prior_lineage_present") is not True:
                raise RuntimeError("demo evidence contract retry artifact row lacks prior lineage")
            retry_embedded_before = strict_int(
                retry_embedded_row.get("lineage_records_before"),
                label=f"retry_context_from_failure_evidence.evidence_rows[{field_index}].lineage_records_before",
            )
            if retry_embedded_before != lineage_before:
                raise RuntimeError("demo evidence contract retry lineage differs from artifact")
            if field.get("derived_from_failed_lineage") is not True:
                raise RuntimeError("demo evidence contract retry is not derived from failed lineage")
            if field.get("archived_verifier_failure_evidence") is not True:
                raise RuntimeError("demo evidence contract retry lacks archived failure evidence")
            if field.get("retry_context_links_archived_failure") is not True:
                raise RuntimeError("demo evidence contract retry does not link archived failure")
        later_step = require_mapping(
            chain[reference_requirements.index("later_passing_attempt")],
            label=f"demos[{demo_index}].later_passing_attempt",
        )
        later_selector = require_mapping(
            later_step.get("selector"),
            label=f"demos[{demo_index}].later_passing_attempt.selector",
        )
        if later_selector.get("run_id") != run_id or later_selector.get("task_id") != task_id:
            raise RuntimeError("demo evidence contract later pass selector differs from failed run/task")
        later_attempt = strict_int(
            later_selector.get("attempt"),
            label="later_passing_attempt.selector.attempt",
        )
        if later_attempt not in retry_attempts:
            raise RuntimeError("demo evidence contract later pass is not one of the linked retries")
        later_row = require_artifact_row(
            artifact_index,
            later_selector,
            label=f"demos[{demo_index}].later_passing_attempt.selector",
        )
        later_embedded_row = require_embedded_row_matches_artifact(
            later_step,
            later_row,
            label=f"demos[{demo_index}].later_passing_attempt",
        )
        later_fields = require_mapping(
            later_step.get("fields"),
            label=f"demos[{demo_index}].later_passing_attempt.fields",
        )
        later_verify = strict_int(
            later_fields.get("verify_returncode"),
            label="later_passing_attempt.fields.verify_returncode",
        )
        if later_fields.get("resolved") is not True or later_verify != 0:
            raise RuntimeError("demo evidence contract later attempt is not verifier-passing")
        later_embedded_verify = strict_int(
            later_embedded_row.get("verify_returncode"),
            label="later_passing_attempt.evidence_row.verify_returncode",
        )
        if later_embedded_row.get("resolved") is not True or later_embedded_verify != 0:
            raise RuntimeError("demo evidence contract later artifact row is not verifier-passing")
        lineage_step = require_mapping(
            chain[reference_requirements.index("lineage_trajectory_recorded")],
            label=f"demos[{demo_index}].lineage_trajectory_recorded",
        )
        lineage_fields = require_mapping(
            lineage_step.get("fields"),
            label=f"demos[{demo_index}].lineage_trajectory_recorded.fields",
        )
        before = strict_int(
            lineage_fields.get("lineage_records_before"),
            label="lineage_trajectory_recorded.fields.lineage_records_before",
        )
        after = strict_int(
            lineage_fields.get("lineage_records_after"),
            label="lineage_trajectory_recorded.fields.lineage_records_after",
        )
        if after <= before:
            raise RuntimeError("demo evidence contract lineage trajectory does not advance")
        lineage_rows = require_sequence(
            lineage_step.get("evidence_rows"),
            label=f"demos[{demo_index}].lineage_trajectory_recorded.evidence_rows",
        )
        if not lineage_rows:
            raise RuntimeError("demo evidence contract requires lineage evidence rows")
        lineage_attempts: list[int] = []
        for lineage_index, lineage_value in enumerate(lineage_rows):
            lineage_embedded_row = require_mapping(
                lineage_value,
                label=f"demos[{demo_index}].lineage_trajectory_recorded.evidence_rows[{lineage_index}]",
            )
            lineage_selector = {
                "run_id": lineage_embedded_row.get("run_id"),
                "task_id": lineage_embedded_row.get("task_id"),
                "attempt": lineage_embedded_row.get("attempt"),
            }
            lineage_artifact_row = require_artifact_row(
                artifact_index,
                lineage_selector,
                label=f"demos[{demo_index}].lineage_trajectory_recorded.evidence_rows[{lineage_index}]",
            )
            if not strict_json_equal(lineage_embedded_row, normalized_artifact_row(lineage_artifact_row)):
                raise RuntimeError("demo evidence contract lineage row differs from artifact")
            if lineage_embedded_row.get("run_id") != run_id or lineage_embedded_row.get("task_id") != task_id:
                raise RuntimeError("demo evidence contract lineage row differs from failed run/task")
            lineage_attempt = strict_int(
                lineage_embedded_row.get("attempt"),
                label=f"lineage_trajectory_recorded.evidence_rows[{lineage_index}].attempt",
            )
            lineage_attempts.append(lineage_attempt)
        lineage_field_attempt_values = require_sequence(
            lineage_fields.get("attempts"),
            label=f"demos[{demo_index}].lineage_trajectory_recorded.fields.attempts",
        )
        lineage_field_attempts = [
            strict_int(
                attempt,
                label=f"lineage_trajectory_recorded.fields.attempts[{attempt_index}]",
            )
            for attempt_index, attempt in enumerate(lineage_field_attempt_values)
        ]
        if lineage_attempts != lineage_field_attempts:
            raise RuntimeError("demo evidence contract lineage attempts differ from artifact")
        if failed_attempt not in lineage_attempts or later_attempt not in lineage_attempts:
            raise RuntimeError("demo evidence contract lineage does not span failed attempt and later pass")
        promotion_step = require_mapping(
            chain[reference_requirements.index("verifier_gated_germline_promotion")],
            label=f"demos[{demo_index}].verifier_gated_germline_promotion",
        )
        promotion_selector = require_mapping(
            promotion_step.get("selector"),
            label=f"demos[{demo_index}].verifier_gated_germline_promotion.selector",
        )
        if promotion_selector != later_selector:
            raise RuntimeError("demo evidence contract promotion selector differs from later passing attempt")
        promotion_embedded_row = require_embedded_row_matches_artifact(
            promotion_step,
            later_row,
            label=f"demos[{demo_index}].verifier_gated_germline_promotion",
        )
        promotion_fields = require_mapping(
            promotion_step.get("fields"),
            label=f"demos[{demo_index}].verifier_gated_germline_promotion.fields",
        )
        if promotion_embedded_row.get("lineage_reconciled_by_core") is not True:
            raise RuntimeError("demo evidence contract promotion artifact lacks core lineage reconciliation")
        promotion_verify = strict_int(
            promotion_fields.get("verify_returncode"),
            label="verifier_gated_germline_promotion.fields.verify_returncode",
        )
        promotion_embedded_verify = strict_int(
            promotion_embedded_row.get("verify_returncode"),
            label="verifier_gated_germline_promotion.evidence_row.verify_returncode",
        )
        if promotion_verify != promotion_embedded_verify:
            raise RuntimeError("demo evidence contract promotion verifier status differs from artifact")
        if promotion_verify != 0:
            raise RuntimeError("demo evidence contract promotion is not verifier-passing")
        if promotion_fields.get("lineage_reconciled_by_core") != promotion_embedded_row.get("lineage_reconciled_by_core"):
            raise RuntimeError("demo evidence contract promotion core reconciliation differs from artifact")
        if promotion_fields.get("lineage_reconciled_by_core") is not True:
            raise RuntimeError("demo evidence contract promotion lacks core lineage reconciliation")
        legacy_promotion_evidence = (
            promotion_fields.get("promotion_evidence_present") is True
            and promotion_embedded_row.get("promotion_evidence_present") is True
        )
        structured_promotion_evidence = (
            promotion_fields.get("promotion_verifier_gated") is True
            and promotion_embedded_row.get("promotion_verifier_gated") is True
            and promotion_fields.get("promotion_structured_evidence_present") is True
            and promotion_embedded_row.get("promotion_structured_evidence_present") is True
            and promotion_fields.get("promotion_lineage_reconciled_by_core") is True
            and promotion_embedded_row.get("promotion_lineage_reconciled_by_core") is True
            and promotion_fields.get("promotion_verify_returncode") == 0
            and type(promotion_fields.get("promotion_verify_returncode")) is int
            and promotion_embedded_row.get("promotion_verify_returncode") == 0
            and type(promotion_embedded_row.get("promotion_verify_returncode")) is int
        )
        if not (legacy_promotion_evidence or structured_promotion_evidence):
            raise RuntimeError("demo evidence contract promotion lacks gated apply evidence")


def selector_summary(selector: dict[str, object]) -> str:
    return (
        f"run_id={selector.get('run_id')!r}, "
        f"task_id={selector.get('task_id')!r}, "
        f"attempt={selector.get('attempt')!r}"
    )


def contract_demo_artifact_lines(evidence: dict[str, object]) -> list[str]:
    artifact = evidence.get("artifact")
    if not isinstance(artifact, str) or not artifact:
        artifact = "<missing>"
    lines = [f"  artifact: {artifact}"]
    source_metadata = evidence.get("source_metadata")
    if isinstance(source_metadata, dict):
        lines.append(
            "  source_metadata: "
            f"source_head={source_metadata.get('source_head')!r}, "
            f"source_head_short={source_metadata.get('source_head_short')!r}, "
            f"source_branch={source_metadata.get('source_branch')!r}, "
            f"source_dirty={source_metadata.get('source_dirty')!r}"
        )
    demos = require_sequence(evidence.get("demos"), label="demos")
    for demo_index, demo_value in enumerate(demos, start=1):
        demo = require_mapping(demo_value, label=f"demos[{demo_index - 1}]")
        chain = require_sequence(
            demo.get("causal_chain"), label=f"demos[{demo_index - 1}].causal_chain"
        )
        steps = {
            require_mapping(step, label=f"demos[{demo_index - 1}].step").get("requirement"): require_mapping(
                step, label=f"demos[{demo_index - 1}].step"
            )
            for step in chain
        }
        failed = require_mapping(
            steps["failed_first_attempt"].get("selector"),
            label=f"demos[{demo_index - 1}].failed_first_attempt.selector",
        )
        retry_selectors = [
            require_mapping(selector, label=f"demos[{demo_index - 1}].retry.selector")
            for selector in require_sequence(
                steps["retry_context_from_failure_evidence"].get("selectors"),
                label=f"demos[{demo_index - 1}].retry_context_from_failure_evidence.selectors",
            )
        ]
        retry_step = steps["retry_context_from_failure_evidence"]
        retry_fields = [
            require_mapping(field, label=f"demos[{demo_index - 1}].retry.fields")
            for field in require_sequence(
                retry_step.get("fields"),
                label=f"demos[{demo_index - 1}].retry_context_from_failure_evidence.fields",
            )
        ]
        archived_failure_selector = require_mapping(
            retry_step.get("archived_failure_selector"),
            label=f"demos[{demo_index - 1}].retry_context_from_failure_evidence.archived_failure_selector",
        )
        retry_causal_flags = [
            "attempt "
            f"{selector.get('attempt')}: "
            f"derived_from_failed_lineage={field.get('derived_from_failed_lineage')}, "
            f"archived_verifier_failure_evidence={field.get('archived_verifier_failure_evidence')}, "
            f"retry_context_links_archived_failure={field.get('retry_context_links_archived_failure')}, "
            f"failed_verify_returncode={field.get('failed_verify_returncode')}, "
            f"failed_lineage_records_after={field.get('failed_lineage_records_after')}"
            for selector, field in zip(retry_selectors, retry_fields)
        ]
        later = require_mapping(
            steps["later_passing_attempt"].get("selector"),
            label=f"demos[{demo_index - 1}].later_passing_attempt.selector",
        )
        lineage_fields = require_mapping(
            steps["lineage_trajectory_recorded"].get("fields"),
            label=f"demos[{demo_index - 1}].lineage_trajectory_recorded.fields",
        )
        promotion = require_mapping(
            steps["verifier_gated_germline_promotion"].get("selector"),
            label=f"demos[{demo_index - 1}].verifier_gated_germline_promotion.selector",
        )
        promotion_fields = require_mapping(
            steps["verifier_gated_germline_promotion"].get("fields"),
            label=f"demos[{demo_index - 1}].verifier_gated_germline_promotion.fields",
        )
        lines.extend(
            [
                f"  demo {demo_index}: {failed.get('run_id')} / {failed.get('task_id')}",
                f"    failed_first_attempt: source={artifact}; {selector_summary(failed)}",
                f"    archived_verifier_failure_evidence: source={artifact}; {selector_summary(failed)}; verify_returncode={steps['failed_first_attempt']['fields']['verify_returncode']}; verify_command={steps['failed_first_attempt']['fields']['verify_command']}; lineage={steps['archived_verifier_failure_evidence']['fields']['lineage_records_before']}->{steps['archived_verifier_failure_evidence']['fields']['lineage_records_after']}",
                "    retry_context_from_failure_evidence: source="
                f"{artifact}; archived_failure_selector={selector_summary(archived_failure_selector)}; "
                f"archived_failure_artifact_sha256={retry_step.get('archived_failure_artifact_sha256')}; selectors=["
                + "; ".join(selector_summary(selector) for selector in retry_selectors)
                + "]; causal_flags=["
                + "; ".join(retry_causal_flags)
                + "]",
                f"    later_passing_attempt: source={artifact}; {selector_summary(later)}",
                f"    lineage_trajectory_recorded: source={artifact}; attempts={lineage_fields.get('attempts')}; lineage={lineage_fields.get('lineage_records_before')}->{lineage_fields.get('lineage_records_after')}",
                f"    verifier_gated_germline_promotion: source={artifact}; {selector_summary(promotion)}; verify_returncode={promotion_fields.get('verify_returncode')}; lineage_reconciled_by_core={promotion_fields.get('lineage_reconciled_by_core')}",
            ]
        )
    return lines


def verify_evidence_contract(
    evidence_json: Path,
    reference_evidence_json: Path,
    *,
    fresh_run_id: str | None = None,
    max_tokens: int = 100_000,
    timeout_secs: int = 1800,
    allow_dirty_source: bool = False,
    require_git_tracked_artifacts: bool = False,
    require_current_head: bool = False,
) -> None:
    if require_current_head and fresh_run_id is None:
        raise RuntimeError("--require-current-head is only supported with --fresh-run-id fresh provenance checks")
    current_head_required = fresh_run_id is not None
    if require_git_tracked_artifacts:
        require_git_tracked_path(evidence_json, label="demo evidence JSON")
    evidence = load_evidence_json(evidence_json)
    reference = load_evidence_json(reference_evidence_json)
    validate_demo_evidence_contract(
        evidence,
        reference,
        evidence_label=str(evidence_json),
        require_git_tracked_artifact=require_git_tracked_artifacts,
    )
    if fresh_run_id is not None:
        artifact = evidence.get("artifact")
        if not isinstance(artifact, str) or not artifact:
            raise RuntimeError("demo evidence contract requires an artifact path for fresh provenance")
        validate_fresh_rows(
            load_jsonl(Path(artifact)),
            run_id=fresh_run_id,
            max_tokens=max_tokens,
            timeout_secs=timeout_secs,
            allow_dirty_source=allow_dirty_source,
            source_label=artifact,
        )
        if not isinstance(evidence.get("source_metadata"), dict):
            raise RuntimeError("fresh demo evidence contract requires source_metadata")
        if current_head_required:
            require_fresh_evidence_source_head_matches_current(evidence)
    print("Demo evidence contract check")
    print(f"  evidence: {evidence_json}")
    print(f"  reference: {reference_evidence_json}")
    if fresh_run_id is None:
        print("  mode: archived historical provider evidence; no fresh run-id provenance check requested")
    else:
        print("  mode: fresh artifact provenance check")
    print(
        "  PASS evidence JSON matches archived demo contract "
        f"(requirements={len(evidence['requirements'])}, demos={len(evidence['demos'])})"
    )
    if fresh_run_id is not None:
        print(
            "  PASS fresh artifact provenance "
            f"(run_id={fresh_run_id!r}, max_tokens={max_tokens}, timeout_secs={timeout_secs})"
        )
        if current_head_required:
            source_head = require_mapping(evidence.get("source_metadata"), label="source_metadata").get("source_head")
            print(f"  PASS current-head provenance (source_head={source_head})")
        artifact = evidence.get("artifact")
        print("  archive_review: fresh artifacts are verified but not archived yet")
        print(f"    artifact_jsonl: {artifact}")
        print(f"    evidence_json: {evidence_json}")
        print(
            "    next: review and commit both artifacts, then rerun this contract with "
            "--require-git-tracked-artifacts before treating them as archived demo proof"
        )
    print(
        "  proved: failed_first_attempt -> archived_verifier_failure_evidence -> "
        "retry_context_from_failure_evidence -> later_passing_attempt -> "
        "lineage_trajectory_recorded -> verifier_gated_germline_promotion"
    )
    for line in contract_demo_artifact_lines(evidence):
        print(line)


def run_command(command: list[str], *, print_only: bool) -> int:
    print(f"$ {display_command(command)}")
    if print_only:
        return 0
    return subprocess.run(command, cwd=repo_root(), check=False).returncode


def canonical_verify_archive_command() -> str:
    return f"bench/self_correction_demo.py verify-archive --evidence-json {DEFAULT_ARCHIVE_EVIDENCE}"


def handoff_demo_evidence_section(text: str) -> str:
    heading = "## Reproducible Demo Evidence Map"
    start = text.find(heading)
    if start < 0:
        return ""
    next_heading = text.find("\n## ", start + len(heading))
    return text[start:] if next_heading < 0 else text[start:next_heading]


def todo_demo_evidence_bullet(text: str) -> str:
    lines = text.splitlines()
    for index, line in enumerate(lines):
        if "The demo-path evidence map" not in line:
            continue
        bullet_lines = [line]
        for continuation in lines[index + 1 :]:
            if continuation.startswith("- "):
                break
            bullet_lines.append(continuation)
        return "\n".join(bullet_lines)
    return ""


def require_ordered_demo_chain(block_name: str, block_text: str, missing: list[str]) -> None:
    cursor = -1
    for requirement in EXPECTED_DEMO_REQUIREMENTS:
        position = block_text.find(requirement, cursor + 1)
        if position < 0:
            missing.append(f"{block_name}: ordered loop chain missing or reorders {requirement}")
            return
        cursor = position


def verify_demo_docs_texts(docs: dict[str, str]) -> None:
    common_required = [
        "python3 bench/self_correction_demo.py verify-demo-docs",
        canonical_verify_archive_command(),
        DEFAULT_ARCHIVE.as_posix(),
        DEFAULT_ARCHIVE_EVIDENCE.as_posix(),
    ]
    caveat_required_lower = [
        "fresh provider-backed",
        "not proof",
        "preflight-only",
        "readiness",
        "confirmed fresh run path",
        "fails closed",
        "not_implemented",
        "audited_sandbox_provider_allowlist_status",
        "audited_sandbox_provider_allowlist_evidence",
        "agent_network_boundary_precondition_executed",
        "agent_network_boundary_precondition_status",
        "not_executed_in_preflight",
        "--require-current-head",
    ]
    missing: list[str] = []
    linked_blocks = {
        "docs/HANDOFF.md Reproducible Demo Evidence Map": handoff_demo_evidence_section(
            docs["docs/HANDOFF.md"]
        ),
        "todos/self-correction-loop.md demo-path evidence bullet": todo_demo_evidence_bullet(
            docs["todos/self-correction-loop.md"]
        ),
    }
    for block_name, block_text in linked_blocks.items():
        for phrase in common_required:
            if phrase not in block_text:
                missing.append(f"{block_name}: {phrase}")
        for requirement in EXPECTED_DEMO_REQUIREMENTS:
            if requirement not in block_text:
                missing.append(f"{block_name}: {requirement}")
        require_ordered_demo_chain(block_name, block_text, missing)
        lowered_block = block_text.lower()
        for phrase in caveat_required_lower:
            if phrase not in lowered_block:
                missing.append(f"{block_name}: {phrase}")
    todo_text = docs["todos/self-correction-loop.md"]
    for phrase in [
        "rerunnable archived-proof command",
        "durable artifact",
        "machine-readable causal-chain map",
        "Fresh provider-backed regeneration remains explicitly unchecked/open",
        "Neither preflight/report nor print-only proves",
    ]:
        if phrase not in todo_text:
            missing.append(f"todos/self-correction-loop.md: {phrase}")
    if missing:
        raise RuntimeError("demo documentation audit missing required text: " + "; ".join(missing))


def verify_demo_docs() -> None:
    docs = {
        "docs/HANDOFF.md": (repo_root() / "docs/HANDOFF.md").read_text(encoding="utf-8"),
        "todos/self-correction-loop.md": (repo_root() / "todos/self-correction-loop.md").read_text(
            encoding="utf-8"
        ),
    }
    verify_demo_docs_texts(docs)
    print(
        "PASS demo docs: canonical archived rerun path, evidence artifacts, "
        "six proof steps, and fresh-evidence caveats documented"
    )


def verify_documented_counts(*, update: bool = False) -> None:
    expected_python_counts = {
        "self_correction": unittest_count_for_script("bench/self_correction.py"),
        "scoring": unittest_count_for_script("bench/self_correction_score.py"),
        "demo_wrapper": current_module_self_test_count(),
    }
    rust_count = cargo_rust_test_count()
    if update:
        pending_updates: list[tuple[Path, str, str]] = []
        for path in (
            repo_root() / "docs/HANDOFF.md",
            repo_root() / "todos/self-correction-loop.md",
        ):
            original = path.read_text(encoding="utf-8")
            updated, replacements = replace_documented_counts(
                original,
                rust_count=rust_count,
                python_counts=expected_python_counts,
            )
            if replacements == 0:
                raise RuntimeError(
                    f"{path.relative_to(repo_root())} has no documented count markers to update"
                )
            pending_updates.append((path, original, updated))
        for path, original, updated in pending_updates:
            if updated != original:
                path.write_text(updated, encoding="utf-8")
                print(f"updated documented counts in {path.relative_to(repo_root())}")
    if handoff_current_rust_test_count() != rust_count:
        raise RuntimeError(
            "docs/HANDOFF.md Current Numbers Rust test count does not match "
            f"cargo test -- --list: documented={handoff_current_rust_test_count()} actual={rust_count}"
        )
    for path in (repo_root() / "docs/HANDOFF.md", repo_root() / "todos/self-correction-loop.md"):
        observed = latest_verification_python_test_counts(path)
        if observed != expected_python_counts:
            raise RuntimeError(
                f"{path.relative_to(repo_root())} latest verification Python counts do not match "
                f"self-tests: documented={observed} actual={expected_python_counts}"
            )
    if handoff_current_python_test_counts() != expected_python_counts:
        raise RuntimeError(
            "docs/HANDOFF.md Current Numbers Python counts do not match self-tests: "
            f"documented={handoff_current_python_test_counts()} actual={expected_python_counts}"
        )
    print(f"PASS documented counts: {documented_counts_summary(rust_count, expected_python_counts)}")


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="Run wrapper unit tests instead of invoking demo commands.",
    )
    subparsers = parser.add_subparsers(dest="mode")

    verify = subparsers.add_parser(
        "verify-archive",
        help="Score a durable archived JSONL demo artifact with --require-demo.",
    )
    verify.add_argument("--archive", type=Path, default=DEFAULT_ARCHIVE)
    verify.add_argument(
        "--evidence-json",
        type=Path,
        help=(
            "Path for a machine-readable demo causal-chain evidence map. "
            "The default archive writes the checked-in evidence map when omitted."
        ),
    )
    verify.add_argument("--print-only", action="store_true")

    contract = subparsers.add_parser(
        "verify-evidence-contract",
        help="Validate a demo evidence JSON against the archived six-step proof contract.",
    )
    contract.add_argument(
        "--evidence-json",
        type=Path,
        default=DEFAULT_ARCHIVE_EVIDENCE,
        help="Demo evidence JSON to validate, such as a freshly generated .demo-evidence.json.",
    )
    contract.add_argument(
        "--reference-evidence-json",
        type=Path,
        default=DEFAULT_ARCHIVE_EVIDENCE,
        help="Archived evidence JSON whose requirements define the demo proof contract.",
    )
    contract.add_argument(
        "--fresh-run-id",
        help=(
            "Optional run_id/prefix for a freshly generated artifact. When set, "
            "the referenced JSONL artifact must also pass fresh provenance, "
            "budget, and clean-source checks."
        ),
    )
    contract.add_argument("--max-tokens", type=int, default=100_000)
    contract.add_argument("--timeout", type=int, default=1800)
    contract.add_argument(
        "--allow-dirty-source",
        action="store_true",
        help="Allow source_dirty=true rows when --fresh-run-id is supplied.",
    )
    contract.add_argument(
        "--require-current-head",
        action="store_true",
        help=(
            "Explicitly request the current-HEAD provenance gate. Fresh provenance "
            "checks with --fresh-run-id require this gate by default; archived "
            "historical evidence without --fresh-run-id is not current-HEAD gated."
        ),
    )
    contract.add_argument(
        "--require-git-tracked-artifacts",
        action="store_true",
        help=(
            "Fail unless the evidence JSON and referenced JSONL artifact are tracked by git. "
            "verify-archive enables this automatically for the durable archived demo path."
        ),
    )

    documented_counts = subparsers.add_parser(
        "verify-documented-counts",
        help=(
            "Check documented Rust/Python test counts. This intentionally runs "
            "cargo test -- --list only when invoked directly, not during --self-test."
        ),
    )
    documented_counts.add_argument(
        "--update",
        action="store_true",
        help=(
            "Rewrite the documented Rust/Python test-count markers before checking them. "
            "This still invokes cargo test -- --list and should be run explicitly, never "
            "from cargo/self-test paths."
        ),
    )

    subparsers.add_parser(
        "verify-demo-docs",
        help=(
            "Check that HANDOFF/TODO document the canonical archived demo rerun path, "
            "durable evidence artifacts, six proof steps, and fresh-evidence caveats."
        ),
    )

    preflight_report = subparsers.add_parser(
        "verify-preflight-report",
        help=(
            "Check a no-network fresh preflight report and print whether its source "
            "snapshot matches the current HEAD. This is readiness-only, not loop proof."
        ),
    )
    preflight_report.add_argument("--report-json", type=Path, required=True)
    preflight_report.add_argument(
        "--require-current-head",
        action="store_true",
        help="Fail when the report source_head/source_dirty does not match the current source state.",
    )

    fresh = subparsers.add_parser(
        "fresh",
        help="Regenerate a fresh demo JSONL artifact, then score it with --require-demo.",
    )
    fresh.add_argument("--results", type=Path, required=True)
    fresh.add_argument("--fixture", default=DEFAULT_FIXTURE)
    fresh.add_argument("--provider", default=DEFAULT_PROVIDER)
    fresh.add_argument("--runs", type=int, default=3)
    fresh.add_argument("--attempts", type=int, default=3)
    fresh.add_argument("--max-tokens", type=int, default=100_000)
    fresh.add_argument("--timeout", type=int, default=1800)
    fresh.add_argument(
        "--run-id",
        required=True,
        help="Required stable prefix for rows produced by this fresh demo invocation.",
    )
    fresh.add_argument(
        "--allow-dirty-source",
        action="store_true",
        help="Omit --require-clean-source when regenerating the benchmark artifact.",
    )
    fresh.add_argument("--keep-workspace", action="store_true")
    fresh.add_argument(
        "--evidence-json",
        type=Path,
        help="Optional path for a machine-readable demo causal-chain evidence map.",
    )
    fresh.add_argument(
        "--confirm-provider-run",
        action="store_true",
        help=(
            "Required for a non-preflight, non-print fresh run because it invokes "
            "provider-backed model CLIs and may consume paid quota/time."
        ),
    )
    fresh.add_argument(
        "--preflight-only",
        action="store_true",
        help=(
            "Check local fresh-run prerequisites (empty output paths, provider binary, "
            "local provider config where supported, and clean source unless "
            "--allow-dirty-source) and print the commands without running the "
            "provider-backed benchmark. This does not validate live auth or quota."
        ),
    )
    fresh.add_argument(
        "--preflight-report-json",
        type=Path,
        help=(
            "With --preflight-only, write a machine-readable no-network readiness "
            "report. The report is not loop evidence and does not validate live "
            "provider auth, quota, or model availability."
        ),
    )
    fresh.add_argument(
        "--preflight-boundary-inventory-json",
        type=Path,
        help=(
            "With --preflight-only, optionally run `python3 bench/agent_network_boundary_check.py --json` "
            "and write the source-boundary inventory to this path. This is readiness/gap evidence only, "
            "not runtime sandbox enforcement or loop proof."
        ),
    )
    fresh.add_argument("--print-only", action="store_true")

    defaultable_argv = list(argv)
    if not defaultable_argv or (
        defaultable_argv[0].startswith("-") and defaultable_argv[0] != "--self-test"
    ):
        defaultable_argv.insert(0, "verify-archive")

    args = parser.parse_args(defaultable_argv)
    return args


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    if args.self_test:
        sys.argv = [sys.argv[0]]
        return unittest.main(exit=False).result.wasSuccessful() is False

    if args.mode == "verify-archive":
        evidence_json = args.evidence_json
        if evidence_json is None and args.archive == DEFAULT_ARCHIVE:
            evidence_json = DEFAULT_ARCHIVE_EVIDENCE
        original_evidence_sha256 = None
        if not args.print_only and evidence_json is not None:
            try:
                require_git_tracked_path(evidence_json, label="demo evidence JSON")
                require_git_tracked_path(args.archive, label="demo evidence contract artifact")
                original_evidence_sha256 = require_existing_normalized_evidence_sha256(evidence_json)
            except RuntimeError as exc:
                print(f"error: {exc}", file=sys.stderr)
                return 2
        result = run_command(
            score_command(args.archive, evidence_json), print_only=args.print_only
        )
        if result != 0 or args.print_only or evidence_json is None:
            return result
        try:
            require_checked_in_evidence_unchanged(evidence_json, original_evidence_sha256)
            verify_evidence_contract(
                evidence_json,
                DEFAULT_ARCHIVE_EVIDENCE,
                require_git_tracked_artifacts=True,
            )
            verify_archive_evidence_regeneration(args.archive, evidence_json)
        except RuntimeError as exc:
            print(f"error: {exc}", file=sys.stderr)
            return 2
        return 0

    if args.mode == "verify-evidence-contract":
        try:
            verify_evidence_contract(
                args.evidence_json,
                args.reference_evidence_json,
                fresh_run_id=args.fresh_run_id,
                max_tokens=args.max_tokens,
                timeout_secs=args.timeout,
                allow_dirty_source=args.allow_dirty_source,
                require_git_tracked_artifacts=args.require_git_tracked_artifacts,
                require_current_head=args.require_current_head,
            )
        except RuntimeError as exc:
            print(f"error: {exc}", file=sys.stderr)
            return 2
        return 0

    if args.mode == "verify-documented-counts":
        try:
            verify_documented_counts(update=args.update)
        except RuntimeError as exc:
            print(f"error: {exc}", file=sys.stderr)
            return 2
        return 0

    if args.mode == "verify-demo-docs":
        try:
            verify_demo_docs()
        except RuntimeError as exc:
            print(f"error: {exc}", file=sys.stderr)
            return 2
        return 0

    if args.mode == "verify-preflight-report":
        try:
            verify_fresh_preflight_report(
                args.report_json,
                require_current_head=args.require_current_head,
            )
        except RuntimeError as exc:
            print(f"error: {exc}", file=sys.stderr)
            return 2
        return 0

    if args.mode == "fresh":
        evidence_json = args.evidence_json or default_fresh_evidence_path(args.results)
        if args.preflight_report_json and not args.preflight_only:
            print("error: --preflight-report-json requires --preflight-only", file=sys.stderr)
            return 2
        if args.preflight_boundary_inventory_json and not args.preflight_only:
            print("error: --preflight-boundary-inventory-json requires --preflight-only", file=sys.stderr)
            return 2
        if args.preflight_only:
            try:
                fresh_preflight(args, evidence_json)
                boundary_inventory = None
                if args.preflight_boundary_inventory_json:
                    ensure_preflight_boundary_inventory_path(
                        args.preflight_boundary_inventory_json,
                        results=args.results,
                        evidence_json=evidence_json,
                        preflight_report_json=args.preflight_report_json,
                    )
                    boundary_inventory = run_agent_network_boundary_inventory_json(
                        args.preflight_boundary_inventory_json
                    )
                if args.preflight_report_json:
                    write_fresh_preflight_report(
                        args.preflight_report_json,
                        fresh_preflight_report(
                            args,
                            evidence_json,
                            boundary_inventory=boundary_inventory,
                        ),
                        results=args.results,
                        evidence_json=evidence_json,
                    )
            except RuntimeError as exc:
                print(f"error: {exc}", file=sys.stderr)
                return 2
            print(fresh_preflight_summary(args))
            if args.preflight_boundary_inventory_json:
                print(f"# wrote agent network boundary inventory: {args.preflight_boundary_inventory_json}")
            if args.preflight_report_json:
                print(f"# wrote preflight report: {args.preflight_report_json}")
            run_command(fresh_command(args), print_only=True)
            print(fresh_validation_summary(args))
            run_command(score_command(args.results, evidence_json), print_only=True)
            run_command(fresh_contract_command(args, evidence_json), print_only=True)
            return 0
        if not args.print_only and not args.confirm_provider_run:
            print(
                "error: fresh provider-backed runs require --confirm-provider-run "
                "because they may consume paid quota/time; use --preflight-only or "
                "--print-only for no-provider dry runs",
                file=sys.stderr,
            )
            return 2
        if not args.print_only:
            try:
                ensure_fresh_output_paths_empty(args, evidence_json)
                ensure_agent_network_boundary_precondition_ready()
                ensure_fresh_sandbox_provider_allowlist_ready()
                fresh_provider_preflight_after_output_paths(args)
            except RuntimeError as exc:
                print(f"error: {exc}", file=sys.stderr)
                return 2
        first = run_command(fresh_command(args), print_only=args.print_only)
        if first != 0:
            return first
        if args.print_only:
            print(fresh_validation_summary(args))
        else:
            try:
                validate_fresh_results(args)
            except RuntimeError as exc:
                print(f"error: {exc}", file=sys.stderr)
                return 2
        result = run_command(
            score_command(args.results, evidence_json), print_only=args.print_only
        )
        if result != 0:
            return result
        if args.print_only:
            run_command(fresh_contract_command(args, evidence_json), print_only=True)
            return 0
        try:
            verify_fresh_evidence_targets_results(evidence_json, args.results)
            verify_evidence_contract(
                evidence_json,
                DEFAULT_ARCHIVE_EVIDENCE,
                fresh_run_id=args.run_id,
                max_tokens=args.max_tokens,
                timeout_secs=args.timeout,
                allow_dirty_source=args.allow_dirty_source,
                require_current_head=True,
            )
        except RuntimeError as exc:
            print(f"error: {exc}", file=sys.stderr)
            return 2
        return 0

    raise AssertionError(f"unhandled mode: {args.mode}")


class SelfCorrectionDemoTests(unittest.TestCase):
    def archived_demo_contract_evidence(self) -> dict[str, object]:
        return load_evidence_json(DEFAULT_ARCHIVE_EVIDENCE)

    def evidence_reference(self, evidence: dict[str, object]) -> dict[str, object]:
        return {"requirements": evidence["requirements"]}

    def required_preflight_network_checks(self) -> dict[str, object]:
        return {
            "benchmark_task_network_policy": FRESH_PREFLIGHT_BENCHMARK_NETWORK_POLICY,
            "restricted_network_policy_current_behavior": FRESH_PREFLIGHT_RESTRICTED_NETWORK_BEHAVIOR,
            "audited_sandbox_provider_allowlist_enforced": FRESH_PREFLIGHT_SANDBOX_PROVIDER_ALLOWLIST_ENFORCED,
            "audited_sandbox_provider_allowlist_status": FRESH_PREFLIGHT_SANDBOX_PROVIDER_ALLOWLIST_STATUS,
            "agent_network_boundary_precondition_required": True,
            "agent_network_boundary_precondition_executed": False,
            "agent_network_boundary_precondition_status": FRESH_PREFLIGHT_AGENT_NETWORK_BOUNDARY_PRECONDITION_STATUS,
        }

    def fresh_sandbox_provider_allowlist_evidence(self) -> dict[str, object]:
        return {
            "status": "enforced",
            "enforcement_layer": "test sandbox wrapper around coding-agent provider launch",
            "launch_boundary": "candidate-worktree agent subprocess",
            "benchmark_network_policy": "Isolated",
            "provider_endpoint_allowlist_enforced": True,
            "allowed_provider_endpoints": ["https://api.openai.com"],
            "public_solution_egress_blocked": True,
            "blocked_solution_hosts": ["github.com", "raw.githubusercontent.com"],
            "sandbox_profile_sha256": TEST_SANDBOX_PROFILE_SHA256,
            "sandbox_profile_lines": TEST_SANDBOX_PROFILE_LINES,
        }

    def fresh_audit_fields(self) -> dict[str, object]:
        return {
            "no_external_solution_search": True,
            "network_policy": "Isolated",
            "audited_sandbox_provider_allowlist_enforced": True,
            "audited_sandbox_provider_allowlist_status": "enforced",
            FRESH_REQUIRED_SANDBOX_PROVIDER_ALLOWLIST_EVIDENCE_FIELD: (
                self.fresh_sandbox_provider_allowlist_evidence()
            ),
        }

    def sync_embedded_rows_for_selector(
        self,
        evidence: dict[str, object],
        selector: dict[str, object],
        normalized_row: dict[str, object],
    ) -> None:
        key = selector_tuple(selector, label="test selector")
        for demo in evidence["demos"]:
            for step in demo["causal_chain"]:
                if "selector" in step and selector_tuple(step["selector"], label="test step") == key:
                    step["evidence_row"] = normalized_row
                for row in step.get("evidence_rows", []):
                    row_selector = {
                        "run_id": row.get("run_id"),
                        "task_id": row.get("task_id"),
                        "attempt": row.get("attempt"),
                    }
                    if selector_tuple(row_selector, label="test evidence row") == key:
                        row.update(normalized_row)

    def evidence_with_source_metadata(self) -> tuple[dict[str, object], list[dict[str, object]]]:
        evidence = self.archived_demo_contract_evidence()
        rows = load_jsonl_rows(repo_path(DEFAULT_ARCHIVE))
        metadata = {
            "source_head": "1234567890abcdef1234567890abcdef12345678",
            "source_head_short": "1234567",
            "source_branch": "main",
            "source_dirty": False,
        }
        evidence["source_metadata"] = metadata
        for row in rows:
            row.update(metadata)
            selector = {
                "run_id": row.get("run_id"),
                "task_id": row.get("task_id"),
                "attempt": row.get("attempt"),
            }
            self.sync_embedded_rows_for_selector(
                evidence,
                selector,
                normalized_artifact_row(row),
            )
        return evidence, rows

    def test_normalized_artifact_row_preserves_benchmark_provenance(self) -> None:
        normalized = normalized_artifact_row(
            {
                "run_id": "fresh-demo-1",
                "task_id": "senior-task",
                "attempt": 1,
                "resolved": False,
                "prior_lineage_present": False,
                "no_external_solution_search": True,
                "network_policy": "Isolated",
                "benchmark_source": SENIOR_SWE_BENCH_SOURCE,
                "senior_swe_bench_export_sha256": "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890",
                "senior_swe_bench_export_row_index": 9,
                "audited_sandbox_provider_allowlist_enforced": True,
                "audited_sandbox_provider_allowlist_status": "enforced",
                "audited_sandbox_provider_allowlist_evidence": self.fresh_sandbox_provider_allowlist_evidence(),
                "stdout": "verbose output should stay in source JSONL only",
            }
        )

        self.assertEqual(normalized["no_external_solution_search"], True)
        self.assertEqual(normalized["network_policy"], "Isolated")
        self.assertEqual(normalized["benchmark_source"], SENIOR_SWE_BENCH_SOURCE)
        self.assertEqual(
            normalized["senior_swe_bench_export_sha256"],
            "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890",
        )
        self.assertEqual(normalized["senior_swe_bench_export_row_index"], 9)
        self.assertEqual(normalized["audited_sandbox_provider_allowlist_enforced"], True)
        self.assertEqual(normalized["audited_sandbox_provider_allowlist_status"], "enforced")
        self.assertEqual(
            normalized["audited_sandbox_provider_allowlist_evidence"],
            self.fresh_sandbox_provider_allowlist_evidence(),
        )
        malformed = normalized_artifact_row(
            {
                "run_id": "fresh-demo-1",
                "task_id": "senior-task",
                "attempt": 1,
                "resolved": False,
                "prior_lineage_present": False,
                "no_external_solution_search": "true",
                "network_policy": [],
                "benchmark_source": "",
                "senior_swe_bench_export_sha256": {},
                "senior_swe_bench_export_row_index": "9",
                "audited_sandbox_provider_allowlist_enforced": "true",
                "audited_sandbox_provider_allowlist_status": [],
                "audited_sandbox_provider_allowlist_evidence": "not-a-map",
            }
        )
        for key in (
            "no_external_solution_search",
            "network_policy",
            "benchmark_source",
            "senior_swe_bench_export_sha256",
            "senior_swe_bench_export_row_index",
            "audited_sandbox_provider_allowlist_enforced",
            "audited_sandbox_provider_allowlist_status",
            "audited_sandbox_provider_allowlist_evidence",
        ):
            self.assertNotIn(key, malformed)
        bool_numeric = normalized_artifact_row(
            {
                "run_id": "fresh-demo-1",
                "task_id": "senior-task",
                "attempt": 1,
                "resolved": False,
                "prior_lineage_present": False,
                "verify_returncode": True,
                "lineage_records_before": False,
                "lineage_records_after": True,
            }
        )
        self.assertIsNone(bool_numeric["verify_returncode"])
        self.assertIsNone(bool_numeric["lineage_records_before"])
        self.assertIsNone(bool_numeric["lineage_records_after"])
        stringly_booleans = normalized_artifact_row(
            {
                "run_id": "fresh-demo-1",
                "task_id": "senior-task",
                "attempt": 1,
                "resolved": "true",
                "prior_lineage_present": "true",
                "lineage_reconciled_by_core": "false",
            }
        )
        self.assertFalse(stringly_booleans["resolved"])
        self.assertFalse(stringly_booleans["prior_lineage_present"])
        self.assertIsNone(stringly_booleans["lineage_reconciled_by_core"])
        self.assertNotIn("stdout", normalized)

    def test_default_verify_archive_command_scores_known_artifact(self) -> None:
        command = score_command(DEFAULT_ARCHIVE)

        self.assertIn("--require-demo", command)
        self.assertIn("--trajectories", command)
        self.assertEqual(Path(command[-1]), DEFAULT_ARCHIVE)

    def test_score_command_can_write_demo_evidence_json(self) -> None:
        command = score_command(DEFAULT_ARCHIVE, Path("evidence.json"))

        self.assertIn("--demo-evidence-json", command)
        self.assertLess(command.index("--demo-evidence-json"), command.index(str(DEFAULT_ARCHIVE)))
        self.assertEqual(command[command.index("--demo-evidence-json") + 1], "evidence.json")

    def test_default_fresh_evidence_path_replaces_jsonl_suffix(self) -> None:
        self.assertEqual(
            default_fresh_evidence_path(Path("docs/results/fresh.jsonl")),
            Path("docs/results/fresh.demo-evidence.json"),
        )
        self.assertEqual(
            default_fresh_evidence_path(Path("docs/results/fresh")),
            Path("docs/results/fresh.demo-evidence.json"),
        )

    def test_provider_binary_name_maps_provider_families(self) -> None:
        self.assertEqual(provider_binary_name("opencode/minimax/MiniMax-M3"), "opencode")
        self.assertEqual(provider_binary_name("pi/zai/glm-5.2"), "pi")
        self.assertEqual(provider_binary_name("gemini"), "gemini")

    def test_run_id_matches_exact_and_numeric_suffix_only(self) -> None:
        self.assertTrue(run_id_matches("fresh-demo", "fresh-demo"))
        self.assertTrue(run_id_matches("fresh-demo-1", "fresh-demo"))
        self.assertTrue(run_id_matches("fresh-demo-12", "fresh-demo"))
        self.assertFalse(run_id_matches("fresh-demo-old", "fresh-demo"))
        self.assertFalse(run_id_matches("fresh-demo-abc", "fresh-demo"))
        self.assertFalse(run_id_matches("fresh-demo-", "fresh-demo"))
        self.assertFalse(run_id_matches("fresh-demo-1-extra", "fresh-demo"))
        self.assertFalse(run_id_matches("", "fresh-demo"))
        self.assertFalse(run_id_matches(None, "fresh-demo"))
        self.assertFalse(run_id_matches(123, "fresh-demo"))

    def test_fresh_contract_command_forwards_allow_dirty_source(self) -> None:
        args = argparse.Namespace(
            run_id="fresh-demo",
            max_tokens=100_000,
            timeout=1800,
            allow_dirty_source=True,
        )

        command = fresh_contract_command(args, Path("docs/results/fresh.demo-evidence.json"))

        self.assertIn("--allow-dirty-source", command)
        self.assertIn("--require-current-head", command)

    def test_documented_python_test_counts_match_self_tests(self) -> None:
        expected_counts = {
            "self_correction": unittest_count_for_script("bench/self_correction.py"),
            "scoring": unittest_count_for_script("bench/self_correction_score.py"),
            "demo_wrapper": current_module_self_test_count(),
        }

        self.assertEqual(handoff_current_python_test_counts(), expected_counts)
        self.assertEqual(
            latest_verification_python_test_counts(repo_root() / "docs/HANDOFF.md"),
            expected_counts,
        )
        self.assertEqual(
            latest_verification_python_test_counts(repo_root() / "todos/self-correction-loop.md"),
            expected_counts,
        )

    def demo_docs_fixture(self) -> dict[str, str]:
        return {
            "docs/HANDOFF.md": "\n".join(
                [
                    "## Reproducible Demo Evidence Map",
                    "Fresh provider-backed regeneration is not proof until archived; "
                    "preflight-only is readiness; the confirmed fresh run path fails closed at "
                    "not_implemented until audited_sandbox_provider_allowlist_status=enforced "
                    "with audited_sandbox_provider_allowlist_evidence; "
                    "preflight records agent_network_boundary_precondition_executed=false and "
                    "agent_network_boundary_precondition_status=not_executed_in_preflight; "
                    "fresh provenance uses --require-current-head.",
                    "python3 bench/self_correction_demo.py verify-demo-docs",
                    canonical_verify_archive_command(),
                    DEFAULT_ARCHIVE.as_posix(),
                    DEFAULT_ARCHIVE_EVIDENCE.as_posix(),
                    *EXPECTED_DEMO_REQUIREMENTS,
                    "## Fresh Provider-Backed Demo Status",
                ]
            ),
            "todos/self-correction-loop.md": "\n".join(
                [
                    "- The demo-path evidence map records "
                    "python3 bench/self_correction_demo.py verify-demo-docs; "
                    f"{canonical_verify_archive_command()}; "
                    f"{DEFAULT_ARCHIVE.as_posix()}; "
                    f"{DEFAULT_ARCHIVE_EVIDENCE.as_posix()}; "
                    f"{'; '.join(EXPECTED_DEMO_REQUIREMENTS)}; "
                    "rerunnable archived-proof command; durable artifact; "
                    "machine-readable causal-chain map; "
                    "Fresh provider-backed regeneration remains explicitly unchecked/open; "
                    "Neither preflight/report nor print-only proves; "
                    "preflight-only is readiness; the confirmed fresh run path fails closed at "
                    "not_implemented until audited_sandbox_provider_allowlist_status=enforced "
                    "with audited_sandbox_provider_allowlist_evidence; "
                    "preflight records agent_network_boundary_precondition_executed=false and "
                    "agent_network_boundary_precondition_status=not_executed_in_preflight; "
                    "fresh provenance uses --require-current-head; "
                    "fresh provider-backed regeneration is not proof yet",
                ]
            ),
        }

    def test_verify_demo_docs_texts_accepts_canonical_rerun_and_all_six_steps(self) -> None:
        verify_demo_docs_texts(self.demo_docs_fixture())

    def test_verify_demo_docs_texts_stops_handoff_section_at_next_h2(self) -> None:
        docs = self.demo_docs_fixture()
        docs["docs/HANDOFF.md"] = docs["docs/HANDOFF.md"].replace(
            "retry_context_from_failure_evidence",
            "",
            1,
        ).replace(
            "## Fresh Provider-Backed Demo Status",
            "## Unrelated Later Section\nretry_context_from_failure_evidence\n## Fresh Provider-Backed Demo Status",
        )

        with self.assertRaisesRegex(RuntimeError, "retry_context_from_failure_evidence"):
            verify_demo_docs_texts(docs)

    def test_verify_demo_docs_texts_accepts_wrapped_todo_evidence_bullet(self) -> None:
        docs = self.demo_docs_fixture()
        docs["todos/self-correction-loop.md"] = docs["todos/self-correction-loop.md"].replace(
            "; durable artifact; machine-readable causal-chain map; ",
            "; durable artifact;\n  machine-readable causal-chain map; ",
        )

        verify_demo_docs_texts(docs)

    def test_verify_demo_docs_texts_rejects_misordered_handoff_loop_chain(self) -> None:
        docs = self.demo_docs_fixture()
        docs["docs/HANDOFF.md"] = docs["docs/HANDOFF.md"].replace(
            "\n".join(EXPECTED_DEMO_REQUIREMENTS),
            "\n".join(
                [
                    "failed_first_attempt",
                    "verifier_gated_germline_promotion",
                    "archived_verifier_failure_evidence",
                    "retry_context_from_failure_evidence",
                    "later_passing_attempt",
                    "lineage_trajectory_recorded",
                ]
            ),
        )

        with self.assertRaisesRegex(RuntimeError, "ordered loop chain"):
            verify_demo_docs_texts(docs)

    def test_verify_demo_docs_texts_rejects_missing_retry_context_step(self) -> None:
        docs = self.demo_docs_fixture()
        docs["docs/HANDOFF.md"] = docs["docs/HANDOFF.md"].replace(
            "retry_context_from_failure_evidence",
            "",
        )

        with self.assertRaisesRegex(RuntimeError, "retry_context_from_failure_evidence"):
            verify_demo_docs_texts(docs)

    def test_verify_demo_docs_texts_rejects_missing_verifier_gated_promotion_step(self) -> None:
        docs = self.demo_docs_fixture()
        docs["docs/HANDOFF.md"] = docs["docs/HANDOFF.md"].replace(
            "verifier_gated_germline_promotion",
            "",
        )

        with self.assertRaisesRegex(RuntimeError, "verifier_gated_germline_promotion"):
            verify_demo_docs_texts(docs)

    def test_verify_demo_docs_texts_rejects_missing_handoff_evidence_path(self) -> None:
        docs = self.demo_docs_fixture()
        docs["docs/HANDOFF.md"] = docs["docs/HANDOFF.md"].replace(
            DEFAULT_ARCHIVE_EVIDENCE.as_posix(),
            "missing.demo-evidence.json",
        )

        with self.assertRaisesRegex(RuntimeError, re.escape(DEFAULT_ARCHIVE_EVIDENCE.as_posix())):
            verify_demo_docs_texts(docs)

    def test_verify_demo_docs_texts_rejects_missing_handoff_fresh_caveat(self) -> None:
        docs = self.demo_docs_fixture()
        docs["docs/HANDOFF.md"] = docs["docs/HANDOFF.md"].replace(
            "Fresh provider-backed regeneration is not proof until archived; "
            "preflight-only is readiness; the confirmed fresh run path fails closed at "
            "not_implemented until audited_sandbox_provider_allowlist_status=enforced "
            "with audited_sandbox_provider_allowlist_evidence; "
            "preflight records agent_network_boundary_precondition_executed=false and "
            "agent_network_boundary_precondition_status=not_executed_in_preflight; "
            "fresh provenance uses --require-current-head.",
            "Archived regeneration caveat is documented elsewhere.",
        )

        with self.assertRaisesRegex(RuntimeError, "fresh provider-backed"):
            verify_demo_docs_texts(docs)

    def test_verify_demo_docs_texts_rejects_missing_handoff_allowlist_evidence_caveat(self) -> None:
        docs = self.demo_docs_fixture()
        docs["docs/HANDOFF.md"] = docs["docs/HANDOFF.md"].replace(
            "with audited_sandbox_provider_allowlist_evidence; ",
            "",
        )

        with self.assertRaisesRegex(RuntimeError, "audited_sandbox_provider_allowlist_evidence"):
            verify_demo_docs_texts(docs)

    def test_verify_demo_docs_texts_rejects_missing_handoff_preflight_not_executed_fields(self) -> None:
        docs = self.demo_docs_fixture()
        docs["docs/HANDOFF.md"] = docs["docs/HANDOFF.md"].replace(
            "preflight records agent_network_boundary_precondition_executed=false and "
            "agent_network_boundary_precondition_status=not_executed_in_preflight; ",
            "preflight records the boundary command; ",
        )

        with self.assertRaisesRegex(RuntimeError, "agent_network_boundary_precondition_executed"):
            verify_demo_docs_texts(docs)

    def test_verify_demo_docs_texts_rejects_missing_handoff_fail_closed_caveat(self) -> None:
        docs = self.demo_docs_fixture()
        docs["docs/HANDOFF.md"] = docs["docs/HANDOFF.md"].replace(
            "the confirmed fresh run path fails closed at not_implemented until ",
            "confirmed fresh runs are available after ",
        )

        with self.assertRaisesRegex(RuntimeError, "fails closed"):
            verify_demo_docs_texts(docs)

    def test_verify_demo_docs_texts_rejects_missing_todo_evidence_path(self) -> None:
        docs = self.demo_docs_fixture()
        docs["todos/self-correction-loop.md"] = docs["todos/self-correction-loop.md"].replace(
            DEFAULT_ARCHIVE_EVIDENCE.as_posix(),
            "missing.demo-evidence.json",
        )

        with self.assertRaisesRegex(RuntimeError, re.escape(DEFAULT_ARCHIVE_EVIDENCE.as_posix())):
            verify_demo_docs_texts(docs)

    def test_verify_demo_docs_texts_rejects_missing_todo_preflight_not_executed_fields(self) -> None:
        docs = self.demo_docs_fixture()
        docs["todos/self-correction-loop.md"] = docs["todos/self-correction-loop.md"].replace(
            "preflight records agent_network_boundary_precondition_executed=false and "
            "agent_network_boundary_precondition_status=not_executed_in_preflight; ",
            "preflight records the boundary command; ",
        )

        with self.assertRaisesRegex(RuntimeError, "agent_network_boundary_precondition_executed"):
            verify_demo_docs_texts(docs)

    def test_verify_demo_docs_texts_rejects_missing_todo_step_even_if_elsewhere(self) -> None:
        docs = self.demo_docs_fixture()
        docs["todos/self-correction-loop.md"] = docs["todos/self-correction-loop.md"].replace(
            "retry_context_from_failure_evidence",
            "",
        ) + "\n- Unrelated note: retry_context_from_failure_evidence"

        with self.assertRaisesRegex(RuntimeError, "retry_context_from_failure_evidence"):
            verify_demo_docs_texts(docs)

    def test_verify_demo_docs_texts_rejects_missing_todo_retry_context_step(self) -> None:
        docs = self.demo_docs_fixture()
        docs["todos/self-correction-loop.md"] = docs["todos/self-correction-loop.md"].replace(
            "retry_context_from_failure_evidence",
            "",
        )

        with self.assertRaisesRegex(RuntimeError, "retry_context_from_failure_evidence"):
            verify_demo_docs_texts(docs)

    def test_verify_demo_docs_texts_rejects_missing_todo_verifier_gated_promotion_step(self) -> None:
        docs = self.demo_docs_fixture()
        docs["todos/self-correction-loop.md"] = docs["todos/self-correction-loop.md"].replace(
            "verifier_gated_germline_promotion",
            "",
        )

        with self.assertRaisesRegex(RuntimeError, "verifier_gated_germline_promotion"):
            verify_demo_docs_texts(docs)

    def test_verify_demo_docs_cli_reports_success(self) -> None:
        stdout = io.StringIO()
        with contextlib.redirect_stdout(stdout):
            result = main(["verify-demo-docs"])

        self.assertEqual(result, 0)
        self.assertIn("PASS demo docs", stdout.getvalue())

    def test_generate_tasks_self_test_covers_senior_swe_bench_policy_payloads(self) -> None:
        result = subprocess.run(
            [sys.executable, str(repo_root() / "bench/generate_tasks.py"), "--self-test"],
            cwd=repo_root(),
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=True,
        )

        self.assertIn("PASS generate_tasks self-test", result.stdout)

    def test_replace_documented_counts_updates_all_count_markers(self) -> None:
        original = "\n".join(
            [
                "| Tests | 1 Rust + 2 self-correction Python + 3 scoring Python + 4 demo-wrapper Python tests |",
                "Latest: `python3 bench/self_correction_demo.py --self-test` ran 4 tests OK; "
                "`python3 bench/self_correction_score.py --self-test` ran 3 tests OK; "
                "`python3 bench/self_correction.py --self-test` ran 2 tests OK; "
                "`python3 bench/self_correction_demo.py verify-documented-counts` passed with "
                "`1 Rust + 2 self-correction Python + 3 scoring Python + 4 demo-wrapper Python tests`.",
            ]
        )

        updated, replacements = replace_documented_counts(
            original,
            rust_count=117,
            python_counts={"self_correction": 26, "scoring": 35, "demo_wrapper": 78},
        )

        self.assertEqual(replacements, 5)
        self.assertIn(
            "117 Rust + 26 self-correction Python + 35 scoring Python + 78 demo-wrapper Python tests",
            updated,
        )
        self.assertIn("`python3 bench/self_correction_demo.py --self-test` ran 78 tests OK", updated)
        self.assertIn("`python3 bench/self_correction_score.py --self-test` ran 35 tests OK", updated)
        self.assertIn("`python3 bench/self_correction.py --self-test` ran 26 tests OK", updated)

    def test_verify_documented_counts_rejects_stale_docs_without_update(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)
            handoff = root / "docs/HANDOFF.md"
            todo = root / "todos/self-correction-loop.md"
            handoff.parent.mkdir(parents=True)
            todo.parent.mkdir(parents=True)
            stale = (
                "| Tests | 1 Rust + 2 self-correction Python + 3 scoring Python + 4 demo-wrapper Python tests |\n"
                "Latest: `python3 bench/self_correction_demo.py --self-test` ran 4 tests OK; "
                "`python3 bench/self_correction_score.py --self-test` ran 3 tests OK; "
                "`python3 bench/self_correction.py --self-test` ran 2 tests OK.\n"
            )
            handoff.write_text(stale, encoding="utf-8")
            todo.write_text(stale, encoding="utf-8")

            with mock.patch(__name__ + ".repo_root", return_value=root), mock.patch(
                __name__ + ".cargo_rust_test_count", return_value=117
            ), mock.patch(
                __name__ + ".unittest_count_for_script", side_effect=[26, 35]
            ), mock.patch(
                __name__ + ".current_module_self_test_count", return_value=78
            ):
                with self.assertRaisesRegex(RuntimeError, "Rust test count does not match"):
                    verify_documented_counts(update=False)

    def test_verify_documented_counts_update_mode_rewrites_docs_before_checking(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)
            handoff = root / "docs/HANDOFF.md"
            todo = root / "todos/self-correction-loop.md"
            handoff.parent.mkdir(parents=True)
            todo.parent.mkdir(parents=True)
            stale = (
                "| Tests | 1 Rust + 2 self-correction Python + 3 scoring Python + 4 demo-wrapper Python tests |\n"
                "Latest: `python3 bench/self_correction_demo.py --self-test` ran 4 tests OK; "
                "`python3 bench/self_correction_score.py --self-test` ran 3 tests OK; "
                "`python3 bench/self_correction.py --self-test` ran 2 tests OK.\n"
            )
            handoff.write_text(stale, encoding="utf-8")
            todo.write_text(stale, encoding="utf-8")

            with mock.patch(__name__ + ".repo_root", return_value=root), mock.patch(
                __name__ + ".cargo_rust_test_count", return_value=117
            ), mock.patch(
                __name__ + ".unittest_count_for_script", side_effect=[26, 35]
            ), mock.patch(
                __name__ + ".current_module_self_test_count", return_value=78
            ), contextlib.redirect_stdout(io.StringIO()):
                verify_documented_counts(update=True)

            self.assertIn(
                "117 Rust + 26 self-correction Python + 35 scoring Python + 78 demo-wrapper Python tests",
                handoff.read_text(encoding="utf-8"),
            )
            self.assertEqual(latest_verification_python_test_counts(todo)["demo_wrapper"], 78)

    def test_rust_test_count_parser_counts_only_cargo_test_lines(self) -> None:
        cargo_list_output = "\n".join(
            [
                "a2_eval::sentinel::tests::suite_reports_score_fraction: test",
                "a2_eval::sentinel::tests::demo_wrapper_self_test_passes_under_cargo_test_without_mutating_archive: test",
                "a2_eval::sentinel::benches::ignored_bench: benchmark",
                "Doc-tests a2_eval",
                "",
            ]
        )

        self.assertEqual(rust_test_count_from_cargo_test_list_output(cargo_list_output), 2)

    def test_opencode_provider_config_requires_configured_provider(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            auth = Path(tmpdir) / "auth.json"
            auth.write_text(
                json.dumps({"minimax-coding-plan": {"type": "api", "key": "redacted"}}),
                encoding="utf-8",
            )

            ensure_opencode_provider_config(
                "opencode/minimax-coding-plan/MiniMax-M3",
                auth_path=auth,
            )
            with self.assertRaises(RuntimeError):
                ensure_opencode_provider_config(
                    "opencode/missing-plan/model",
                    auth_path=auth,
                )

    def test_fresh_evidence_path_refuses_non_empty_file(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            evidence = Path(tmpdir) / "fresh.demo-evidence.json"
            evidence.write_text('{"old": true}\n', encoding="utf-8")

            with self.assertRaises(RuntimeError):
                ensure_fresh_evidence_path(evidence)

    def test_no_args_defaults_to_verify_archive_mode(self) -> None:
        args = parse_args([])

        self.assertEqual(args.mode, "verify-archive")
        self.assertEqual(args.archive, DEFAULT_ARCHIVE)
        self.assertIsNone(args.evidence_json)

    def test_verify_documented_counts_update_flag_parses(self) -> None:
        args = parse_args(["verify-documented-counts", "--update"])

        self.assertEqual(args.mode, "verify-documented-counts")
        self.assertTrue(args.update)

    def test_verify_preflight_report_cli_parses(self) -> None:
        args = parse_args(
            [
                "verify-preflight-report",
                "--report-json",
                "fresh.report.json",
                "--require-current-head",
            ]
        )

        self.assertEqual(args.mode, "verify-preflight-report")
        self.assertEqual(args.report_json, Path("fresh.report.json"))
        self.assertTrue(args.require_current_head)

    def test_verify_preflight_report_cli_dispatches_require_current_head(self) -> None:
        with mock.patch(__name__ + ".verify_fresh_preflight_report") as verify:
            result = main(
                [
                    "verify-preflight-report",
                    "--report-json",
                    "fresh.report.json",
                    "--require-current-head",
                ]
            )

        self.assertEqual(result, 0)
        verify.assert_called_once_with(Path("fresh.report.json"), require_current_head=True)

    def test_archive_flags_work_without_explicit_subcommand(self) -> None:
        args = parse_args(["--archive", "custom.jsonl", "--print-only"])

        self.assertEqual(args.mode, "verify-archive")
        self.assertEqual(args.archive, Path("custom.jsonl"))
        self.assertIsNone(args.evidence_json)
        self.assertTrue(args.print_only)

    def test_default_verify_archive_print_only_includes_checked_in_evidence_json(self) -> None:
        stdout = io.StringIO()
        with contextlib.redirect_stdout(stdout):
            result = main(["verify-archive", "--print-only"])

        output = stdout.getvalue()
        self.assertEqual(result, 0)
        self.assertIn("--demo-evidence-json", output)
        self.assertIn(str(DEFAULT_ARCHIVE_EVIDENCE), output)
        self.assertIn(str(DEFAULT_ARCHIVE), output)

    def test_default_verify_archive_runs_checked_in_six_step_contract_after_score(self) -> None:
        stdout = io.StringIO()
        with mock.patch(__name__ + ".run_command", return_value=0) as run, mock.patch(
            __name__ + ".verify_archive_evidence_regeneration"
        ) as regeneration, contextlib.redirect_stdout(stdout):
            result = main(["verify-archive"])

        self.assertEqual(result, 0)
        run.assert_called_once_with(
            score_command(DEFAULT_ARCHIVE, DEFAULT_ARCHIVE_EVIDENCE),
            print_only=False,
        )
        regeneration.assert_called_once_with(DEFAULT_ARCHIVE, DEFAULT_ARCHIVE_EVIDENCE)
        output = stdout.getvalue()
        self.assertIn(f"evidence: {DEFAULT_ARCHIVE_EVIDENCE}", output)
        self.assertIn(f"reference: {DEFAULT_ARCHIVE_EVIDENCE}", output)
        self.assertIn(f"artifact: {DEFAULT_ARCHIVE}", output)
        self.assertIn(
            "proved: failed_first_attempt -> archived_verifier_failure_evidence -> "
            "retry_context_from_failure_evidence -> later_passing_attempt -> "
            "lineage_trajectory_recorded -> verifier_gated_germline_promotion",
            output,
        )
        self.assertIn("failed_first_attempt: source=", output)
        self.assertIn("archived_verifier_failure_evidence: source=", output)
        self.assertIn("retry_context_from_failure_evidence: source=", output)
        self.assertIn("later_passing_attempt: source=", output)
        self.assertIn("lineage_trajectory_recorded: source=", output)
        self.assertIn("verifier_gated_germline_promotion: source=", output)

    def test_verify_archive_runs_evidence_contract_after_successful_score(self) -> None:
        with mock.patch(__name__ + ".require_git_tracked_path") as tracked, mock.patch(
            __name__ + ".require_existing_normalized_evidence_sha256", return_value="a" * 64
        ), mock.patch(__name__ + ".require_checked_in_evidence_unchanged") as unchanged, mock.patch(
            __name__ + ".run_command", return_value=0
        ) as run, mock.patch(__name__ + ".verify_evidence_contract") as contract, mock.patch(
            __name__ + ".verify_archive_evidence_regeneration"
        ) as regeneration:
            result = main(
                [
                    "verify-archive",
                    "--archive",
                    "custom.jsonl",
                    "--evidence-json",
                    "custom.demo-evidence.json",
                ]
            )

        self.assertEqual(result, 0)
        self.assertEqual(tracked.call_count, 2)
        tracked.assert_any_call(Path("custom.demo-evidence.json"), label="demo evidence JSON")
        tracked.assert_any_call(Path("custom.jsonl"), label="demo evidence contract artifact")
        run.assert_called_once()
        unchanged.assert_called_once_with(Path("custom.demo-evidence.json"), "a" * 64)
        contract.assert_called_once_with(
            Path("custom.demo-evidence.json"),
            DEFAULT_ARCHIVE_EVIDENCE,
            require_git_tracked_artifacts=True,
        )
        regeneration.assert_called_once_with(Path("custom.jsonl"), Path("custom.demo-evidence.json"))

    def test_verify_archive_clean_room_regeneration_matches_checked_in_evidence(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            expected = Path(tmpdir) / "expected.demo-evidence.json"
            expected.write_text(
                json.dumps(
                    {
                        "artifact": "docs/benchmark-results/self-correction/demo.jsonl",
                        "complete": True,
                        "generated_at": "old timestamp ignored",
                        "demos": [{"requirement": "lineage_trajectory_recorded"}],
                    },
                    sort_keys=True,
                ),
                encoding="utf-8",
            )

            def fake_run_command(command: list[str], *, print_only: bool = False) -> int:
                self.assertFalse(print_only)
                output_path = Path(command[command.index("--demo-evidence-json") + 1])
                self.assertFalse(output_path.exists())
                output_path.write_text(
                    json.dumps(
                        {
                            "artifact": "docs/benchmark-results/self-correction/demo.jsonl",
                            "complete": True,
                            "generated_at": "new timestamp ignored",
                            "demos": [{"requirement": "lineage_trajectory_recorded"}],
                        },
                        sort_keys=True,
                    ),
                    encoding="utf-8",
                )
                return 0

            stdout = io.StringIO()
            with mock.patch(__name__ + ".run_command", side_effect=fake_run_command), contextlib.redirect_stdout(stdout):
                verify_archive_evidence_regeneration(Path("demo.jsonl"), expected)

        self.assertIn("PASS clean-room evidence regeneration", stdout.getvalue())

    def test_verify_archive_clean_room_regeneration_detects_mismatch(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            expected = Path(tmpdir) / "expected.demo-evidence.json"
            expected.write_text(json.dumps({"complete": True}), encoding="utf-8")

            def fake_run_command(command: list[str], *, print_only: bool = False) -> int:
                output_path = Path(command[command.index("--demo-evidence-json") + 1])
                output_path.write_text(json.dumps({"complete": False}), encoding="utf-8")
                return 0

            with mock.patch(__name__ + ".run_command", side_effect=fake_run_command):
                with self.assertRaisesRegex(RuntimeError, "regeneration produced different"):
                    verify_archive_evidence_regeneration(Path("demo.jsonl"), expected)

    def test_verify_archive_rejects_in_place_checked_in_evidence_mutation(self) -> None:
        stderr = io.StringIO()
        with tempfile.TemporaryDirectory() as tmpdir:
            evidence = Path(tmpdir) / "custom.demo-evidence.json"
            evidence.write_text(json.dumps({"complete": True}), encoding="utf-8")

            def fake_run_command(command: list[str], *, print_only: bool = False) -> int:
                evidence.write_text(json.dumps({"complete": False}), encoding="utf-8")
                return 0

            with mock.patch(__name__ + ".require_git_tracked_path"), mock.patch(
                __name__ + ".run_command", side_effect=fake_run_command
            ), mock.patch(__name__ + ".verify_evidence_contract") as contract, mock.patch(
                __name__ + ".verify_archive_evidence_regeneration"
            ) as regeneration, contextlib.redirect_stderr(stderr):
                result = main(
                    [
                        "verify-archive",
                        "--archive",
                        "custom.jsonl",
                        "--evidence-json",
                        str(evidence),
                    ]
                )

        self.assertEqual(result, 2)
        self.assertIn("changed the normalized checked-in demo evidence", stderr.getvalue())
        contract.assert_not_called()
        regeneration.assert_not_called()

    def test_verify_archive_rejects_missing_checked_in_evidence_before_scoring(self) -> None:
        stderr = io.StringIO()
        with tempfile.TemporaryDirectory() as tmpdir:
            missing_evidence = Path(tmpdir) / "missing.demo-evidence.json"
            with mock.patch(__name__ + ".require_git_tracked_path"), mock.patch(
                __name__ + ".run_command"
            ) as run, contextlib.redirect_stderr(stderr):
                result = main(
                    [
                        "verify-archive",
                        "--archive",
                        "custom.jsonl",
                        "--evidence-json",
                        str(missing_evidence),
                    ]
                )

        self.assertEqual(result, 2)
        self.assertIn("must exist and be non-empty before verify-archive scoring", stderr.getvalue())
        run.assert_not_called()

    def test_verify_archive_rejects_invalid_checked_in_evidence_before_scoring(self) -> None:
        stderr = io.StringIO()
        with tempfile.TemporaryDirectory() as tmpdir:
            invalid_evidence = Path(tmpdir) / "invalid.demo-evidence.json"
            invalid_evidence.write_text("{not json", encoding="utf-8")
            with mock.patch(__name__ + ".require_git_tracked_path"), mock.patch(
                __name__ + ".run_command"
            ) as run, contextlib.redirect_stderr(stderr):
                result = main(
                    [
                        "verify-archive",
                        "--archive",
                        "custom.jsonl",
                        "--evidence-json",
                        str(invalid_evidence),
                    ]
                )

        self.assertEqual(result, 2)
        self.assertIn("demo evidence JSON is invalid JSON", stderr.getvalue())
        run.assert_not_called()

    def test_verify_archive_rejects_untracked_paths_before_scoring(self) -> None:
        stderr = io.StringIO()
        with mock.patch(
            __name__ + ".require_git_tracked_path",
            side_effect=RuntimeError("demo evidence JSON is not git-tracked"),
        ), mock.patch(__name__ + ".run_command") as run, contextlib.redirect_stderr(stderr):
            result = main(
                [
                    "verify-archive",
                    "--archive",
                    "custom.jsonl",
                    "--evidence-json",
                    "custom.demo-evidence.json",
                ]
            )

        self.assertEqual(result, 2)
        self.assertIn("not git-tracked", stderr.getvalue())
        run.assert_not_called()

    def test_verify_evidence_contract_cli_forwards_tracked_artifact_requirement(self) -> None:
        with mock.patch(__name__ + ".verify_evidence_contract") as contract:
            result = main(
                [
                    "verify-evidence-contract",
                    "--evidence-json",
                    "custom.demo-evidence.json",
                    "--reference-evidence-json",
                    str(DEFAULT_ARCHIVE_EVIDENCE),
                    "--require-git-tracked-artifacts",
                ]
            )

        self.assertEqual(result, 0)
        contract.assert_called_once_with(
            Path("custom.demo-evidence.json"),
            DEFAULT_ARCHIVE_EVIDENCE,
            fresh_run_id=None,
            max_tokens=100_000,
            timeout_secs=1800,
            allow_dirty_source=False,
            require_git_tracked_artifacts=True,
            require_current_head=False,
        )

    def test_require_git_tracked_path_normalizes_absolute_repo_paths(self) -> None:
        with mock.patch(__name__ + ".git_output", return_value=DEFAULT_ARCHIVE.as_posix()) as git:
            require_git_tracked_path(
                repo_root() / DEFAULT_ARCHIVE,
                label="demo evidence contract artifact",
            )

        git.assert_called_once_with(["ls-files", "--", DEFAULT_ARCHIVE.as_posix()])

    def test_verify_evidence_contract_rejects_untracked_artifact_when_required(self) -> None:
        evidence = self.archived_demo_contract_evidence()
        with mock.patch(__name__ + ".git_output", return_value=""):
            with self.assertRaisesRegex(RuntimeError, "artifact is not git-tracked"):
                validate_demo_evidence_contract(
                    evidence,
                    self.evidence_reference(evidence),
                    evidence_label="untracked-artifact.demo-evidence.json",
                    require_git_tracked_artifact=True,
                )

    def test_verify_evidence_contract_rejects_untracked_evidence_json_when_required(self) -> None:
        with mock.patch(__name__ + ".git_output", return_value=""):
            with self.assertRaisesRegex(RuntimeError, "demo evidence JSON is not git-tracked"):
                verify_evidence_contract(
                    DEFAULT_ARCHIVE_EVIDENCE,
                    DEFAULT_ARCHIVE_EVIDENCE,
                    require_git_tracked_artifacts=True,
                )

    def test_verify_archive_skips_evidence_contract_when_scoring_fails(self) -> None:
        with mock.patch(__name__ + ".require_git_tracked_path"), mock.patch(
            __name__ + ".require_existing_normalized_evidence_sha256", return_value="a" * 64
        ), mock.patch(__name__ + ".run_command", return_value=1), mock.patch(
            __name__ + ".verify_evidence_contract"
        ) as contract, mock.patch(__name__ + ".verify_archive_evidence_regeneration") as regeneration:
            result = main(
                [
                    "verify-archive",
                    "--archive",
                    "custom.jsonl",
                    "--evidence-json",
                    "custom.demo-evidence.json",
                ]
            )

        self.assertEqual(result, 1)
        contract.assert_not_called()
        regeneration.assert_not_called()

    def test_fresh_refuses_provider_run_without_explicit_confirmation(self) -> None:
        stdout = io.StringIO()
        stderr = io.StringIO()
        with mock.patch(__name__ + ".fresh_preflight") as preflight, mock.patch(
            __name__ + ".run_command"
        ) as run, contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
            result = main(
                [
                    "fresh",
                    "--results",
                    "docs/benchmark-results/self-correction/a2-fresh-demo.jsonl",
                    "--run-id",
                    "fresh-demo",
                ]
            )

        self.assertEqual(result, 2)
        self.assertEqual(stdout.getvalue(), "")
        self.assertIn("--confirm-provider-run", stderr.getvalue())
        preflight.assert_not_called()
        run.assert_not_called()

    def test_fresh_refuses_confirmed_provider_run_when_agent_boundary_precondition_fails(self) -> None:
        stderr = io.StringIO()
        with tempfile.TemporaryDirectory() as tmpdir:
            results = Path(tmpdir) / "fresh.jsonl"
            evidence = Path(tmpdir) / "fresh.demo-evidence.json"
            failed = subprocess.CompletedProcess(
                AGENT_NETWORK_BOUNDARY_PRECONDITION_COMMAND,
                1,
                stdout="",
                stderr="sandbox runtime missing",
            )
            with mock.patch(__name__ + ".subprocess.run", return_value=failed) as run_precondition, mock.patch(
                __name__ + ".ensure_fresh_sandbox_provider_allowlist_ready"
            ) as sandbox_ready, mock.patch(
                __name__ + ".fresh_provider_preflight_after_output_paths"
            ) as provider_preflight, mock.patch(
                __name__ + ".run_command"
            ) as run, contextlib.redirect_stderr(stderr):
                result = main(
                    [
                        "fresh",
                        "--results",
                        str(results),
                        "--evidence-json",
                        str(evidence),
                        "--run-id",
                        "fresh-demo",
                        "--confirm-provider-run",
                    ]
                )

        self.assertEqual(result, 2)
        self.assertIn("agent network boundary precondition failed closed", stderr.getvalue())
        self.assertIn("sandbox runtime missing", stderr.getvalue())
        self.assertFalse(results.exists())
        self.assertFalse(evidence.exists())
        run_precondition.assert_called_once()
        sandbox_ready.assert_not_called()
        provider_preflight.assert_not_called()
        run.assert_not_called()

    def test_fresh_refuses_confirmed_provider_run_without_enforced_sandbox_allowlist_after_boundary_passes(self) -> None:
        stderr = io.StringIO()
        with tempfile.TemporaryDirectory() as tmpdir:
            results = Path(tmpdir) / "fresh.jsonl"
            evidence = Path(tmpdir) / "fresh.demo-evidence.json"
            with mock.patch(__name__ + ".ensure_agent_network_boundary_precondition_ready") as boundary_ready, mock.patch(
                __name__ + ".fresh_provider_preflight_after_output_paths"
            ) as provider_preflight, mock.patch(
                __name__ + ".run_command"
            ) as run, contextlib.redirect_stderr(stderr):
                result = main(
                    [
                        "fresh",
                        "--results",
                        str(results),
                        "--evidence-json",
                        str(evidence),
                        "--run-id",
                        "fresh-demo",
                        "--confirm-provider-run",
                    ]
                )

        self.assertEqual(result, 2)
        self.assertIn("no audited sandbox/provider allowlist is enforced", stderr.getvalue())
        self.assertFalse(results.exists())
        self.assertFalse(evidence.exists())
        boundary_ready.assert_called_once_with()
        provider_preflight.assert_not_called()
        run.assert_not_called()

    def test_fresh_runs_evidence_contract_after_confirmed_successful_score(self) -> None:
        with mock.patch(__name__ + ".ensure_agent_network_boundary_precondition_ready"), mock.patch(
            __name__ + ".ensure_fresh_sandbox_provider_allowlist_ready"
        ), mock.patch(
            __name__ + ".fresh_provider_preflight_after_output_paths"
        ), mock.patch(__name__ + ".run_command", side_effect=[0, 0]) as run, mock.patch(
            __name__ + ".validate_fresh_results"
        ), mock.patch(__name__ + ".verify_fresh_evidence_targets_results") as target_guard, mock.patch(
            __name__ + ".verify_evidence_contract"
        ) as contract:
            result = main(
                [
                    "fresh",
                    "--results",
                    "docs/benchmark-results/self-correction/a2-fresh-demo.jsonl",
                    "--run-id",
                    "fresh-demo",
                    "--confirm-provider-run",
                ]
            )

        self.assertEqual(result, 0)
        self.assertEqual(run.call_count, 2)
        target_guard.assert_called_once_with(
            Path("docs/benchmark-results/self-correction/a2-fresh-demo.demo-evidence.json"),
            Path("docs/benchmark-results/self-correction/a2-fresh-demo.jsonl"),
        )
        contract.assert_called_once_with(
            Path("docs/benchmark-results/self-correction/a2-fresh-demo.demo-evidence.json"),
            DEFAULT_ARCHIVE_EVIDENCE,
            fresh_run_id="fresh-demo",
            max_tokens=100_000,
            timeout_secs=1800,
            allow_dirty_source=False,
            require_current_head=True,
        )

    def test_fresh_rejects_post_score_results_mutation_before_contract(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            results = Path(tmpdir) / "fresh.jsonl"
            evidence = Path(tmpdir) / "fresh.demo-evidence.json"
            fresh_row = {
                "run_id": "fresh-demo",
                "source_head": "1234567890abcdef1234567890abcdef12345678",
                "source_head_short": "1234567",
                "source_branch": "main",
                "source_dirty": False,
                "max_tokens": 100_000,
                "timeout_secs": 1800,
                "no_external_solution_search": True,
                "network_policy": "Isolated",
                "audited_sandbox_provider_allowlist_enforced": True,
                "audited_sandbox_provider_allowlist_status": "enforced",
                FRESH_REQUIRED_SANDBOX_PROVIDER_ALLOWLIST_EVIDENCE_FIELD: (
                    self.fresh_sandbox_provider_allowlist_evidence()
                ),
            }
            calls = 0

            def fake_run_command(command: list[str], *, print_only: bool = False) -> int:
                nonlocal calls
                calls += 1
                self.assertFalse(print_only)
                if calls == 1:
                    results.write_text(json.dumps(fresh_row) + "\n", encoding="utf-8")
                elif calls == 2:
                    evidence.write_text(
                        json.dumps(
                            {
                                "artifact": str(results),
                                "artifact_sha256": sha256_file(results),
                            }
                        ),
                        encoding="utf-8",
                    )
                    with results.open("a", encoding="utf-8") as handle:
                        handle.write(json.dumps({**fresh_row, "attempt": 2}) + "\n")
                else:
                    self.fail(f"unexpected run_command call: {command}")
                return 0

            stderr = io.StringIO()
            with mock.patch(__name__ + ".ensure_agent_network_boundary_precondition_ready"), mock.patch(
                __name__ + ".ensure_fresh_sandbox_provider_allowlist_ready"
            ), mock.patch(
                __name__ + ".fresh_provider_preflight_after_output_paths"
            ), mock.patch(__name__ + ".run_command", side_effect=fake_run_command), mock.patch(
                __name__ + ".verify_evidence_contract"
            ) as contract, contextlib.redirect_stderr(stderr):
                result = main(
                    [
                        "fresh",
                        "--results",
                        str(results),
                        "--evidence-json",
                        str(evidence),
                        "--run-id",
                        "fresh-demo",
                        "--confirm-provider-run",
                    ]
                )

            self.assertEqual(result, 2)
            self.assertIn("artifact_sha256 does not match", stderr.getvalue())
            contract.assert_not_called()

    def test_verify_fresh_evidence_targets_results_accepts_matching_artifact(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            results = Path(tmpdir) / "fresh.jsonl"
            results.write_text('{"run_id":"fresh-demo-1"}\n', encoding="utf-8")
            evidence = Path(tmpdir) / "fresh.demo-evidence.json"
            evidence.write_text(
                json.dumps(
                    {
                        "artifact": str(results),
                        "artifact_sha256": sha256_file(results),
                    }
                ),
                encoding="utf-8",
            )

            verify_fresh_evidence_targets_results(evidence, results)

    def test_verify_fresh_evidence_targets_results_rejects_mismatched_artifact(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            results = Path(tmpdir) / "fresh.jsonl"
            other_results = Path(tmpdir) / "other.jsonl"
            results.write_text('{"run_id":"fresh-demo-1"}\n', encoding="utf-8")
            other_results.write_text('{"run_id":"fresh-demo-1"}\n', encoding="utf-8")
            evidence = Path(tmpdir) / "fresh.demo-evidence.json"
            evidence.write_text(
                json.dumps(
                    {
                        "artifact": str(other_results),
                        "artifact_sha256": sha256_file(other_results),
                    }
                ),
                encoding="utf-8",
            )

            with self.assertRaisesRegex(RuntimeError, "different artifact"):
                verify_fresh_evidence_targets_results(evidence, results)

    def test_verify_fresh_evidence_targets_results_rejects_hash_mismatch(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            results = Path(tmpdir) / "fresh.jsonl"
            results.write_text('{"run_id":"fresh-demo-1"}\n', encoding="utf-8")
            evidence = Path(tmpdir) / "fresh.demo-evidence.json"
            evidence.write_text(
                json.dumps(
                    {
                        "artifact": str(results),
                        "artifact_sha256": "0" * 64,
                    }
                ),
                encoding="utf-8",
            )

            with self.assertRaisesRegex(RuntimeError, "artifact_sha256 does not match"):
                verify_fresh_evidence_targets_results(evidence, results)

    def test_verify_evidence_contract_fresh_run_id_rejects_stale_archive_rows(self) -> None:
        with self.assertRaisesRegex(RuntimeError, "outside the requested run_id"):
            verify_evidence_contract(
                DEFAULT_ARCHIVE_EVIDENCE,
                DEFAULT_ARCHIVE_EVIDENCE,
                fresh_run_id="fresh-demo",
            )

    def test_verify_evidence_contract_cli_rejects_stale_archive_for_fresh_run_id(self) -> None:
        stdout = io.StringIO()
        stderr = io.StringIO()

        with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
            result = main(
                [
                    "verify-evidence-contract",
                    "--evidence-json",
                    str(DEFAULT_ARCHIVE_EVIDENCE),
                    "--fresh-run-id",
                    "fresh-demo",
                ]
            )

        self.assertEqual(result, 2)
        self.assertEqual(stdout.getvalue(), "")
        self.assertIn("outside the requested run_id", stderr.getvalue())

    def test_verify_evidence_contract_archived_mode_omits_fresh_archive_review(self) -> None:
        stdout = io.StringIO()
        with contextlib.redirect_stdout(stdout):
            verify_evidence_contract(DEFAULT_ARCHIVE_EVIDENCE, DEFAULT_ARCHIVE_EVIDENCE)

        output = stdout.getvalue()
        self.assertIn("mode: archived historical provider evidence", output)
        self.assertNotIn("archive_review:", output)
        self.assertNotIn("--require-git-tracked-artifacts", output)

    def test_verify_evidence_contract_prints_fresh_provenance_mode_when_checked(self) -> None:
        stdout = io.StringIO()
        evidence, rows = self.evidence_with_source_metadata()

        with mock.patch(__name__ + ".load_evidence_json", side_effect=[evidence, self.evidence_reference(evidence)]), mock.patch(
            __name__ + ".load_jsonl_rows", return_value=rows
        ), mock.patch(__name__ + ".load_jsonl", return_value=rows) as load_rows, mock.patch(
            __name__ + ".validate_fresh_rows"
        ) as validate_rows, mock.patch(
            __name__ + ".current_source_metadata", return_value=evidence["source_metadata"]
        ), contextlib.redirect_stdout(stdout):
            verify_evidence_contract(
                DEFAULT_ARCHIVE_EVIDENCE,
                DEFAULT_ARCHIVE_EVIDENCE,
                fresh_run_id="fresh-demo",
                max_tokens=123,
                timeout_secs=456,
            )

        load_rows.assert_called_once_with(DEFAULT_ARCHIVE)
        validate_rows.assert_called_once()
        self.assertEqual(validate_rows.call_args.kwargs["run_id"], "fresh-demo")
        self.assertEqual(validate_rows.call_args.kwargs["max_tokens"], 123)
        self.assertEqual(validate_rows.call_args.kwargs["timeout_secs"], 456)
        output = stdout.getvalue()
        self.assertIn("mode: fresh artifact provenance check", output)
        self.assertIn("PASS fresh artifact provenance", output)
        self.assertIn("PASS current-head provenance", output)
        self.assertIn("archive_review: fresh artifacts are verified but not archived yet", output)
        self.assertIn(f"artifact_jsonl: {DEFAULT_ARCHIVE}", output)
        self.assertIn(f"evidence_json: {DEFAULT_ARCHIVE_EVIDENCE}", output)
        self.assertIn("--require-git-tracked-artifacts", output)
        self.assertIn("run_id='fresh-demo'", output)
        self.assertIn("source_metadata:", output)

    def test_verify_evidence_contract_requires_fresh_run_id_for_current_head_gate(self) -> None:
        with self.assertRaisesRegex(RuntimeError, "only supported with --fresh-run-id"):
            verify_evidence_contract(
                DEFAULT_ARCHIVE_EVIDENCE,
                DEFAULT_ARCHIVE_EVIDENCE,
                require_current_head=True,
            )

    def test_verify_evidence_contract_fresh_current_head_gate_rejects_stale_source_by_default(self) -> None:
        evidence = {
            "artifact": "docs/benchmark-results/self-correction/fresh.jsonl",
            "source_metadata": {
                "source_head": "1234567890abcdef1234567890abcdef12345678",
                "source_head_short": "1234567",
                "source_branch": "main",
                "source_dirty": False,
            },
        }
        current = {
            "source_head": "abcdef1234567890abcdef1234567890abcdef12",
            "source_head_short": "abcdef1",
            "source_branch": "main",
            "source_dirty": False,
        }
        with mock.patch(__name__ + ".load_evidence_json", side_effect=[evidence, {"requirements": EXPECTED_DEMO_REQUIREMENTS}]), mock.patch(
            __name__ + ".validate_demo_evidence_contract"
        ), mock.patch(__name__ + ".load_jsonl", return_value=[]), mock.patch(
            __name__ + ".validate_fresh_rows"
        ), mock.patch(__name__ + ".current_source_metadata", return_value=current):
            with self.assertRaisesRegex(RuntimeError, "differs from current HEAD"):
                verify_evidence_contract(
                    DEFAULT_ARCHIVE_EVIDENCE,
                    DEFAULT_ARCHIVE_EVIDENCE,
                    fresh_run_id="fresh-demo",
                )

    def test_verify_evidence_contract_fresh_rejects_evidence_rows_missing_sandbox_audit_snapshot(self) -> None:
        evidence, rows = self.evidence_with_source_metadata()
        for row in rows:
            row.update(self.fresh_audit_fields())
        with mock.patch(__name__ + ".load_evidence_json", side_effect=[evidence, self.evidence_reference(evidence)]), mock.patch(
            __name__ + ".load_jsonl_rows", return_value=rows
        ):
            with self.assertRaisesRegex(RuntimeError, "embedded row differs from artifact"):
                verify_evidence_contract(
                    DEFAULT_ARCHIVE_EVIDENCE,
                    DEFAULT_ARCHIVE_EVIDENCE,
                    fresh_run_id="fresh-demo",
                )

    def test_verify_evidence_contract_fresh_current_head_gate_prints_pass(self) -> None:
        source_head = "1234567890abcdef1234567890abcdef12345678"
        evidence = {
            "artifact": "docs/benchmark-results/self-correction/fresh.jsonl",
            "requirements": EXPECTED_DEMO_REQUIREMENTS,
            "demos": [],
            "source_metadata": {
                "source_head": source_head,
                "source_head_short": "1234567",
                "source_branch": "main",
                "source_dirty": False,
            },
        }
        current = {
            "source_head": source_head,
            "source_head_short": "1234567",
            "source_branch": "main",
            "source_dirty": False,
        }
        stdout = io.StringIO()
        with mock.patch(__name__ + ".load_evidence_json", side_effect=[evidence, {"requirements": EXPECTED_DEMO_REQUIREMENTS}]), mock.patch(
            __name__ + ".validate_demo_evidence_contract"
        ), mock.patch(__name__ + ".load_jsonl", return_value=[]), mock.patch(
            __name__ + ".validate_fresh_rows"
        ), mock.patch(__name__ + ".current_source_metadata", return_value=current), contextlib.redirect_stdout(stdout):
            verify_evidence_contract(
                DEFAULT_ARCHIVE_EVIDENCE,
                DEFAULT_ARCHIVE_EVIDENCE,
                fresh_run_id="fresh-demo",
                require_current_head=True,
            )

        self.assertIn("PASS current-head provenance", stdout.getvalue())

    def test_verify_evidence_contract_fresh_provenance_requires_source_metadata(self) -> None:
        evidence = self.archived_demo_contract_evidence()
        fresh_rows = [
            {
                "run_id": "fresh-demo",
                "source_head": "1234567890abcdef1234567890abcdef12345678",
                "source_head_short": "1234567",
                "source_branch": "main",
                "source_dirty": False,
                "max_tokens": 100_000,
                "timeout_secs": 1800,
                "no_external_solution_search": True,
                "network_policy": "Isolated",
                "audited_sandbox_provider_allowlist_enforced": True,
                "audited_sandbox_provider_allowlist_status": "enforced",
                FRESH_REQUIRED_SANDBOX_PROVIDER_ALLOWLIST_EVIDENCE_FIELD: (
                    self.fresh_sandbox_provider_allowlist_evidence()
                ),
            }
        ]

        with mock.patch(__name__ + ".load_evidence_json", side_effect=[evidence, self.evidence_reference(evidence)]), mock.patch(
            __name__ + ".load_jsonl", return_value=fresh_rows
        ):
            with self.assertRaisesRegex(RuntimeError, "requires source_metadata"):
                verify_evidence_contract(
                    DEFAULT_ARCHIVE_EVIDENCE,
                    DEFAULT_ARCHIVE_EVIDENCE,
                    fresh_run_id="fresh-demo",
                )

    def test_verify_evidence_contract_accepts_complete_six_step_demo(self) -> None:
        evidence = self.archived_demo_contract_evidence()

        validate_demo_evidence_contract(
            evidence,
            self.evidence_reference(evidence),
            evidence_label=str(DEFAULT_ARCHIVE_EVIDENCE),
        )

    def test_verify_evidence_contract_accepts_source_metadata_matching_rows(self) -> None:
        evidence, rows = self.evidence_with_source_metadata()

        with mock.patch(__name__ + ".load_jsonl_rows", return_value=rows):
            validate_demo_evidence_contract(
                evidence,
                self.evidence_reference(evidence),
                evidence_label="source-metadata.demo-evidence.json",
            )

    def test_verify_evidence_contract_rejects_source_metadata_mismatched_rows(self) -> None:
        evidence, rows = self.evidence_with_source_metadata()
        rows[0]["source_head"] = "abcdef1234567890abcdef1234567890abcdef12"

        with mock.patch(__name__ + ".load_jsonl_rows", return_value=rows):
            with self.assertRaisesRegex(RuntimeError, "source_metadata differs from artifact row"):
                validate_demo_evidence_contract(
                    evidence,
                    self.evidence_reference(evidence),
                    evidence_label="source-metadata-mismatch.demo-evidence.json",
                )

    def test_verify_evidence_contract_accepts_row_level_source_metadata_without_top_level_summary(self) -> None:
        evidence, rows = self.evidence_with_source_metadata()
        evidence.pop("source_metadata")

        with mock.patch(__name__ + ".load_jsonl_rows", return_value=rows):
            validate_demo_evidence_contract(
                evidence,
                self.evidence_reference(evidence),
                evidence_label="row-level-source-metadata.demo-evidence.json",
            )

    def test_verify_evidence_contract_prints_concrete_artifact_selectors(self) -> None:
        stdout = io.StringIO()

        with contextlib.redirect_stdout(stdout):
            verify_evidence_contract(DEFAULT_ARCHIVE_EVIDENCE, DEFAULT_ARCHIVE_EVIDENCE)

        output = stdout.getvalue()
        self.assertIn(str(DEFAULT_ARCHIVE), output)
        self.assertIn("mode: archived historical provider evidence", output)
        self.assertIn("no fresh run-id provenance check requested", output)
        self.assertIn("failed_first_attempt: source=", output)
        self.assertIn("archived_verifier_failure_evidence: source=", output)
        self.assertIn("verify_command=cargo test -p a2_archive", output)
        self.assertIn("retry_context_from_failure_evidence: source=", output)
        self.assertIn("archived_failure_selector=run_id='self-correction-20260615T165316Z'", output)
        self.assertIn("archived_failure_artifact_sha256=33a83345adac350b9a79bdd7842ac0c0cad1b698f7fc636a8a12f0c32fe7cee3", output)
        self.assertIn("derived_from_failed_lineage=True", output)
        self.assertIn("archived_verifier_failure_evidence=True", output)
        self.assertIn("retry_context_links_archived_failure=True", output)
        self.assertIn("failed_verify_returncode=1", output)
        self.assertIn("failed_lineage_records_after=1", output)
        self.assertIn("later_passing_attempt: source=", output)
        self.assertIn("lineage_trajectory_recorded: source=", output)
        self.assertIn("attempts=[1, 2]", output)
        self.assertIn("verifier_gated_germline_promotion: source=", output)
        self.assertIn("verify_returncode=0", output)
        self.assertIn("lineage_reconciled_by_core=True", output)

    def test_verify_evidence_contract_rejects_bool_numeric_embedded_row_mismatch(self) -> None:
        evidence = self.archived_demo_contract_evidence()
        rows = load_jsonl_rows(repo_path(DEFAULT_ARCHIVE))
        failed_selector = evidence["demos"][0]["causal_chain"][0]["selector"]
        failed_row = require_artifact_row(
            artifact_rows_by_selector(rows),
            failed_selector,
            label="failed first attempt",
        )
        failed_row["verify_returncode"] = True
        failed_row["lineage_records_before"] = False
        failed_row["lineage_records_after"] = True

        with mock.patch(__name__ + ".load_jsonl_rows", return_value=rows):
            with self.assertRaisesRegex(RuntimeError, "embedded row differs from artifact"):
                validate_demo_evidence_contract(
                    evidence,
                    self.evidence_reference(evidence),
                    evidence_label="bool-numeric.demo-evidence.json",
                )

    def test_verify_evidence_contract_rejects_bool_selector_attempt(self) -> None:
        evidence = self.archived_demo_contract_evidence()
        evidence["demos"][0]["causal_chain"][0]["selector"]["attempt"] = True

        with self.assertRaisesRegex(RuntimeError, "selector lacks run_id/task_id/attempt"):
            validate_demo_evidence_contract(
                evidence,
                self.evidence_reference(evidence),
                evidence_label="bool-selector.demo-evidence.json",
            )

    def test_verify_evidence_contract_rejects_artifact_hash_mismatch(self) -> None:
        evidence = self.archived_demo_contract_evidence()
        evidence["artifact_sha256"] = "d" * 64

        with self.assertRaisesRegex(RuntimeError, "artifact_sha256 does not match"):
            validate_demo_evidence_contract(
                evidence,
                self.evidence_reference(evidence),
                evidence_label="mismatched.demo-evidence.json",
            )

    def test_verify_evidence_contract_rejects_failed_attempt_without_verifier_command(self) -> None:
        evidence = self.archived_demo_contract_evidence()
        failed_step = evidence["demos"][0]["causal_chain"][0]
        failed_step["fields"]["verify_command"] = ""

        with self.assertRaisesRegex(RuntimeError, "first attempt lacks verifier command"):
            validate_demo_evidence_contract(
                evidence,
                self.evidence_reference(evidence),
                evidence_label="missing-failed-command.demo-evidence.json",
            )

    def test_verify_evidence_contract_rejects_reference_missing_required_step(self) -> None:
        evidence = self.archived_demo_contract_evidence()
        broken_reference = {
            "requirements": [
                "failed_first_attempt",
                "archived_verifier_failure_evidence",
                "retry_context_from_failure_evidence",
                "later_passing_attempt",
                "lineage_trajectory_recorded",
            ]
        }

        with self.assertRaisesRegex(RuntimeError, "expected six-step proof"):
            validate_demo_evidence_contract(
                evidence,
                broken_reference,
                evidence_label="fresh.demo-evidence.json",
            )

    def test_verify_evidence_contract_rejects_pass_at_one_without_retry_chain(self) -> None:
        evidence = self.archived_demo_contract_evidence()
        evidence["complete"] = False
        evidence["demos"] = []

        with self.assertRaisesRegex(RuntimeError, "incomplete"):
            validate_demo_evidence_contract(
                evidence,
                self.evidence_reference(evidence),
                evidence_label="pass-at-one.demo-evidence.json",
            )

    def test_verify_evidence_contract_rejects_retry_without_archived_failure_link(self) -> None:
        evidence = self.archived_demo_contract_evidence()
        retry_step = evidence["demos"][0]["causal_chain"][2]
        retry_step["fields"][0]["retry_context_links_archived_failure"] = False

        with self.assertRaisesRegex(RuntimeError, "does not link archived failure"):
            validate_demo_evidence_contract(
                evidence,
                self.evidence_reference(evidence),
                evidence_label="broken.demo-evidence.json",
            )

    def test_verify_evidence_contract_rejects_retry_without_failed_verifier_details(self) -> None:
        evidence = self.archived_demo_contract_evidence()
        retry_step = evidence["demos"][0]["causal_chain"][2]
        retry_step["fields"][0].pop("failed_verify_command", None)

        with self.assertRaisesRegex(RuntimeError, "does not carry failed verifier command"):
            validate_demo_evidence_contract(
                evidence,
                self.evidence_reference(evidence),
                evidence_label="missing-failed-verifier-details.demo-evidence.json",
            )

    def test_verify_evidence_contract_rejects_retry_without_failed_lineage_boundary(self) -> None:
        evidence = self.archived_demo_contract_evidence()
        retry_step = evidence["demos"][0]["causal_chain"][2]
        retry_step["fields"][0]["failed_lineage_records_after"] = 0

        with self.assertRaisesRegex(RuntimeError, "does not carry failed lineage boundary"):
            validate_demo_evidence_contract(
                evidence,
                self.evidence_reference(evidence),
                evidence_label="missing-failed-lineage-boundary.demo-evidence.json",
            )

    def test_verify_evidence_contract_rejects_retry_summary_without_failed_lineage_boundary(self) -> None:
        evidence = self.archived_demo_contract_evidence()
        retry_step = evidence["demos"][0]["causal_chain"][2]
        retry_step["failed_lineage_records_after"] = 0

        with self.assertRaisesRegex(RuntimeError, "retry summary does not carry failed lineage boundary"):
            validate_demo_evidence_contract(
                evidence,
                self.evidence_reference(evidence),
                evidence_label="missing-failed-lineage-summary.demo-evidence.json",
            )

    def test_verify_evidence_contract_rejects_retry_summary_archived_selector_mismatch(self) -> None:
        evidence = self.archived_demo_contract_evidence()
        retry_step = evidence["demos"][0]["causal_chain"][2]
        retry_step["archived_failure_selector"]["attempt"] = 2

        with self.assertRaisesRegex(RuntimeError, "retry summary is not tied to archived failure selector"):
            validate_demo_evidence_contract(
                evidence,
                self.evidence_reference(evidence),
                evidence_label="wrong-retry-archive-selector.demo-evidence.json",
            )

    def test_verify_evidence_contract_rejects_retry_summary_artifact_hash_mismatch(self) -> None:
        evidence = self.archived_demo_contract_evidence()
        retry_step = evidence["demos"][0]["causal_chain"][2]
        retry_step["archived_failure_artifact_sha256"] = "e" * 64

        with self.assertRaisesRegex(RuntimeError, "retry summary is not tied to archived failure artifact hash"):
            validate_demo_evidence_contract(
                evidence,
                self.evidence_reference(evidence),
                evidence_label="wrong-retry-archive-hash.demo-evidence.json",
            )

    def test_verify_evidence_contract_rejects_missing_retry_selectors(self) -> None:
        evidence = self.archived_demo_contract_evidence()
        retry_step = evidence["demos"][0]["causal_chain"][2]
        retry_step.pop("selectors")

        with self.assertRaisesRegex(RuntimeError, "retry_context_from_failure_evidence.selectors"):
            validate_demo_evidence_contract(
                evidence,
                self.evidence_reference(evidence),
                evidence_label="missing-selectors.demo-evidence.json",
            )

    def test_verify_evidence_contract_rejects_unpaired_retry_evidence_rows(self) -> None:
        evidence = self.archived_demo_contract_evidence()
        retry_step = evidence["demos"][0]["causal_chain"][2]
        retry_step["evidence_rows"].append(dict(retry_step["evidence_rows"][0]))

        with self.assertRaisesRegex(RuntimeError, "paired retry selectors and evidence rows"):
            validate_demo_evidence_contract(
                evidence,
                self.evidence_reference(evidence),
                evidence_label="extra-retry-row.demo-evidence.json",
            )

    def test_verify_evidence_contract_rejects_non_advancing_archived_failure_lineage(self) -> None:
        evidence = self.archived_demo_contract_evidence()
        failed_step = evidence["demos"][0]["causal_chain"][0]
        archived_step = evidence["demos"][0]["causal_chain"][1]
        failed_selector = failed_step["selector"]
        rows = load_jsonl_rows(repo_path(DEFAULT_ARCHIVE))
        failed_row = require_artifact_row(
            artifact_rows_by_selector(rows), failed_selector, label="test failed selector"
        )
        failed_row["lineage_records_before"] = 0
        failed_row["lineage_records_after"] = 0
        archived_step["fields"]["lineage_advanced"] = True
        archived_step["fields"]["lineage_records_before"] = 0
        archived_step["fields"]["lineage_records_after"] = 0
        self.sync_embedded_rows_for_selector(
            evidence, failed_selector, normalized_artifact_row(failed_row)
        )

        with mock.patch(__name__ + ".load_jsonl_rows", return_value=rows):
            with self.assertRaisesRegex(RuntimeError, "failure evidence did not advance lineage"):
                validate_demo_evidence_contract(
                    evidence,
                    self.evidence_reference(evidence),
                    evidence_label="non-advancing-failure.demo-evidence.json",
                )

    def test_verify_evidence_contract_rejects_lineage_that_does_not_span_later_pass(self) -> None:
        evidence = self.archived_demo_contract_evidence()
        failed_step = evidence["demos"][0]["causal_chain"][0]
        lineage_step = evidence["demos"][0]["causal_chain"][4]
        lineage_step["evidence_rows"] = [failed_step["evidence_row"]]
        lineage_step["fields"]["attempts"] = [1]

        with self.assertRaisesRegex(RuntimeError, "lineage does not span failed attempt and later pass"):
            validate_demo_evidence_contract(
                evidence,
                self.evidence_reference(evidence),
                evidence_label="lineage-missing-later-pass.demo-evidence.json",
            )

    def test_verify_evidence_contract_rejects_promotion_selector_not_later_pass(self) -> None:
        evidence = self.archived_demo_contract_evidence()
        failed_step = evidence["demos"][0]["causal_chain"][0]
        promotion_step = evidence["demos"][0]["causal_chain"][5]
        promotion_step["selector"] = failed_step["selector"]

        with self.assertRaisesRegex(RuntimeError, "promotion selector differs from later passing attempt"):
            validate_demo_evidence_contract(
                evidence,
                self.evidence_reference(evidence),
                evidence_label="promotion-selector-mismatch.demo-evidence.json",
            )

    def test_verify_evidence_contract_rejects_absent_promotion_evidence(self) -> None:
        evidence = self.archived_demo_contract_evidence()
        promotion_step = evidence["demos"][0]["causal_chain"][5]
        promotion_selector = promotion_step["selector"]
        rows = load_jsonl_rows(repo_path(DEFAULT_ARCHIVE))
        promotion_row = require_artifact_row(
            artifact_rows_by_selector(rows), promotion_selector, label="test promotion selector"
        )
        promotion_row["stdout"] = ""
        promotion_row["stderr"] = ""
        promotion_step["fields"]["promotion_evidence_present"] = False
        self.sync_embedded_rows_for_selector(
            evidence, promotion_selector, normalized_artifact_row(promotion_row)
        )

        with mock.patch(__name__ + ".load_jsonl_rows", return_value=rows):
            with self.assertRaisesRegex(RuntimeError, "promotion lacks gated apply evidence"):
                validate_demo_evidence_contract(
                    evidence,
                    self.evidence_reference(evidence),
                    evidence_label="missing-promotion.demo-evidence.json",
                )

    def test_verify_evidence_contract_rejects_promotion_fields_spoof_without_artifact_evidence(self) -> None:
        evidence = self.archived_demo_contract_evidence()
        promotion_step = evidence["demos"][0]["causal_chain"][5]
        promotion_selector = promotion_step["selector"]
        rows = load_jsonl_rows(repo_path(DEFAULT_ARCHIVE))
        promotion_row = require_artifact_row(
            artifact_rows_by_selector(rows), promotion_selector, label="test promotion selector"
        )
        promotion_row["stdout"] = ""
        promotion_row["stderr"] = ""
        promotion_row["promotion_evidence_present"] = False
        promotion_step["fields"]["promotion_evidence_present"] = True
        self.sync_embedded_rows_for_selector(
            evidence, promotion_selector, normalized_artifact_row(promotion_row)
        )

        with mock.patch(__name__ + ".load_jsonl_rows", return_value=rows):
            with self.assertRaisesRegex(RuntimeError, "promotion lacks gated apply evidence"):
                validate_demo_evidence_contract(
                    evidence,
                    self.evidence_reference(evidence),
                    evidence_label="promotion-field-spoof.demo-evidence.json",
                )

    def test_verify_evidence_contract_rejects_stringly_legacy_promotion_booleans(self) -> None:
        evidence = self.archived_demo_contract_evidence()
        promotion_step = evidence["demos"][0]["causal_chain"][5]
        promotion_selector = promotion_step["selector"]
        rows = load_jsonl_rows(repo_path(DEFAULT_ARCHIVE))
        promotion_row = require_artifact_row(
            artifact_rows_by_selector(rows), promotion_selector, label="test promotion selector"
        )
        promotion_row["stdout"] = ""
        promotion_row["stderr"] = ""
        promotion_row["promotion_evidence_present"] = "true"
        promotion_step["fields"] = {
            "verify_returncode": 0,
            "lineage_reconciled_by_core": True,
            "promotion_evidence_present": "true",
        }
        self.sync_embedded_rows_for_selector(
            evidence, promotion_selector, normalized_artifact_row(promotion_row)
        )

        with mock.patch(__name__ + ".load_jsonl_rows", return_value=rows):
            with self.assertRaisesRegex(RuntimeError, "promotion lacks gated apply evidence"):
                validate_demo_evidence_contract(
                    evidence,
                    self.evidence_reference(evidence),
                    evidence_label="stringly-legacy-promotion.demo-evidence.json",
                )

    def test_verify_evidence_contract_rejects_promotion_when_artifact_verifier_failed(self) -> None:
        evidence = self.archived_demo_contract_evidence()
        promotion_step = evidence["demos"][0]["causal_chain"][5]
        promotion_selector = promotion_step["selector"]
        rows = load_jsonl_rows(repo_path(DEFAULT_ARCHIVE))
        promotion_row = require_artifact_row(
            artifact_rows_by_selector(rows), promotion_selector, label="test promotion selector"
        )
        promotion_row["verify_returncode"] = 1
        promotion_step["fields"]["verify_returncode"] = 1
        self.sync_embedded_rows_for_selector(
            evidence, promotion_selector, normalized_artifact_row(promotion_row)
        )

        with mock.patch(__name__ + ".load_jsonl_rows", return_value=rows):
            with self.assertRaisesRegex(RuntimeError, "later artifact row is not verifier-passing"):
                validate_demo_evidence_contract(
                    evidence,
                    self.evidence_reference(evidence),
                    evidence_label="failed-promotion.demo-evidence.json",
                )

    def test_verify_evidence_contract_rejects_stringly_promotion_booleans(self) -> None:
        evidence = self.archived_demo_contract_evidence()
        promotion_step = evidence["demos"][0]["causal_chain"][5]
        promotion_selector = promotion_step["selector"]
        rows = load_jsonl_rows(repo_path(DEFAULT_ARCHIVE))
        promotion_row = require_artifact_row(
            artifact_rows_by_selector(rows), promotion_selector, label="test promotion selector"
        )
        promotion_row["stdout"] = ""
        promotion_row["stderr"] = ""
        promotion_row["promotion"] = {
            "verifier_gated": "true",
            "evidence_present": "true",
            "lineage_reconciled_by_core": "true",
            "verify_returncode": 0,
        }
        promotion_step["fields"] = {
            "verify_returncode": 0,
            "lineage_reconciled_by_core": True,
            "promotion_verifier_gated": "true",
            "promotion_structured_evidence_present": "true",
            "promotion_lineage_reconciled_by_core": "true",
            "promotion_verify_returncode": 0,
        }
        self.sync_embedded_rows_for_selector(
            evidence, promotion_selector, normalized_artifact_row(promotion_row)
        )

        with mock.patch(__name__ + ".load_jsonl_rows", return_value=rows):
            with self.assertRaisesRegex(RuntimeError, "promotion lacks gated apply evidence"):
                validate_demo_evidence_contract(
                    evidence,
                    self.evidence_reference(evidence),
                    evidence_label="stringly-promotion.demo-evidence.json",
                )

    def test_verify_archive_print_only_includes_demo_evidence_json(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            stdout = io.StringIO()
            evidence = Path(tmpdir) / "evidence.json"
            with contextlib.redirect_stdout(stdout):
                result = main(
                    [
                        "verify-archive",
                        "--evidence-json",
                        str(evidence),
                        "--print-only",
                    ]
                )

        output = stdout.getvalue()
        self.assertEqual(result, 0)
        self.assertIn("--demo-evidence-json", output)
        self.assertIn(str(evidence), output)
        self.assertIn("bench/self_correction_score.py", output)

    def test_fresh_command_requires_clean_source_by_default(self) -> None:
        args = argparse.Namespace(
            fixture=DEFAULT_FIXTURE,
            provider=DEFAULT_PROVIDER,
            runs=3,
            attempts=3,
            max_tokens=100_000,
            timeout=1800,
            results=Path("docs/benchmark-results/self-correction/fresh.jsonl"),
            run_id="fresh-demo",
            allow_dirty_source=False,
            keep_workspace=False,
            evidence_json=None,
        )

        command = fresh_command(args)

        self.assertIn("--require-clean-source", command)
        self.assertIn("--fixture", command)
        self.assertIn(DEFAULT_FIXTURE, command)
        self.assertIn("--provider", command)
        self.assertIn(DEFAULT_PROVIDER, command)
        self.assertIn("--runs", command)
        self.assertIn("3", command)
        self.assertIn("--max-tokens", command)
        self.assertIn("100000", command)
        self.assertIn("--timeout", command)
        self.assertIn("1800", command)
        self.assertIn("--run-id", command)
        self.assertIn("fresh-demo", command)

    def test_fresh_command_can_print_dirty_local_smoke(self) -> None:
        args = argparse.Namespace(
            fixture=DEFAULT_FIXTURE,
            provider=DEFAULT_PROVIDER,
            runs=1,
            attempts=2,
            max_tokens=100_000,
            timeout=1800,
            results=Path("/tmp/local-smoke.jsonl"),
            run_id=None,
            allow_dirty_source=True,
            keep_workspace=True,
            evidence_json=None,
        )

        command = fresh_command(args)

        self.assertNotIn("--require-clean-source", command)
        self.assertIn("--keep-workspace", command)

    def test_fresh_print_only_shows_internal_validation_before_scoring(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            stdout = io.StringIO()
            results = Path(tmpdir) / "fresh-print-only.jsonl"
            with contextlib.redirect_stdout(stdout):
                result = main(
                    [
                        "fresh",
                        "--results",
                        str(results),
                        "--run-id",
                        "fresh-demo",
                        "--print-only",
                    ]
                )

        output = stdout.getvalue()
        self.assertEqual(result, 0)
        self.assertIn("# would validate fresh results before scoring", output)
        self.assertIn("all rows match run_id 'fresh-demo'", output)
        self.assertIn("no host-specific path markers are present", output)
        self.assertIn("source_dirty=false", output)
        self.assertIn("no_external_solution_search=true and network_policy=Isolated are recorded for every row", output)
        self.assertIn(str(results.with_suffix(".demo-evidence.json")), output)
        self.assertIn("verify-evidence-contract", output)
        self.assertIn("--fresh-run-id", output)
        self.assertIn("fresh-demo", output)
        self.assertIn("--max-tokens", output)
        self.assertIn("100000", output)
        self.assertIn("--timeout", output)
        self.assertIn("1800", output)
        self.assertLess(
            output.index("# would validate fresh results before scoring"),
            output.index("bench/self_correction_score.py"),
        )
        self.assertLess(
            output.index("bench/self_correction_score.py"),
            output.index("verify-evidence-contract"),
        )

    def test_fresh_print_only_honors_explicit_evidence_json(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            stdout = io.StringIO()
            results = Path(tmpdir) / "fresh-print-only.jsonl"
            evidence = Path(tmpdir) / "custom-evidence.json"
            with contextlib.redirect_stdout(stdout):
                result = main(
                    [
                        "fresh",
                        "--results",
                        str(results),
                        "--run-id",
                        "fresh-demo",
                        "--evidence-json",
                        str(evidence),
                        "--print-only",
                    ]
                )

        output = stdout.getvalue()
        self.assertEqual(result, 0)
        self.assertIn(str(evidence), output)
        self.assertNotIn(str(results.with_suffix(".demo-evidence.json")), output)

    def test_fresh_preflight_checks_local_prerequisites_and_prints_commands(self) -> None:
        original_which = shutil.which
        shutil.which = lambda binary: f"/usr/bin/{binary}"
        try:
            with tempfile.TemporaryDirectory() as tmpdir:
                stdout = io.StringIO()
                results = Path(tmpdir) / "fresh-preflight.jsonl"
                with contextlib.redirect_stdout(stdout):
                    result = main(
                        [
                            "fresh",
                            "--results",
                            str(results),
                            "--run-id",
                            "fresh-demo",
                            "--allow-dirty-source",
                            "--provider",
                            "local-test-provider/model",
                            "--preflight-only",
                        ]
                    )
        finally:
            shutil.which = original_which

        output = stdout.getvalue()
        self.assertEqual(result, 0)
        self.assertIn("# preflight checked local prerequisites", output)
        self.assertIn("dirty source allowed", output)
        self.assertIn("benchmark task payloads request network_policy=Isolated", output)
        self.assertIn(
            "audited sandbox/provider-allowlist execution is not implemented/enforced yet",
            output,
        )
        self.assertIn("Live provider auth, quota, and model availability are not verified", output)
        self.assertIn("bench/self_correction.py", output)
        self.assertIn("# would validate fresh results before scoring", output)
        self.assertIn("no_external_solution_search=true and network_policy=Isolated are recorded for every row", output)
        self.assertIn(str(results.with_suffix(".demo-evidence.json")), output)
        self.assertIn("verify-evidence-contract", output)
        self.assertIn("--reference-evidence-json", output)
        self.assertIn(str(DEFAULT_ARCHIVE_EVIDENCE), output)
        self.assertIn("--fresh-run-id", output)
        self.assertIn("fresh-demo", output)
        self.assertIn("--max-tokens", output)
        self.assertIn("100000", output)
        self.assertIn("--timeout", output)
        self.assertIn("1800", output)
        self.assertLess(output.index("bench/self_correction_score.py"), output.index("verify-evidence-contract"))

    def test_fresh_preflight_writes_machine_readable_readiness_report(self) -> None:
        original_which = shutil.which
        shutil.which = lambda binary: f"/usr/bin/{binary}"
        try:
            with tempfile.TemporaryDirectory() as tmpdir:
                stdout = io.StringIO()
                results = Path(tmpdir) / "fresh-preflight.jsonl"
                report = Path(tmpdir) / "fresh-preflight.report.json"
                with contextlib.redirect_stdout(stdout):
                    result = main(
                        [
                            "fresh",
                            "--results",
                            str(results),
                            "--run-id",
                            "fresh-demo",
                            "--allow-dirty-source",
                            "--provider",
                            "local-test-provider/model",
                            "--preflight-only",
                            "--preflight-report-json",
                            str(report),
                        ]
                    )
                data = json.loads(report.read_text(encoding="utf-8"))
        finally:
            shutil.which = original_which

        self.assertEqual(result, 0)
        self.assertIn("# wrote preflight report", stdout.getvalue())
        self.assertEqual(data["mode"], "fresh_preflight")
        self.assertFalse(data["creates_loop_evidence"])
        self.assertFalse(data["provider_backed_benchmark_executed"])
        self.assertFalse(data["results_created"])
        self.assertFalse(data["evidence_json_created"])
        self.assertFalse(data["fresh_provenance_contract_executed"])
        self.assertFalse(data["live_provider_auth_quota_model_checked"])
        self.assertEqual(data["results"], str(results))
        self.assertEqual(data["evidence_json"], str(results.with_suffix(".demo-evidence.json")))
        self.assertEqual(data["preflight_report_json"], str(report))
        self.assertFalse(results.exists())
        self.assertFalse(results.with_suffix(".demo-evidence.json").exists())
        self.assertTrue(data["checks"]["preflight_report_path_empty"])
        self.assertTrue(data["checks"]["preflight_report_path_distinct_from_results"])
        self.assertTrue(data["checks"]["preflight_report_path_distinct_from_evidence"])
        self.assertEqual(data["checks"]["provider_binary"], "local-test-provider")
        self.assertTrue(data["checks"]["provider_binary_present"])
        self.assertFalse(data["checks"]["local_provider_config_checked"])
        self.assertIsNone(data["checks"]["local_provider_config_present_when_supported"])
        self.assertTrue(data["checks"]["dirty_source_allowed"])
        self.assertIsNone(data["checks"]["source_clean_checked_before_output_creation"])
        self.assertEqual(data["checks"]["benchmark_task_network_policy"], "Isolated")
        self.assertEqual(
            data["checks"]["restricted_network_policy_current_behavior"],
            "fail_closed_provider_launch_until_audited_sandbox_provider_allowlist",
        )
        self.assertFalse(data["checks"]["audited_sandbox_provider_allowlist_enforced"])
        self.assertEqual(data["checks"]["audited_sandbox_provider_allowlist_status"], "not_implemented")
        notes = " ".join(data["notes"])
        self.assertIn("before fresh results/evidence files are created", notes)
        self.assertIn("network_policy=Isolated", notes)
        self.assertIn("fail closed until an audited sandbox/provider allowlist exists", notes)
        self.assertIn("agent network boundary precondition", notes)
        self.assertIn("does not execute it", notes)
        self.assertIn("expected to fail closed", notes)
        self.assertIn("bench/self_correction.py", data["commands"]["harness"])
        self.assertIn("--demo-evidence-json", data["commands"]["scorer"])
        self.assertTrue(data["checks"]["agent_network_boundary_precondition_required"])
        self.assertFalse(data["checks"]["agent_network_boundary_precondition_executed"])
        self.assertEqual(
            data["checks"]["agent_network_boundary_precondition_status"],
            "not_executed_in_preflight",
        )
        self.assertEqual(
            data["commands"]["agent_network_boundary_inventory"],
            "python3 bench/agent_network_boundary_check.py --self-test",
        )
        self.assertEqual(
            data["commands"]["agent_network_boundary_precondition"],
            "python3 bench/agent_network_boundary_check.py --require-sandbox-runtime",
        )
        self.assertIn("verify-evidence-contract", data["commands"]["fresh_provenance_contract"])
        self.assertIn("--reference-evidence-json", data["commands"]["fresh_provenance_contract"])
        self.assertIn(str(DEFAULT_ARCHIVE_EVIDENCE), data["commands"]["fresh_provenance_contract"])
        self.assertIn("--fresh-run-id", data["commands"]["fresh_provenance_contract"])
        self.assertIn("fresh-demo", data["commands"]["fresh_provenance_contract"])
        self.assertIn("--max-tokens", data["commands"]["fresh_provenance_contract"])
        self.assertIn("100000", data["commands"]["fresh_provenance_contract"])
        self.assertIn("--timeout", data["commands"]["fresh_provenance_contract"])
        self.assertIn("1800", data["commands"]["fresh_provenance_contract"])
        self.assertIn("future outputs only", " ".join(data["notes"]))
        self.assertIn("not loop evidence", " ".join(data["notes"]))

    def test_fresh_preflight_can_write_boundary_inventory_artifact_and_report_summary(self) -> None:
        original_which = shutil.which
        shutil.which = lambda binary: f"/usr/bin/{binary}"
        try:
            with tempfile.TemporaryDirectory() as tmpdir:
                stdout = io.StringIO()
                results = Path(tmpdir) / "fresh-preflight.jsonl"
                report = Path(tmpdir) / "fresh-preflight.report.json"
                inventory = Path(tmpdir) / "fresh-preflight.boundary.json"

                def write_inventory(path: Path) -> dict[str, object]:
                    path.write_text(
                        json.dumps({"launch_sandbox_enforced": False}) + "\n",
                        encoding="utf-8",
                    )
                    return {
                        "path": str(path),
                        "command": "python3 bench/agent_network_boundary_check.py --json",
                        "status": "recorded",
                        "creates_loop_evidence": False,
                        "proves_runtime_sandbox_enforcement": False,
                        "a2_owned_fail_closed": True,
                        "a2_owned_sandbox_enforced": False,
                        "sandbox_runtime_available": False,
                        "launch_sandbox_enforced": False,
                    }

                with mock.patch(
                    __name__ + ".run_agent_network_boundary_inventory_json",
                    side_effect=write_inventory,
                ) as run_inventory, contextlib.redirect_stdout(stdout):
                    result = main(
                        [
                            "fresh",
                            "--results",
                            str(results),
                            "--run-id",
                            "fresh-demo",
                            "--allow-dirty-source",
                            "--provider",
                            "local-test-provider/model",
                            "--preflight-only",
                            "--preflight-report-json",
                            str(report),
                            "--preflight-boundary-inventory-json",
                            str(inventory),
                        ]
                    )
                data = json.loads(report.read_text(encoding="utf-8"))
                inventory_exists = inventory.exists()
                with contextlib.redirect_stdout(io.StringIO()):
                    verify_fresh_preflight_report(report)
        finally:
            shutil.which = original_which

        self.assertEqual(result, 0)
        run_inventory.assert_called_once_with(inventory)
        self.assertTrue(inventory_exists)
        self.assertIn("# wrote agent network boundary inventory", stdout.getvalue())
        self.assertIn("# wrote preflight report", stdout.getvalue())
        self.assertTrue(data["boundary_inventory_created"])
        self.assertEqual(data["boundary_inventory_json"], str(inventory))
        self.assertFalse(data["boundary_inventory"]["creates_loop_evidence"])
        self.assertFalse(data["boundary_inventory"]["proves_runtime_sandbox_enforcement"])
        self.assertTrue(data["checks"]["agent_network_boundary_inventory_json_requested"])
        self.assertTrue(data["checks"]["agent_network_boundary_inventory_json_executed"])
        self.assertEqual(data["checks"]["agent_network_boundary_inventory_json_status"], "recorded")

    def test_preflight_boundary_inventory_requires_preflight_only(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            stderr = io.StringIO()
            results = Path(tmpdir) / "fresh.jsonl"
            inventory = Path(tmpdir) / "fresh.boundary.json"
            with contextlib.redirect_stderr(stderr):
                result = main(
                    [
                        "fresh",
                        "--results",
                        str(results),
                        "--run-id",
                        "fresh-demo",
                        "--preflight-boundary-inventory-json",
                        str(inventory),
                    ]
                )

        self.assertEqual(result, 2)
        self.assertIn("--preflight-boundary-inventory-json requires --preflight-only", stderr.getvalue())
        self.assertFalse(inventory.exists())

    def test_fresh_preflight_boundary_inventory_refuses_non_empty_file(self) -> None:
        original_which = shutil.which
        shutil.which = lambda binary: f"/usr/bin/{binary}"
        try:
            with tempfile.TemporaryDirectory() as tmpdir:
                stderr = io.StringIO()
                results = Path(tmpdir) / "fresh.jsonl"
                inventory = Path(tmpdir) / "fresh.boundary.json"
                inventory.write_text('{"stale": true}\n', encoding="utf-8")
                with contextlib.redirect_stderr(stderr):
                    result = main(
                        [
                            "fresh",
                            "--results",
                            str(results),
                            "--run-id",
                            "fresh-demo",
                            "--allow-dirty-source",
                            "--provider",
                            "local-test-provider/model",
                            "--preflight-only",
                            "--preflight-boundary-inventory-json",
                            str(inventory),
                        ]
                    )
        finally:
            shutil.which = original_which

        self.assertEqual(result, 2)
        self.assertIn("fresh demo boundary inventory path already contains data", stderr.getvalue())

    def test_fresh_preflight_boundary_inventory_refuses_report_alias(self) -> None:
        original_which = shutil.which
        shutil.which = lambda binary: f"/usr/bin/{binary}"
        try:
            with tempfile.TemporaryDirectory() as tmpdir:
                stderr = io.StringIO()
                results = Path(tmpdir) / "fresh.jsonl"
                report = Path(tmpdir) / "fresh.report.json"
                with contextlib.redirect_stderr(stderr):
                    result = main(
                        [
                            "fresh",
                            "--results",
                            str(results),
                            "--run-id",
                            "fresh-demo",
                            "--allow-dirty-source",
                            "--provider",
                            "local-test-provider/model",
                            "--preflight-only",
                            "--preflight-report-json",
                            str(report),
                            "--preflight-boundary-inventory-json",
                            str(report),
                        ]
                    )
        finally:
            shutil.which = original_which

        self.assertEqual(result, 2)
        self.assertIn("boundary inventory path must be distinct from preflight report path", stderr.getvalue())
        self.assertFalse(report.exists())

    def test_fresh_preflight_boundary_inventory_alias_guard_resolves_symlinked_paths(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)
            real_dir = root / "real"
            real_dir.mkdir()
            symlink_dir = root / "link"
            try:
                symlink_dir.symlink_to(real_dir, target_is_directory=True)
            except (OSError, NotImplementedError):
                self.skipTest("filesystem does not support directory symlinks")
            results = real_dir / "fresh.jsonl"
            inventory_alias = symlink_dir / "fresh.jsonl"

            with self.assertRaisesRegex(RuntimeError, "boundary inventory path must be distinct from results path"):
                ensure_preflight_boundary_inventory_path(
                    inventory_alias,
                    results=results,
                    evidence_json=real_dir / "fresh.demo-evidence.json",
                    preflight_report_json=real_dir / "fresh.report.json",
                )

    def test_fresh_preflight_report_records_clean_check_before_output_creation(self) -> None:
        args = argparse.Namespace(
            results=Path("docs/benchmark-results/self-correction/fresh.jsonl"),
            preflight_report_json=Path("docs/benchmark-results/self-correction/fresh.preflight.json"),
            fixture=DEFAULT_FIXTURE,
            provider=DEFAULT_PROVIDER,
            run_id="fresh-demo",
            runs=3,
            attempts=3,
            max_tokens=100_000,
            timeout=1800,
            allow_dirty_source=False,
            keep_workspace=False,
        )

        source_metadata = {
            "source_head": "1234567890abcdef1234567890abcdef12345678",
            "source_head_short": "1234567",
            "source_branch": "(detached)",
            "source_dirty": False,
        }
        with mock.patch(__name__ + ".current_source_metadata", return_value=source_metadata):
            data = fresh_preflight_report(args, Path("docs/benchmark-results/self-correction/fresh.demo-evidence.json"))

        self.assertEqual(data["source_metadata"], source_metadata)
        self.assertTrue(data["checks"]["source_clean_required"])
        self.assertTrue(data["checks"]["source_clean"])
        self.assertTrue(data["checks"]["source_clean_checked_before_output_creation"])
        self.assertIn("source revision metadata", " ".join(data["notes"]))
        self.assertIn("before fresh results/evidence files are created", " ".join(data["notes"]))

    def test_fresh_preflight_report_records_dirty_source_when_allowed(self) -> None:
        args = argparse.Namespace(
            results=Path("docs/benchmark-results/self-correction/fresh.jsonl"),
            preflight_report_json=Path("docs/benchmark-results/self-correction/fresh.preflight.json"),
            fixture=DEFAULT_FIXTURE,
            provider=DEFAULT_PROVIDER,
            run_id="fresh-demo",
            runs=3,
            attempts=3,
            max_tokens=100_000,
            timeout=1800,
            allow_dirty_source=True,
            keep_workspace=False,
        )
        dirty_metadata = {
            "source_head": "1234567890abcdef1234567890abcdef12345678",
            "source_head_short": "1234567",
            "source_branch": "main",
            "source_dirty": True,
        }

        with mock.patch(__name__ + ".current_source_metadata", return_value=dirty_metadata):
            data = fresh_preflight_report(args, Path("docs/benchmark-results/self-correction/fresh.demo-evidence.json"))

        self.assertEqual(data["source_metadata"], dirty_metadata)
        self.assertFalse(data["checks"]["source_clean_required"])
        self.assertIsNone(data["checks"]["source_clean"])
        self.assertIsNone(data["checks"]["source_clean_checked_before_output_creation"])
        self.assertTrue(data["checks"]["dirty_source_allowed"])

    def test_verify_preflight_report_prints_stale_snapshot_without_loop_claim(self) -> None:
        report = {
            "mode": "fresh_preflight",
            "creates_loop_evidence": False,
            "provider_backed_benchmark_executed": False,
            "results_created": False,
            "evidence_json_created": False,
            "fresh_provenance_contract_executed": False,
            "live_provider_auth_quota_model_checked": False,
            "checks": self.required_preflight_network_checks(),
            "source_metadata": {
                "source_head": "1234567890abcdef1234567890abcdef12345678",
                "source_dirty": False,
            },
        }
        current = {
            "source_head": "abcdef1234567890abcdef1234567890abcdef12",
            "source_dirty": False,
            "source_head_short": "abcdef1",
            "source_branch": "main",
        }
        with tempfile.TemporaryDirectory() as tmpdir:
            report_path = Path(tmpdir) / "fresh.report.json"
            report_path.write_text(json.dumps(report), encoding="utf-8")
            stdout = io.StringIO()
            with mock.patch(__name__ + ".current_source_metadata", return_value=current), contextlib.redirect_stdout(stdout):
                verify_fresh_preflight_report(report_path)

        output = stdout.getvalue()
        self.assertIn("STALE source snapshot differs from current HEAD/state", output)
        self.assertIn("benchmark_task_network_policy: Isolated", output)
        self.assertIn("fail_closed_provider_launch_until_audited_sandbox_provider_allowlist", output)
        self.assertIn("audited_sandbox_provider_allowlist_enforced: False", output)
        self.assertIn("audited_sandbox_provider_allowlist_status: not_implemented", output)
        self.assertIn("agent_network_boundary_precondition_required: True", output)
        self.assertIn("agent_network_boundary_precondition_executed: False", output)
        self.assertIn("agent_network_boundary_precondition_status: not_executed_in_preflight", output)
        self.assertIn("readiness only", output)
        self.assertIn("not loop evidence", output)

    def test_verify_preflight_report_require_current_head_accepts_matching_snapshot(self) -> None:
        report = {
            "mode": "fresh_preflight",
            "creates_loop_evidence": False,
            "provider_backed_benchmark_executed": False,
            "results_created": False,
            "evidence_json_created": False,
            "fresh_provenance_contract_executed": False,
            "live_provider_auth_quota_model_checked": False,
            "checks": self.required_preflight_network_checks(),
            "source_metadata": {
                "source_head": "1234567890abcdef1234567890abcdef12345678",
                "source_dirty": True,
            },
        }
        current = {
            "source_head": "1234567890abcdef1234567890abcdef12345678",
            "source_dirty": True,
            "source_head_short": "1234567",
            "source_branch": "main",
        }
        with tempfile.TemporaryDirectory() as tmpdir:
            report_path = Path(tmpdir) / "fresh.report.json"
            report_path.write_text(json.dumps(report), encoding="utf-8")
            stdout = io.StringIO()
            with mock.patch(__name__ + ".current_source_metadata", return_value=current), contextlib.redirect_stdout(stdout):
                verify_fresh_preflight_report(report_path, require_current_head=True)

        output = stdout.getvalue()
        self.assertIn("PASS source snapshot matches current HEAD/state", output)
        self.assertIn("benchmark_task_network_policy: Isolated", output)
        self.assertIn("audited_sandbox_provider_allowlist_enforced: False", output)
        self.assertIn("agent_network_boundary_precondition_required: True", output)
        self.assertIn("agent_network_boundary_precondition_executed: False", output)
        self.assertIn("agent_network_boundary_precondition_status: not_executed_in_preflight", output)
        self.assertIn("readiness only", output)
        self.assertIn("not loop evidence", output)

    def test_verify_preflight_report_rejects_declared_future_outputs_that_now_exist(self) -> None:
        current = {
            "source_head": "1234567890abcdef1234567890abcdef12345678",
            "source_dirty": False,
            "source_head_short": "1234567",
            "source_branch": "main",
        }
        for populated_field, expected in [
            ("results", "declared results_created=false.*results path now contains data"),
            ("evidence_json", "declared evidence_created=false.*evidence path now contains data"),
        ]:
            with self.subTest(populated_field=populated_field), tempfile.TemporaryDirectory() as tmpdir:
                results = Path(tmpdir) / "fresh-results.jsonl"
                evidence = Path(tmpdir) / "fresh.demo-evidence.json"
                output_path = results if populated_field == "results" else evidence
                output_path.write_text('{"run_id": "fresh-demo"}\n', encoding="utf-8")
                report = {
                    "mode": "fresh_preflight",
                    "creates_loop_evidence": False,
                    "provider_backed_benchmark_executed": False,
                    "results_created": False,
                    "evidence_json_created": False,
                    "fresh_provenance_contract_executed": False,
                    "live_provider_auth_quota_model_checked": False,
                    "results": str(results),
                    "evidence_json": str(evidence),
                    "checks": self.required_preflight_network_checks(),
                    "source_metadata": {
                        "source_head": current["source_head"],
                        "source_dirty": current["source_dirty"],
                    },
                }
                report_path = Path(tmpdir) / "fresh.report.json"
                report_path.write_text(json.dumps(report), encoding="utf-8")
                with mock.patch(__name__ + ".current_source_metadata", return_value=current), contextlib.redirect_stdout(
                    io.StringIO()
                ), self.assertRaisesRegex(RuntimeError, expected):
                    verify_fresh_preflight_report(report_path, require_current_head=True)

    def test_verify_preflight_report_allows_legacy_reports_without_output_paths(self) -> None:
        report = {
            "mode": "fresh_preflight",
            "creates_loop_evidence": False,
            "provider_backed_benchmark_executed": False,
            "results_created": False,
            "evidence_json_created": False,
            "fresh_provenance_contract_executed": False,
            "live_provider_auth_quota_model_checked": False,
            "source_metadata": {
                "source_head": "1234567890abcdef1234567890abcdef12345678",
                "source_dirty": False,
            },
        }
        current = {
            "source_head": "1234567890abcdef1234567890abcdef12345678",
            "source_dirty": False,
            "source_head_short": "1234567",
            "source_branch": "main",
        }
        with tempfile.TemporaryDirectory() as tmpdir:
            report_path = Path(tmpdir) / "fresh.report.json"
            report_path.write_text(json.dumps(report), encoding="utf-8")
            stdout = io.StringIO()
            with mock.patch(__name__ + ".current_source_metadata", return_value=current), contextlib.redirect_stdout(stdout):
                verify_fresh_preflight_report(report_path, require_current_head=True)

        output = stdout.getvalue()
        self.assertIn("PASS source snapshot matches current HEAD/state", output)
        self.assertIn("benchmark_task_network_policy: legacy report: not recorded", output)

    def test_verify_preflight_report_rejects_missing_network_policy_audit_fields(self) -> None:
        current = {
            "source_head": "1234567890abcdef1234567890abcdef12345678",
            "source_dirty": False,
            "source_head_short": "1234567",
            "source_branch": "main",
        }
        for checks, expected in [
            ({}, "checks.benchmark_task_network_policy"),
            (
                {"benchmark_task_network_policy": "Isolated"},
                "checks.restricted_network_policy_current_behavior",
            ),
            (
                {
                    "benchmark_task_network_policy": "Open",
                    "restricted_network_policy_current_behavior": FRESH_PREFLIGHT_RESTRICTED_NETWORK_BEHAVIOR,
                    "audited_sandbox_provider_allowlist_enforced": FRESH_PREFLIGHT_SANDBOX_PROVIDER_ALLOWLIST_ENFORCED,
                    "audited_sandbox_provider_allowlist_status": FRESH_PREFLIGHT_SANDBOX_PROVIDER_ALLOWLIST_STATUS,
                },
                "checks.benchmark_task_network_policy",
            ),
            (
                {
                    "benchmark_task_network_policy": FRESH_PREFLIGHT_BENCHMARK_NETWORK_POLICY,
                    "restricted_network_policy_current_behavior": FRESH_PREFLIGHT_RESTRICTED_NETWORK_BEHAVIOR,
                },
                "checks.audited_sandbox_provider_allowlist_enforced",
            ),
            (
                {
                    "benchmark_task_network_policy": FRESH_PREFLIGHT_BENCHMARK_NETWORK_POLICY,
                    "restricted_network_policy_current_behavior": FRESH_PREFLIGHT_RESTRICTED_NETWORK_BEHAVIOR,
                    "audited_sandbox_provider_allowlist_enforced": False,
                    "audited_sandbox_provider_allowlist_status": "wired",
                    "agent_network_boundary_precondition_required": True,
                },
                "checks.audited_sandbox_provider_allowlist_status",
            ),
            (
                {
                    "benchmark_task_network_policy": FRESH_PREFLIGHT_BENCHMARK_NETWORK_POLICY,
                    "restricted_network_policy_current_behavior": FRESH_PREFLIGHT_RESTRICTED_NETWORK_BEHAVIOR,
                    "audited_sandbox_provider_allowlist_enforced": FRESH_PREFLIGHT_SANDBOX_PROVIDER_ALLOWLIST_ENFORCED,
                    "audited_sandbox_provider_allowlist_status": FRESH_PREFLIGHT_SANDBOX_PROVIDER_ALLOWLIST_STATUS,
                },
                "checks.agent_network_boundary_precondition_required",
            ),
            (
                {
                    "benchmark_task_network_policy": FRESH_PREFLIGHT_BENCHMARK_NETWORK_POLICY,
                    "restricted_network_policy_current_behavior": FRESH_PREFLIGHT_RESTRICTED_NETWORK_BEHAVIOR,
                    "audited_sandbox_provider_allowlist_enforced": FRESH_PREFLIGHT_SANDBOX_PROVIDER_ALLOWLIST_ENFORCED,
                    "audited_sandbox_provider_allowlist_status": FRESH_PREFLIGHT_SANDBOX_PROVIDER_ALLOWLIST_STATUS,
                    "agent_network_boundary_precondition_required": True,
                    "agent_network_boundary_precondition_executed": True,
                    "agent_network_boundary_precondition_status": FRESH_PREFLIGHT_AGENT_NETWORK_BOUNDARY_PRECONDITION_STATUS,
                },
                "checks.agent_network_boundary_precondition_executed",
            ),
            (
                {
                    "benchmark_task_network_policy": FRESH_PREFLIGHT_BENCHMARK_NETWORK_POLICY,
                    "restricted_network_policy_current_behavior": FRESH_PREFLIGHT_RESTRICTED_NETWORK_BEHAVIOR,
                    "audited_sandbox_provider_allowlist_enforced": FRESH_PREFLIGHT_SANDBOX_PROVIDER_ALLOWLIST_ENFORCED,
                    "audited_sandbox_provider_allowlist_status": FRESH_PREFLIGHT_SANDBOX_PROVIDER_ALLOWLIST_STATUS,
                    "agent_network_boundary_precondition_required": True,
                    "agent_network_boundary_precondition_executed": False,
                    "agent_network_boundary_precondition_status": "passed",
                },
                "checks.agent_network_boundary_precondition_status",
            ),
        ]:
            with self.subTest(checks=checks), tempfile.TemporaryDirectory() as tmpdir:
                report = {
                    "mode": "fresh_preflight",
                    "creates_loop_evidence": False,
                    "provider_backed_benchmark_executed": False,
                    "results_created": False,
                    "evidence_json_created": False,
                    "fresh_provenance_contract_executed": False,
                    "live_provider_auth_quota_model_checked": False,
                    "checks": checks,
                    "source_metadata": {
                        "source_head": current["source_head"],
                        "source_dirty": current["source_dirty"],
                    },
                }
                report_path = Path(tmpdir) / "fresh.report.json"
                report_path.write_text(json.dumps(report), encoding="utf-8")
                with mock.patch(__name__ + ".current_source_metadata", return_value=current), contextlib.redirect_stdout(
                    io.StringIO()
                ), self.assertRaisesRegex(RuntimeError, expected):
                    verify_fresh_preflight_report(report_path, require_current_head=True)

    def test_verify_preflight_report_require_current_head_rejects_stale_snapshot(self) -> None:
        report = {
            "mode": "fresh_preflight",
            "creates_loop_evidence": False,
            "provider_backed_benchmark_executed": False,
            "results_created": False,
            "evidence_json_created": False,
            "fresh_provenance_contract_executed": False,
            "live_provider_auth_quota_model_checked": False,
            "checks": self.required_preflight_network_checks(),
            "source_metadata": {
                "source_head": "1234567890abcdef1234567890abcdef12345678",
                "source_dirty": False,
            },
        }
        current = {
            "source_head": "abcdef1234567890abcdef1234567890abcdef12",
            "source_dirty": False,
            "source_head_short": "abcdef1",
            "source_branch": "main",
        }
        with tempfile.TemporaryDirectory() as tmpdir:
            report_path = Path(tmpdir) / "fresh.report.json"
            report_path.write_text(json.dumps(report), encoding="utf-8")
            with mock.patch(__name__ + ".current_source_metadata", return_value=current), contextlib.redirect_stdout(
                io.StringIO()
            ), self.assertRaisesRegex(
                RuntimeError,
                "source_head differs from current HEAD",
            ):
                verify_fresh_preflight_report(report_path, require_current_head=True)

    def test_fresh_preflight_report_refuses_non_empty_file(self) -> None:
        original_which = shutil.which
        shutil.which = lambda binary: f"/usr/bin/{binary}"
        try:
            with tempfile.TemporaryDirectory() as tmpdir:
                stdout = io.StringIO()
                stderr = io.StringIO()
                results = Path(tmpdir) / "fresh-preflight.jsonl"
                report = Path(tmpdir) / "fresh-preflight.report.json"
                report.write_text('{"old": true}\n', encoding="utf-8")
                with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
                    result = main(
                        [
                            "fresh",
                            "--results",
                            str(results),
                            "--run-id",
                            "fresh-demo",
                            "--allow-dirty-source",
                            "--provider",
                            "local-test-provider/model",
                            "--preflight-only",
                            "--preflight-report-json",
                            str(report),
                        ]
                    )
        finally:
            shutil.which = original_which

        self.assertEqual(result, 2)
        self.assertIn("fresh demo preflight report path already contains data", stderr.getvalue())
        self.assertEqual(stdout.getvalue(), "")

    def test_fresh_preflight_report_refuses_results_alias(self) -> None:
        original_which = shutil.which
        shutil.which = lambda binary: f"/usr/bin/{binary}"
        try:
            with tempfile.TemporaryDirectory() as tmpdir:
                stdout = io.StringIO()
                stderr = io.StringIO()
                results = Path(tmpdir) / "fresh-preflight.jsonl"
                with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
                    result = main(
                        [
                            "fresh",
                            "--results",
                            str(results),
                            "--run-id",
                            "fresh-demo",
                            "--allow-dirty-source",
                            "--provider",
                            "local-test-provider/model",
                            "--preflight-only",
                            "--preflight-report-json",
                            str(results),
                        ]
                    )
        finally:
            shutil.which = original_which

        self.assertEqual(result, 2)
        self.assertIn("preflight report path must be distinct from results path", stderr.getvalue())
        self.assertEqual(stdout.getvalue(), "")

    def test_fresh_preflight_report_refuses_default_evidence_alias(self) -> None:
        original_which = shutil.which
        shutil.which = lambda binary: f"/usr/bin/{binary}"
        try:
            with tempfile.TemporaryDirectory() as tmpdir:
                stdout = io.StringIO()
                stderr = io.StringIO()
                results = Path(tmpdir) / "fresh-preflight.jsonl"
                evidence = results.with_suffix(".demo-evidence.json")
                with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
                    result = main(
                        [
                            "fresh",
                            "--results",
                            str(results),
                            "--run-id",
                            "fresh-demo",
                            "--allow-dirty-source",
                            "--provider",
                            "local-test-provider/model",
                            "--preflight-only",
                            "--preflight-report-json",
                            str(evidence),
                        ]
                    )
        finally:
            shutil.which = original_which

        self.assertEqual(result, 2)
        self.assertIn("preflight report path must be distinct from evidence path", stderr.getvalue())
        self.assertEqual(stdout.getvalue(), "")

    def test_fresh_preflight_report_refuses_explicit_evidence_alias(self) -> None:
        original_which = shutil.which
        shutil.which = lambda binary: f"/usr/bin/{binary}"
        try:
            with tempfile.TemporaryDirectory() as tmpdir:
                stdout = io.StringIO()
                stderr = io.StringIO()
                results = Path(tmpdir) / "fresh-preflight.jsonl"
                evidence = Path(tmpdir) / "custom-evidence.json"
                with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
                    result = main(
                        [
                            "fresh",
                            "--results",
                            str(results),
                            "--run-id",
                            "fresh-demo",
                            "--allow-dirty-source",
                            "--provider",
                            "local-test-provider/model",
                            "--evidence-json",
                            str(evidence),
                            "--preflight-only",
                            "--preflight-report-json",
                            str(evidence),
                        ]
                    )
        finally:
            shutil.which = original_which

        self.assertEqual(result, 2)
        self.assertIn("preflight report path must be distinct from evidence path", stderr.getvalue())
        self.assertEqual(stdout.getvalue(), "")

    def test_bare_opencode_provider_does_not_claim_config_check(self) -> None:
        self.assertFalse(provider_config_checked("opencode"))
        self.assertTrue(provider_config_checked("opencode/minimax-coding-plan/MiniMax-M3"))

    def test_preflight_report_requires_preflight_only(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            stdout = io.StringIO()
            stderr = io.StringIO()
            results = Path(tmpdir) / "fresh.jsonl"
            report = Path(tmpdir) / "fresh.report.json"
            with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
                result = main(
                    [
                        "fresh",
                        "--results",
                        str(results),
                        "--run-id",
                        "fresh-demo",
                        "--preflight-report-json",
                        str(report),
                        "--print-only",
                    ]
                )

        self.assertEqual(result, 2)
        self.assertIn("--preflight-report-json requires --preflight-only", stderr.getvalue())
        self.assertEqual(stdout.getvalue(), "")

    def test_fresh_mode_refuses_non_empty_evidence_before_harness(self) -> None:
        original_which = shutil.which
        shutil.which = lambda binary: f"/usr/bin/{binary}"
        try:
            with tempfile.TemporaryDirectory() as tmpdir:
                stdout = io.StringIO()
                stderr = io.StringIO()
                results = Path(tmpdir) / "fresh.jsonl"
                evidence = Path(tmpdir) / "fresh.demo-evidence.json"
                evidence.write_text('{"old": true}\n', encoding="utf-8")
                with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
                    result = main(
                        [
                            "fresh",
                            "--results",
                            str(results),
                            "--run-id",
                            "fresh-demo",
                            "--allow-dirty-source",
                            "--provider",
                            "local-test-provider/model",
                            "--evidence-json",
                            str(evidence),
                            "--confirm-provider-run",
                        ]
                    )
        finally:
            shutil.which = original_which

        self.assertEqual(result, 2)
        self.assertIn("fresh demo evidence path already contains data", stderr.getvalue())
        self.assertNotIn("bench/self_correction.py", stdout.getvalue())

    def test_fresh_results_refuses_non_empty_file_by_default(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            results = Path(tmpdir) / "existing.jsonl"
            results.write_text('{"old": true}\n', encoding="utf-8")

            with self.assertRaises(RuntimeError):
                ensure_fresh_results_path(results)

    def test_fresh_results_allows_empty_precreated_file(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            empty = Path(tmpdir) / "empty.jsonl"
            empty.touch()

            ensure_fresh_results_path(empty)

    def test_fresh_mode_requires_run_id(self) -> None:
        with self.assertRaises(SystemExit):
            parse_args(["fresh", "--results", "fresh.jsonl"])

    def test_validate_fresh_results_requires_current_run_and_budget_metadata(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            results = Path(tmpdir) / "fresh.jsonl"
            rows = [
                {
                    "run_id": "fresh-demo-1",
                    "source_head": "abcdef1234567890abcdef1234567890abcdef12",
                    "source_head_short": "abcdef1",
                    "source_branch": "main",
                    "source_dirty": False,
                    "max_tokens": 100_000,
                    "timeout_secs": 1800,
                    "no_external_solution_search": True,
                    "network_policy": "Isolated",
                    "audited_sandbox_provider_allowlist_enforced": True,
                    "audited_sandbox_provider_allowlist_status": "enforced",
                    FRESH_REQUIRED_SANDBOX_PROVIDER_ALLOWLIST_EVIDENCE_FIELD: (
                        self.fresh_sandbox_provider_allowlist_evidence()
                    ),
                },
                {
                    "run_id": "fresh-demo-2",
                    "source_head": "abcdef1234567890abcdef1234567890abcdef12",
                    "source_head_short": "abcdef1",
                    "source_branch": "main",
                    "source_dirty": False,
                    "max_tokens": 100_000,
                    "timeout_secs": 1800,
                    "no_external_solution_search": True,
                    "network_policy": "Isolated",
                    "audited_sandbox_provider_allowlist_enforced": True,
                    "audited_sandbox_provider_allowlist_status": "enforced",
                    FRESH_REQUIRED_SANDBOX_PROVIDER_ALLOWLIST_EVIDENCE_FIELD: (
                        self.fresh_sandbox_provider_allowlist_evidence()
                    ),
                },
            ]
            results.write_text(
                "".join(json.dumps(row) + "\n" for row in rows),
                encoding="utf-8",
            )
            args = argparse.Namespace(
                results=results,
                run_id="fresh-demo",
                allow_dirty_source=False,
                max_tokens=100_000,
                timeout=1800,
            )

            validate_fresh_results(args)

    def test_validate_fresh_results_accepts_senior_swe_bench_export_provenance(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            results = Path(tmpdir) / "fresh.jsonl"
            row = {
                "run_id": "fresh-demo-1",
                "benchmark_source": SENIOR_SWE_BENCH_SOURCE,
                "senior_swe_bench_export_sha256": "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890",
                "senior_swe_bench_export_row_index": 7,
                "source_head": "abcdef1234567890abcdef1234567890abcdef12",
                "source_head_short": "abcdef1",
                "source_branch": "main",
                "source_dirty": False,
                "max_tokens": 100_000,
                "timeout_secs": 1800,
                "no_external_solution_search": True,
                "network_policy": "Isolated",
                "audited_sandbox_provider_allowlist_enforced": True,
                "audited_sandbox_provider_allowlist_status": "enforced",
                FRESH_REQUIRED_SANDBOX_PROVIDER_ALLOWLIST_EVIDENCE_FIELD: (
                    self.fresh_sandbox_provider_allowlist_evidence()
                ),
            }
            results.write_text(json.dumps(row) + "\n", encoding="utf-8")
            args = argparse.Namespace(
                results=results,
                run_id="fresh-demo",
                allow_dirty_source=False,
                max_tokens=100_000,
                timeout=1800,
            )

            validate_fresh_results(args)

    def test_validate_fresh_results_rejects_senior_swe_bench_rows_without_export_provenance(self) -> None:
        base_row = {
            "run_id": "fresh-demo-1",
            "benchmark_source": SENIOR_SWE_BENCH_SOURCE,
            "source_head": "abcdef1234567890abcdef1234567890abcdef12",
            "source_head_short": "abcdef1",
            "source_branch": "main",
            "source_dirty": False,
            "max_tokens": 100_000,
            "timeout_secs": 1800,
            "no_external_solution_search": True,
            "network_policy": "Isolated",
            "audited_sandbox_provider_allowlist_enforced": True,
            "audited_sandbox_provider_allowlist_status": "enforced",
            FRESH_REQUIRED_SANDBOX_PROVIDER_ALLOWLIST_EVIDENCE_FIELD: (
                self.fresh_sandbox_provider_allowlist_evidence()
            ),
        }
        scenarios = [
            ({**base_row}, "senior_swe_bench_export_sha256"),
            (
                {
                    **base_row,
                    "senior_swe_bench_export_sha256": "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890",
                    "senior_swe_bench_export_row_index": 0,
                },
                "positive integer senior_swe_bench_export_row_index",
            ),
        ]
        for row, message in scenarios:
            with self.subTest(message=message), tempfile.TemporaryDirectory() as tmpdir:
                results = Path(tmpdir) / "fresh.jsonl"
                results.write_text(json.dumps(row) + "\n", encoding="utf-8")
                args = argparse.Namespace(
                    results=results,
                    run_id="fresh-demo",
                    allow_dirty_source=False,
                    max_tokens=100_000,
                    timeout=1800,
                )

                with self.assertRaisesRegex(RuntimeError, message):
                    validate_fresh_results(args)

    def test_validate_fresh_results_rejects_orphan_senior_swe_bench_export_provenance(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            results = Path(tmpdir) / "fresh.jsonl"
            row = {
                "run_id": "fresh-demo-1",
                "benchmark_source": "self",
                "senior_swe_bench_export_sha256": "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890",
                "senior_swe_bench_export_row_index": 1,
                "source_head": "abcdef1234567890abcdef1234567890abcdef12",
                "source_head_short": "abcdef1",
                "source_branch": "main",
                "source_dirty": False,
                "max_tokens": 100_000,
                "timeout_secs": 1800,
                "no_external_solution_search": True,
                "network_policy": "Isolated",
                "audited_sandbox_provider_allowlist_enforced": True,
                "audited_sandbox_provider_allowlist_status": "enforced",
                FRESH_REQUIRED_SANDBOX_PROVIDER_ALLOWLIST_EVIDENCE_FIELD: (
                    self.fresh_sandbox_provider_allowlist_evidence()
                ),
            }
            results.write_text(json.dumps(row) + "\n", encoding="utf-8")
            args = argparse.Namespace(
                results=results,
                run_id="fresh-demo",
                allow_dirty_source=False,
                max_tokens=100_000,
                timeout=1800,
            )

            with self.assertRaisesRegex(RuntimeError, "without benchmark_source"):
                validate_fresh_results(args)

    def test_validate_fresh_results_rejects_unreproducible_source_metadata(self) -> None:
        full_head = "abcdef1234567890abcdef1234567890abcdef12"
        base_row = {
            "run_id": "fresh-demo-1",
            "source_head": full_head,
            "source_head_short": "abcdef1",
            "source_branch": "main",
            "source_dirty": False,
            "max_tokens": 100_000,
            "timeout_secs": 1800,
            "no_external_solution_search": True,
            "network_policy": "Isolated",
            "audited_sandbox_provider_allowlist_enforced": True,
            "audited_sandbox_provider_allowlist_status": "enforced",
            FRESH_REQUIRED_SANDBOX_PROVIDER_ALLOWLIST_EVIDENCE_FIELD: (
                self.fresh_sandbox_provider_allowlist_evidence()
            ),
        }
        scenarios = [
            (
                [{**base_row, "source_head": "123456", "source_head_short": "123456"}],
                "invalid source_head",
            ),
            (
                [
                    base_row,
                    {
                        **base_row,
                        "run_id": "fresh-demo-2",
                        "source_head": "1234567890abcdef1234567890abcdef12345678",
                        "source_head_short": "1234567",
                    },
                ],
                "source metadata differs",
            ),
        ]
        for rows, message in scenarios:
            with self.subTest(message=message), tempfile.TemporaryDirectory() as tmpdir:
                results = Path(tmpdir) / "fresh.jsonl"
                results.write_text(
                    "".join(json.dumps(row) + "\n" for row in rows),
                    encoding="utf-8",
                )
                args = argparse.Namespace(
                    results=results,
                    run_id="fresh-demo",
                    allow_dirty_source=False,
                    max_tokens=100_000,
                    timeout=1800,
                )

                with self.assertRaisesRegex(RuntimeError, message):
                    validate_fresh_results(args)

    def test_validate_fresh_results_rejects_host_path_markers_in_jsonl(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            results = Path(tmpdir) / "fresh.jsonl"
            row = {
                "run_id": "fresh-demo-1",
                "source_head": "abcdef1234567890abcdef1234567890abcdef12",
                "source_head_short": "abcdef1",
                "source_branch": "main",
                "source_dirty": False,
                "max_tokens": 100_000,
                "timeout_secs": 1800,
                "no_external_solution_search": True,
                "network_policy": "Isolated",
                "audited_sandbox_provider_allowlist_enforced": True,
                "audited_sandbox_provider_allowlist_status": "enforced",
                FRESH_REQUIRED_SANDBOX_PROVIDER_ALLOWLIST_EVIDENCE_FIELD: (
                    self.fresh_sandbox_provider_allowlist_evidence()
                ),
                "stdout": "tool output leaked /Users/example/project/file.rs",
            }
            results.write_text(json.dumps(row) + "\n", encoding="utf-8")
            args = argparse.Namespace(
                results=results,
                run_id="fresh-demo",
                allow_dirty_source=False,
                max_tokens=100_000,
                timeout=1800,
            )

            with self.assertRaisesRegex(RuntimeError, "host-specific path marker"):
                validate_fresh_results(args)

    def test_validate_fresh_results_rejects_gated_promotion_without_matching_artifact(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            results = Path(tmpdir) / "fresh.jsonl"
            row = {
                "run_id": "fresh-demo-1",
                "task_id": "task",
                "attempt": 2,
                "resolved": True,
                "verify_returncode": 0,
                "verify_command": "cargo test -p demo hidden",
                "lineage_records_after": 2,
                "lineage_reconciled_by_core": True,
                "source_head": "abcdef1234567890abcdef1234567890abcdef12",
                "source_head_short": "abcdef1",
                "source_branch": "main",
                "source_dirty": False,
                "max_tokens": 100_000,
                "timeout_secs": 1800,
                "no_external_solution_search": True,
                "network_policy": "Isolated",
                "audited_sandbox_provider_allowlist_enforced": True,
                "audited_sandbox_provider_allowlist_status": "enforced",
                FRESH_REQUIRED_SANDBOX_PROVIDER_ALLOWLIST_EVIDENCE_FIELD: (
                    self.fresh_sandbox_provider_allowlist_evidence()
                ),
                "promotion": {
                    "verifier_gated": True,
                    "evidence_present": True,
                    "lineage_reconciled_by_core": True,
                    "verify_returncode": 0,
                },
            }
            results.write_text(json.dumps(row) + "\n", encoding="utf-8")
            args = argparse.Namespace(
                results=results,
                run_id="fresh-demo",
                allow_dirty_source=False,
                max_tokens=100_000,
                timeout=1800,
            )

            with self.assertRaisesRegex(RuntimeError, "without a matching promotion artifact"):
                validate_fresh_results(args)

    def test_validate_fresh_results_rejects_stale_or_mismatched_rows(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            results = Path(tmpdir) / "fresh.jsonl"
            results.write_text(
                json.dumps(
                    {
                        "run_id": "old-demo-1",
                        "source_head": "abcdef1234567890abcdef1234567890abcdef12",
                        "source_head_short": "abcdef1",
                        "source_branch": "main",
                        "source_dirty": False,
                        "max_tokens": 100_000,
                        "timeout_secs": 1800,
                        "no_external_solution_search": True,
                        "network_policy": "Isolated",
                        "audited_sandbox_provider_allowlist_enforced": True,
                        "audited_sandbox_provider_allowlist_status": "enforced",
                        FRESH_REQUIRED_SANDBOX_PROVIDER_ALLOWLIST_EVIDENCE_FIELD: (
                            self.fresh_sandbox_provider_allowlist_evidence()
                        ),
                    }
                )
                + "\n",
                encoding="utf-8",
            )
            args = argparse.Namespace(
                results=results,
                run_id="fresh-demo",
                allow_dirty_source=False,
                max_tokens=100_000,
                timeout=1800,
            )

            with self.assertRaises(RuntimeError):
                validate_fresh_results(args)

    def test_validate_fresh_results_rejects_same_prefix_non_numeric_suffix(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            results = Path(tmpdir) / "fresh.jsonl"
            results.write_text(
                json.dumps(
                    {
                        "run_id": "fresh-demo-old",
                        "source_head": "abcdef1234567890abcdef1234567890abcdef12",
                        "source_head_short": "abcdef1",
                        "source_branch": "main",
                        "source_dirty": False,
                        "max_tokens": 100_000,
                        "timeout_secs": 1800,
                        "no_external_solution_search": True,
                        "network_policy": "Isolated",
                        "audited_sandbox_provider_allowlist_enforced": True,
                        "audited_sandbox_provider_allowlist_status": "enforced",
                        FRESH_REQUIRED_SANDBOX_PROVIDER_ALLOWLIST_EVIDENCE_FIELD: (
                            self.fresh_sandbox_provider_allowlist_evidence()
                        ),
                    }
                )
                + "\n",
                encoding="utf-8",
            )
            args = argparse.Namespace(
                results=results,
                run_id="fresh-demo",
                allow_dirty_source=False,
                max_tokens=100_000,
                timeout=1800,
            )

            with self.assertRaises(RuntimeError):
                validate_fresh_results(args)

    def test_validate_fresh_results_rejects_empty_output(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            results = Path(tmpdir) / "fresh.jsonl"
            results.touch()
            args = argparse.Namespace(
                results=results,
                run_id="fresh-demo",
                allow_dirty_source=False,
                max_tokens=100_000,
                timeout=1800,
            )

            with self.assertRaises(RuntimeError):
                validate_fresh_results(args)

    def test_validate_fresh_results_rejects_invalid_jsonl(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            results = Path(tmpdir) / "fresh.jsonl"
            results.write_text("not json\n", encoding="utf-8")
            args = argparse.Namespace(
                results=results,
                run_id="fresh-demo",
                allow_dirty_source=False,
                max_tokens=100_000,
                timeout=1800,
            )

            with self.assertRaises(RuntimeError):
                validate_fresh_results(args)

    def test_validate_fresh_results_rejects_missing_audit_fields(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            results = Path(tmpdir) / "fresh.jsonl"
            results.write_text(
                json.dumps({"run_id": "fresh-demo-1", "source_head": "abcdef"}) + "\n",
                encoding="utf-8",
            )
            args = argparse.Namespace(
                results=results,
                run_id="fresh-demo",
                allow_dirty_source=False,
                max_tokens=100_000,
                timeout=1800,
            )

            with self.assertRaises(RuntimeError):
                validate_fresh_results(args)

    def test_validate_fresh_results_rejects_missing_sandbox_allowlist_audit(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            results = Path(tmpdir) / "fresh.jsonl"
            results.write_text(
                json.dumps(
                    {
                        "run_id": "fresh-demo-1",
                        "source_head": "abcdef1234567890abcdef1234567890abcdef12",
                        "source_head_short": "abcdef1",
                        "source_branch": "main",
                        "source_dirty": False,
                        "max_tokens": 100_000,
                        "timeout_secs": 1800,
                        "no_external_solution_search": True,
                        "network_policy": "Isolated",
                    }
                )
                + "\n",
                encoding="utf-8",
            )
            args = argparse.Namespace(
                results=results,
                run_id="fresh-demo",
                allow_dirty_source=False,
                max_tokens=100_000,
                timeout=1800,
            )

            with self.assertRaisesRegex(
                RuntimeError, "audited_sandbox_provider_allowlist_enforced"
            ):
                validate_fresh_results(args)

    def test_validate_fresh_results_rejects_malformed_sandbox_allowlist_audit(self) -> None:
        base_row = {
            "run_id": "fresh-demo-1",
            "source_head": "abcdef1234567890abcdef1234567890abcdef12",
            "source_head_short": "abcdef1",
            "source_branch": "main",
            "source_dirty": False,
            "max_tokens": 100_000,
            "timeout_secs": 1800,
            "no_external_solution_search": True,
            "network_policy": "Isolated",
            "audited_sandbox_provider_allowlist_enforced": True,
            "audited_sandbox_provider_allowlist_status": "enforced",
            "audited_sandbox_provider_allowlist_evidence": self.fresh_sandbox_provider_allowlist_evidence(),
        }
        malformed_cases = {
            "audited_sandbox_provider_allowlist_enforced": "true",
            "audited_sandbox_provider_allowlist_status": [],
            "audited_sandbox_provider_allowlist_evidence": "not-a-map",
        }
        args = argparse.Namespace(
            run_id="fresh-demo",
            allow_dirty_source=False,
            max_tokens=100_000,
            timeout=1800,
        )
        for field, malformed in malformed_cases.items():
            with self.subTest(field=field), tempfile.TemporaryDirectory() as tmpdir:
                results = Path(tmpdir) / "fresh.jsonl"
                row = dict(base_row)
                row[field] = malformed
                results.write_text(json.dumps(row) + "\n", encoding="utf-8")
                args.results = results
                with self.assertRaises(RuntimeError):
                    validate_fresh_results(args)

    def test_validate_fresh_results_rejects_unenforced_sandbox_allowlist_audit(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            results = Path(tmpdir) / "fresh.jsonl"
            row = {
                "run_id": "fresh-demo-1",
                "source_head": "abcdef1234567890abcdef1234567890abcdef12",
                "source_head_short": "abcdef1",
                "source_branch": "main",
                "source_dirty": False,
                "max_tokens": 100_000,
                "timeout_secs": 1800,
                "no_external_solution_search": True,
                "network_policy": "Isolated",
                "audited_sandbox_provider_allowlist_enforced": False,
                "audited_sandbox_provider_allowlist_status": "not_implemented",
            }
            results.write_text(json.dumps(row) + "\n", encoding="utf-8")
            args = argparse.Namespace(
                results=results,
                run_id="fresh-demo",
                allow_dirty_source=False,
                max_tokens=100_000,
                timeout=1800,
            )

            with self.assertRaisesRegex(
                RuntimeError, "must record audited sandbox/provider allowlist enforcement"
            ):
                validate_fresh_results(args)

    def test_validate_fresh_results_rejects_sandbox_allowlist_status_mismatch(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            results = Path(tmpdir) / "fresh.jsonl"
            row = {
                "run_id": "fresh-demo-1",
                "source_head": "abcdef1234567890abcdef1234567890abcdef12",
                "source_head_short": "abcdef1",
                "source_branch": "main",
                "source_dirty": False,
                "max_tokens": 100_000,
                "timeout_secs": 1800,
                "no_external_solution_search": True,
                "network_policy": "Isolated",
                "audited_sandbox_provider_allowlist_enforced": True,
                "audited_sandbox_provider_allowlist_status": "wired",
            }
            results.write_text(json.dumps(row) + "\n", encoding="utf-8")
            args = argparse.Namespace(
                results=results,
                run_id="fresh-demo",
                allow_dirty_source=False,
                max_tokens=100_000,
                timeout=1800,
            )

            with self.assertRaisesRegex(RuntimeError, "must record status='enforced'"):
                validate_fresh_results(args)

    def test_validate_fresh_results_rejects_missing_sandbox_allowlist_evidence(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            results = Path(tmpdir) / "fresh.jsonl"
            row = {
                "run_id": "fresh-demo-1",
                "source_head": "abcdef1234567890abcdef1234567890abcdef12",
                "source_head_short": "abcdef1",
                "source_branch": "main",
                "source_dirty": False,
                "max_tokens": 100_000,
                "timeout_secs": 1800,
                "no_external_solution_search": True,
                "network_policy": "Isolated",
                "audited_sandbox_provider_allowlist_enforced": True,
                "audited_sandbox_provider_allowlist_status": "enforced",
            }
            results.write_text(json.dumps(row) + "\n", encoding="utf-8")
            args = argparse.Namespace(
                results=results,
                run_id="fresh-demo",
                allow_dirty_source=False,
                max_tokens=100_000,
                timeout=1800,
            )

            with self.assertRaisesRegex(
                RuntimeError, "without audited_sandbox_provider_allowlist_evidence evidence"
            ):
                validate_fresh_results(args)

    def test_validate_fresh_results_rejects_incomplete_sandbox_allowlist_evidence(self) -> None:
        base_row = {
            "run_id": "fresh-demo-1",
            "source_head": "abcdef1234567890abcdef1234567890abcdef12",
            "source_head_short": "abcdef1",
            "source_branch": "main",
            "source_dirty": False,
            "max_tokens": 100_000,
            "timeout_secs": 1800,
            "no_external_solution_search": True,
            "network_policy": "Isolated",
            "audited_sandbox_provider_allowlist_enforced": True,
            "audited_sandbox_provider_allowlist_status": "enforced",
        }
        scenarios = [
            (
                {**self.fresh_sandbox_provider_allowlist_evidence(), "provider_endpoint_allowlist_enforced": False},
                "provider_endpoint_allowlist_enforced=true",
            ),
            (
                {**self.fresh_sandbox_provider_allowlist_evidence(), "blocked_solution_hosts": ["example.com"]},
                "github.com as blocked",
            ),
            (
                {**self.fresh_sandbox_provider_allowlist_evidence(), "sandbox_profile_sha256": "not-a-sha"},
                "durable sandbox runtime or profile hash",
            ),
            (
                {k: v for k, v in self.fresh_sandbox_provider_allowlist_evidence().items() if k != "sandbox_profile_lines"},
                "sandbox_profile_lines",
            ),
            (
                {**self.fresh_sandbox_provider_allowlist_evidence(), "sandbox_profile_lines": ["(version 1)"]},
                "sandbox_profile_lines must match sandbox_profile_sha256",
            ),
            (
                {
                    **self.fresh_sandbox_provider_allowlist_evidence(),
                    "sandbox_profile_sha256": hashlib.sha256(
                        "\n".join(["(version 1)", "(allow default)"]).encode("utf-8") + b"\n"
                    ).hexdigest(),
                    "sandbox_profile_lines": ["(version 1)", "(allow default)"],
                },
                "deny network by default",
            ),
            (
                {
                    **self.fresh_sandbox_provider_allowlist_evidence(),
                    "sandbox_profile_sha256": hashlib.sha256(
                        "\n".join(["(version 1)", "(allow default)", "(deny network*)"]).encode("utf-8") + b"\n"
                    ).hexdigest(),
                    "sandbox_profile_lines": ["(version 1)", "(allow default)", "(deny network*)"],
                },
                "allowed provider endpoint hosts",
            ),
            (
                {
                    **self.fresh_sandbox_provider_allowlist_evidence(),
                    "sandbox_profile_sha256": hashlib.sha256(
                        "\n".join([
                            "(version 1)",
                            "(allow default)",
                            "(deny network*)",
                            "; comment mentions api.openai.com but does not allow it",
                        ]).encode("utf-8") + b"\n"
                    ).hexdigest(),
                    "sandbox_profile_lines": [
                        "(version 1)",
                        "(allow default)",
                        "(deny network*)",
                        "; comment mentions api.openai.com but does not allow it",
                    ],
                },
                "allowed provider endpoint hosts",
            ),
            (
                {
                    **self.fresh_sandbox_provider_allowlist_evidence(),
                    "sandbox_profile_sha256": hashlib.sha256(
                        "\n".join([
                            "(version 1)",
                            "(allow default)",
                            "(deny network*)",
                            '(allow network-outbound (remote tcp "api.openai.com:443"))',
                            '(allow network-outbound (remote tcp "github.com:443"))',
                        ]).encode("utf-8") + b"\n"
                    ).hexdigest(),
                    "sandbox_profile_lines": [
                        "(version 1)",
                        "(allow default)",
                        "(deny network*)",
                        '(allow network-outbound (remote tcp "api.openai.com:443"))',
                        '(allow network-outbound (remote tcp "github.com:443"))',
                    ],
                },
                "cannot allow blocked solution hosts",
            ),
            (
                {**self.fresh_sandbox_provider_allowlist_evidence(), "allowed_provider_endpoints": ["https://github.com"]},
                "allows blocked solution host",
            ),
            (
                {**self.fresh_sandbox_provider_allowlist_evidence(), "allowed_provider_endpoints": ["https://api.example-provider.invalid"]},
                "real provider endpoints",
            ),
            (
                {**self.fresh_sandbox_provider_allowlist_evidence(), "allowed_provider_endpoints": ["https://localhost:1234"]},
                "real provider endpoints",
            ),
            (
                {**self.fresh_sandbox_provider_allowlist_evidence(), "allowed_provider_endpoints": ["https://example.com"]},
                "real provider endpoints",
            ),
            (
                {**self.fresh_sandbox_provider_allowlist_evidence(), "allowed_provider_endpoints": ["https://provider.test"]},
                "real provider endpoints",
            ),
            (
                {**self.fresh_sandbox_provider_allowlist_evidence(), "allowed_provider_endpoints": ["https://192.168.0.10"]},
                "real provider endpoints",
            ),
            (
                {**self.fresh_sandbox_provider_allowlist_evidence(), "allowed_provider_endpoints": ["https://169.254.1.1"]},
                "real provider endpoints",
            ),
            (
                {**self.fresh_sandbox_provider_allowlist_evidence(), "allowed_provider_endpoints": ["https://not a host"]},
                "real provider endpoints",
            ),
        ]
        for evidence, message in scenarios:
            with self.subTest(message=message), tempfile.TemporaryDirectory() as tmpdir:
                results = Path(tmpdir) / "fresh.jsonl"
                row = {
                    **base_row,
                    FRESH_REQUIRED_SANDBOX_PROVIDER_ALLOWLIST_EVIDENCE_FIELD: evidence,
                }
                results.write_text(json.dumps(row) + "\n", encoding="utf-8")
                args = argparse.Namespace(
                    results=results,
                    run_id="fresh-demo",
                    allow_dirty_source=False,
                    max_tokens=100_000,
                    timeout=1800,
                )

                with self.assertRaisesRegex(RuntimeError, message):
                    validate_fresh_results(args)

    def test_validate_fresh_results_ignores_sandbox_profile_comment_hosts(self) -> None:
        profile_lines = [
            *TEST_SANDBOX_PROFILE_LINES,
            "; audit note: do not allow github.com or raw.githubusercontent.com",
            "# blocked hosts include github.com",
        ]
        evidence = {
            **self.fresh_sandbox_provider_allowlist_evidence(),
            "sandbox_profile_sha256": hashlib.sha256(
                ("\n".join(profile_lines) + "\n").encode("utf-8")
            ).hexdigest(),
            "sandbox_profile_lines": profile_lines,
        }
        validate_fresh_sandbox_provider_allowlist_evidence(
            {
                "audited_sandbox_provider_allowlist_evidence": evidence,
            },
            index=1,
        )

    def test_validate_fresh_results_rejects_dirty_source(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            results = Path(tmpdir) / "fresh.jsonl"
            results.write_text(
                json.dumps(
                    {
                        "run_id": "fresh-demo-1",
                        "source_head": "abcdef1234567890abcdef1234567890abcdef12",
                        "source_head_short": "abcdef1",
                        "source_branch": "main",
                        "source_dirty": True,
                        "max_tokens": 100_000,
                        "timeout_secs": 1800,
                        "no_external_solution_search": True,
                        "network_policy": "Isolated",
                        "audited_sandbox_provider_allowlist_enforced": True,
                        "audited_sandbox_provider_allowlist_status": "enforced",
                        FRESH_REQUIRED_SANDBOX_PROVIDER_ALLOWLIST_EVIDENCE_FIELD: (
                            self.fresh_sandbox_provider_allowlist_evidence()
                        ),
                    }
                )
                + "\n",
                encoding="utf-8",
            )
            args = argparse.Namespace(
                results=results,
                run_id="fresh-demo",
                allow_dirty_source=False,
                max_tokens=100_000,
                timeout=1800,
            )

            with self.assertRaises(RuntimeError):
                validate_fresh_results(args)

    def test_validate_fresh_results_rejects_budget_mismatch(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            results = Path(tmpdir) / "fresh.jsonl"
            results.write_text(
                json.dumps(
                    {
                        "run_id": "fresh-demo-1",
                        "source_head": "abcdef1234567890abcdef1234567890abcdef12",
                        "source_head_short": "abcdef1",
                        "source_branch": "main",
                        "source_dirty": False,
                        "max_tokens": 99_999,
                        "timeout_secs": 1800,
                        "no_external_solution_search": True,
                        "network_policy": "Isolated",
                        "audited_sandbox_provider_allowlist_enforced": True,
                        "audited_sandbox_provider_allowlist_status": "enforced",
                        FRESH_REQUIRED_SANDBOX_PROVIDER_ALLOWLIST_EVIDENCE_FIELD: (
                            self.fresh_sandbox_provider_allowlist_evidence()
                        ),
                    }
                )
                + "\n",
                encoding="utf-8",
            )
            args = argparse.Namespace(
                results=results,
                run_id="fresh-demo",
                allow_dirty_source=False,
                max_tokens=100_000,
                timeout=1800,
            )

            with self.assertRaises(RuntimeError):
                validate_fresh_results(args)


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
