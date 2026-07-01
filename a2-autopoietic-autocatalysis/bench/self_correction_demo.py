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
import io
import json
import shutil
import shlex
import subprocess
import sys
import tempfile
import unittest
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
HOST_PATH_MARKERS = ("/Users", "/tmp", "/var/folders")
EXPECTED_DEMO_REQUIREMENTS = [
    "failed_first_attempt",
    "archived_verifier_failure_evidence",
    "retry_context_from_failure_evidence",
    "later_passing_attempt",
    "lineage_trajectory_recorded",
    "verifier_gated_germline_promotion",
]


def repo_root() -> Path:
    return Path(__file__).resolve().parents[1]


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


def ensure_clean_source() -> None:
    status = subprocess.run(
        ["git", "status", "--porcelain", "--", "."],
        cwd=repo_root(),
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    if status.returncode != 0:
        raise RuntimeError(f"could not inspect source cleanliness: {status.stderr.strip()}")
    if status.stdout.strip():
        raise RuntimeError(
            "fresh demo source tree is dirty; commit/stash changes or pass --allow-dirty-source"
        )


def fresh_preflight(args: argparse.Namespace, evidence_json: Path) -> None:
    ensure_fresh_results_path(args.results)
    ensure_fresh_evidence_path(evidence_json)
    ensure_provider_binary(args.provider)
    ensure_provider_config(args.provider)
    if not args.allow_dirty_source:
        ensure_clean_source()


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


def fresh_preflight_report(args: argparse.Namespace, evidence_json: Path) -> dict[str, object]:
    config_checked = provider_config_checked(args.provider)
    return {
        "mode": "fresh_preflight",
        "creates_loop_evidence": False,
        "live_provider_auth_quota_model_checked": False,
        "results": str(args.results),
        "evidence_json": str(evidence_json),
        "preflight_report_json": str(args.preflight_report_json),
        "fixture": args.fixture,
        "provider": args.provider,
        "run_id": args.run_id,
        "runs": args.runs,
        "attempts": args.attempts,
        "max_tokens": args.max_tokens,
        "timeout_secs": args.timeout,
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
            "source_clean": None if args.allow_dirty_source else True,
            "source_clean_checked_before_output_creation": None
            if args.allow_dirty_source
            else True,
            "dirty_source_allowed": args.allow_dirty_source,
        },
        "commands": {
            "harness": display_command(fresh_command(args)),
            "validation": fresh_validation_summary(args),
            "scorer": display_command(score_command(args.results, evidence_json)),
            "fresh_provenance_contract": display_command(
                fresh_contract_command(args, evidence_json)
            ),
        },
        "notes": [
            "No provider-backed benchmark was executed by this preflight.",
            "Live provider auth, quota, and model availability are not verified until the fresh run executes.",
            "Clean-source readiness is checked before fresh results/evidence files are created; newly generated rows record that pre-run source state, and the new artifacts must then be archived deliberately.",
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
            )
            if key not in row
        ]
        if missing:
            raise RuntimeError(
                f"fresh demo row {index} is missing audit field(s): {', '.join(missing)}"
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
        f"local provider config present when supported; {source_check}. "
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
    if value is None:
        return None
    try:
        return int(value)  # type: ignore[arg-type]
    except (TypeError, ValueError):
        return None


def optional_bool_value(value: object) -> bool | None:
    if value is True or value is False:
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


def normalized_artifact_row(row: dict[str, object]) -> dict[str, object]:
    promotion = artifact_promotion(row)
    return {
        "run_id": str(row.get("run_id") or ""),
        "task_id": str(row.get("task_id") or ""),
        "attempt": max(int(row.get("attempt") or 1), 1),
        "resolved": bool(row.get("resolved")),
        "prior_lineage_present": bool(row.get("prior_lineage_present")),
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
        "lineage_reconciled_by_core": bool(row["lineage_reconciled_by_core"])
        if "lineage_reconciled_by_core" in row
        else None,
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


def selector_tuple(selector: dict[str, object], *, label: str) -> tuple[str, str, int]:
    run_id = selector.get("run_id")
    task_id = selector.get("task_id")
    attempt = selector.get("attempt")
    if not isinstance(run_id, str) or not isinstance(task_id, str) or not isinstance(attempt, int):
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


def require_embedded_row_matches_artifact(
    step: dict[str, object],
    artifact_row: dict[str, object],
    *,
    label: str,
) -> dict[str, object]:
    embedded = require_mapping(step.get("evidence_row"), label=f"{label}.evidence_row")
    expected = normalized_artifact_row(artifact_row)
    if embedded != expected:
        raise RuntimeError(f"demo evidence contract embedded row differs from artifact at {label}")
    return embedded


def validate_demo_evidence_contract(
    evidence: dict[str, object],
    reference: dict[str, object],
    *,
    evidence_label: str,
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
    artifact_sha256 = evidence.get("artifact_sha256")
    if not isinstance(artifact_sha256, str) or len(artifact_sha256) != 64:
        raise RuntimeError("demo evidence contract requires a 64-character artifact_sha256")
    if artifact_sha256 != sha256_file(artifact_path):
        raise RuntimeError("demo evidence contract artifact_sha256 does not match artifact bytes")
    artifact_rows = load_jsonl_rows(artifact_path)
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
        failed_verify = failed_fields.get("verify_returncode")
        if not isinstance(failed_verify, int) or failed_verify == 0:
            raise RuntimeError("demo evidence contract first attempt lacks verifier failure")
        if failed_embedded_row.get("verify_returncode") != failed_verify:
            raise RuntimeError("demo evidence contract failed verifier status differs from artifact")
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
        if archived_embedded_row.get("lineage_records_before") != archived_fields.get("lineage_records_before"):
            raise RuntimeError("demo evidence contract archived lineage start differs from artifact")
        if archived_embedded_row.get("lineage_records_after") != archived_fields.get("lineage_records_after"):
            raise RuntimeError("demo evidence contract archived lineage end differs from artifact")
        archived_before = archived_embedded_row.get("lineage_records_before")
        archived_after = archived_embedded_row.get("lineage_records_after")
        if not isinstance(archived_before, int) or not isinstance(archived_after, int) or archived_after <= archived_before:
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
        if not retry_fields or len(retry_fields) != len(retry_selectors):
            raise RuntimeError("demo evidence contract requires paired retry selectors and fields")
        failed_lineage_after = archived_fields.get("lineage_records_after")
        if not isinstance(failed_lineage_after, int):
            raise RuntimeError("demo evidence contract archived failure lacks lineage_records_after")
        retry_attempts: set[int] = set()
        for field_index, field_value in enumerate(retry_fields):
            retry_selector = require_mapping(
                retry_selectors[field_index],
                label=f"demos[{demo_index}].retry_context_from_failure_evidence.selectors[{field_index}]",
            )
            if retry_selector.get("run_id") != run_id or retry_selector.get("task_id") != task_id:
                raise RuntimeError("demo evidence contract retry selector differs from failed run/task")
            retry_attempt = retry_selector.get("attempt")
            if not isinstance(retry_attempt, int) or retry_attempt <= failed_attempt:
                raise RuntimeError("demo evidence contract retry attempt does not follow failure")
            retry_attempts.add(retry_attempt)
            retry_row = require_artifact_row(
                artifact_index,
                retry_selector,
                label=f"demos[{demo_index}].retry_context_from_failure_evidence.selectors[{field_index}]",
            )
            retry_embedded_rows = require_sequence(
                retry_step.get("evidence_rows"),
                label=f"demos[{demo_index}].retry_context_from_failure_evidence.evidence_rows",
            )
            if field_index >= len(retry_embedded_rows):
                raise RuntimeError("demo evidence contract missing embedded retry row")
            retry_embedded_row = require_mapping(
                retry_embedded_rows[field_index],
                label=f"demos[{demo_index}].retry_context_from_failure_evidence.evidence_rows[{field_index}]",
            )
            if retry_embedded_row != normalized_artifact_row(retry_row):
                raise RuntimeError("demo evidence contract embedded retry row differs from artifact")
            field = require_mapping(
                field_value,
                label=f"demos[{demo_index}].retry_context_from_failure_evidence.fields[{field_index}]",
            )
            if field.get("failed_attempt_selector") != failed_selector:
                raise RuntimeError("demo evidence contract retry is not tied to failed selector")
            if field.get("failed_verify_returncode") != failed_verify:
                raise RuntimeError("demo evidence contract retry does not carry failed verifier status")
            if field.get("failed_verify_command") != failed_fields.get("verify_command"):
                raise RuntimeError("demo evidence contract retry does not carry failed verifier command")
            if field.get("failed_lineage_records_after") != failed_lineage_after:
                raise RuntimeError("demo evidence contract retry does not carry failed lineage boundary")
            lineage_before = field.get("lineage_records_before")
            if not isinstance(lineage_before, int) or lineage_before < failed_lineage_after:
                raise RuntimeError("demo evidence contract retry lineage predates archived failure")
            if retry_embedded_row.get("prior_lineage_present") is not True:
                raise RuntimeError("demo evidence contract retry artifact row lacks prior lineage")
            if retry_embedded_row.get("lineage_records_before") != lineage_before:
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
        later_attempt = later_selector.get("attempt")
        if not isinstance(later_attempt, int) or later_attempt not in retry_attempts:
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
        if later_fields.get("resolved") is not True or later_fields.get("verify_returncode") != 0:
            raise RuntimeError("demo evidence contract later attempt is not verifier-passing")
        if later_embedded_row.get("resolved") is not True or later_embedded_row.get("verify_returncode") != 0:
            raise RuntimeError("demo evidence contract later artifact row is not verifier-passing")
        lineage_step = require_mapping(
            chain[reference_requirements.index("lineage_trajectory_recorded")],
            label=f"demos[{demo_index}].lineage_trajectory_recorded",
        )
        lineage_fields = require_mapping(
            lineage_step.get("fields"),
            label=f"demos[{demo_index}].lineage_trajectory_recorded.fields",
        )
        before = lineage_fields.get("lineage_records_before")
        after = lineage_fields.get("lineage_records_after")
        if not isinstance(before, int) or not isinstance(after, int) or after <= before:
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
            if lineage_embedded_row != normalized_artifact_row(lineage_artifact_row):
                raise RuntimeError("demo evidence contract lineage row differs from artifact")
            if lineage_embedded_row.get("run_id") != run_id or lineage_embedded_row.get("task_id") != task_id:
                raise RuntimeError("demo evidence contract lineage row differs from failed run/task")
            lineage_attempt = lineage_embedded_row.get("attempt")
            if not isinstance(lineage_attempt, int):
                raise RuntimeError("demo evidence contract lineage row lacks attempt")
            lineage_attempts.append(lineage_attempt)
        if lineage_attempts != lineage_fields.get("attempts"):
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
        if promotion_fields.get("verify_returncode") != promotion_embedded_row.get("verify_returncode"):
            raise RuntimeError("demo evidence contract promotion verifier status differs from artifact")
        if promotion_fields.get("verify_returncode") != 0:
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
            and promotion_embedded_row.get("promotion_verify_returncode") == 0
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
        retry_fields = [
            require_mapping(field, label=f"demos[{demo_index - 1}].retry.fields")
            for field in require_sequence(
                steps["retry_context_from_failure_evidence"].get("fields"),
                label=f"demos[{demo_index - 1}].retry_context_from_failure_evidence.fields",
            )
        ]
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
                f"    archived_verifier_failure_evidence: source={artifact}; {selector_summary(failed)}; lineage={steps['archived_verifier_failure_evidence']['fields']['lineage_records_before']}->{steps['archived_verifier_failure_evidence']['fields']['lineage_records_after']}",
                "    retry_context_from_failure_evidence: source="
                f"{artifact}; selectors=["
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
) -> None:
    evidence = load_evidence_json(evidence_json)
    reference = load_evidence_json(reference_evidence_json)
    validate_demo_evidence_contract(
        evidence,
        reference,
        evidence_label=str(evidence_json),
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
    print("Demo evidence contract check")
    print(f"  evidence: {evidence_json}")
    print(f"  reference: {reference_evidence_json}")
    print(
        "  PASS evidence JSON matches archived demo contract "
        f"(requirements={len(evidence['requirements'])}, demos={len(evidence['demos'])})"
    )
    if fresh_run_id is not None:
        print(
            "  PASS fresh artifact provenance "
            f"(run_id={fresh_run_id!r}, max_tokens={max_tokens}, timeout_secs={timeout_secs})"
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
        result = run_command(
            score_command(args.archive, evidence_json), print_only=args.print_only
        )
        if result != 0 or args.print_only or evidence_json is None:
            return result
        try:
            verify_evidence_contract(evidence_json, DEFAULT_ARCHIVE_EVIDENCE)
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
        if args.preflight_only:
            try:
                fresh_preflight(args, evidence_json)
                if args.preflight_report_json:
                    write_fresh_preflight_report(
                        args.preflight_report_json,
                        fresh_preflight_report(args, evidence_json),
                        results=args.results,
                        evidence_json=evidence_json,
                    )
            except RuntimeError as exc:
                print(f"error: {exc}", file=sys.stderr)
                return 2
            print(fresh_preflight_summary(args))
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
                fresh_preflight(args, evidence_json)
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
            verify_evidence_contract(
                evidence_json,
                DEFAULT_ARCHIVE_EVIDENCE,
                fresh_run_id=args.run_id,
                max_tokens=args.max_tokens,
                timeout_secs=args.timeout,
                allow_dirty_source=args.allow_dirty_source,
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

    def test_verify_archive_runs_evidence_contract_after_successful_score(self) -> None:
        with mock.patch(__name__ + ".run_command", return_value=0) as run, mock.patch(
            __name__ + ".verify_evidence_contract"
        ) as contract:
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
        run.assert_called_once()
        contract.assert_called_once_with(
            Path("custom.demo-evidence.json"), DEFAULT_ARCHIVE_EVIDENCE
        )

    def test_verify_archive_skips_evidence_contract_when_scoring_fails(self) -> None:
        with mock.patch(__name__ + ".run_command", return_value=1), mock.patch(
            __name__ + ".verify_evidence_contract"
        ) as contract:
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

    def test_fresh_runs_evidence_contract_after_confirmed_successful_score(self) -> None:
        with mock.patch(__name__ + ".fresh_preflight"), mock.patch(
            __name__ + ".run_command", side_effect=[0, 0]
        ) as run, mock.patch(__name__ + ".validate_fresh_results"), mock.patch(
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
        contract.assert_called_once_with(
            Path("docs/benchmark-results/self-correction/a2-fresh-demo.demo-evidence.json"),
            DEFAULT_ARCHIVE_EVIDENCE,
            fresh_run_id="fresh-demo",
            max_tokens=100_000,
            timeout_secs=1800,
            allow_dirty_source=False,
        )

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

    def test_verify_evidence_contract_accepts_complete_six_step_demo(self) -> None:
        evidence = self.archived_demo_contract_evidence()

        validate_demo_evidence_contract(
            evidence,
            self.evidence_reference(evidence),
            evidence_label=str(DEFAULT_ARCHIVE_EVIDENCE),
        )

    def test_verify_evidence_contract_prints_concrete_artifact_selectors(self) -> None:
        stdout = io.StringIO()

        with contextlib.redirect_stdout(stdout):
            verify_evidence_contract(DEFAULT_ARCHIVE_EVIDENCE, DEFAULT_ARCHIVE_EVIDENCE)

        output = stdout.getvalue()
        self.assertIn(str(DEFAULT_ARCHIVE), output)
        self.assertIn("failed_first_attempt: source=", output)
        self.assertIn("archived_verifier_failure_evidence: source=", output)
        self.assertIn("retry_context_from_failure_evidence: source=", output)
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

    def test_verify_evidence_contract_rejects_artifact_hash_mismatch(self) -> None:
        evidence = self.archived_demo_contract_evidence()
        evidence["artifact_sha256"] = "d" * 64

        with self.assertRaisesRegex(RuntimeError, "artifact_sha256 does not match"):
            validate_demo_evidence_contract(
                evidence,
                self.evidence_reference(evidence),
                evidence_label="mismatched.demo-evidence.json",
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
        self.assertIn("source_dirty=false", output)
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
        self.assertIn("Live provider auth, quota, and model availability are not verified", output)
        self.assertIn("bench/self_correction.py", output)
        self.assertIn("# would validate fresh results before scoring", output)
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
        self.assertFalse(data["live_provider_auth_quota_model_checked"])
        self.assertEqual(data["results"], str(results))
        self.assertEqual(data["evidence_json"], str(results.with_suffix(".demo-evidence.json")))
        self.assertEqual(data["preflight_report_json"], str(report))
        self.assertTrue(data["checks"]["preflight_report_path_empty"])
        self.assertTrue(data["checks"]["preflight_report_path_distinct_from_results"])
        self.assertTrue(data["checks"]["preflight_report_path_distinct_from_evidence"])
        self.assertEqual(data["checks"]["provider_binary"], "local-test-provider")
        self.assertTrue(data["checks"]["provider_binary_present"])
        self.assertFalse(data["checks"]["local_provider_config_checked"])
        self.assertIsNone(data["checks"]["local_provider_config_present_when_supported"])
        self.assertTrue(data["checks"]["dirty_source_allowed"])
        self.assertIsNone(data["checks"]["source_clean_checked_before_output_creation"])
        self.assertIn("before fresh results/evidence files are created", " ".join(data["notes"]))
        self.assertIn("bench/self_correction.py", data["commands"]["harness"])
        self.assertIn("--demo-evidence-json", data["commands"]["scorer"])
        self.assertIn("verify-evidence-contract", data["commands"]["fresh_provenance_contract"])
        self.assertIn("--reference-evidence-json", data["commands"]["fresh_provenance_contract"])
        self.assertIn(str(DEFAULT_ARCHIVE_EVIDENCE), data["commands"]["fresh_provenance_contract"])
        self.assertIn("--fresh-run-id", data["commands"]["fresh_provenance_contract"])
        self.assertIn("fresh-demo", data["commands"]["fresh_provenance_contract"])
        self.assertIn("--max-tokens", data["commands"]["fresh_provenance_contract"])
        self.assertIn("100000", data["commands"]["fresh_provenance_contract"])
        self.assertIn("--timeout", data["commands"]["fresh_provenance_contract"])
        self.assertIn("1800", data["commands"]["fresh_provenance_contract"])
        self.assertIn("not loop evidence", " ".join(data["notes"]))

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

        data = fresh_preflight_report(args, Path("docs/benchmark-results/self-correction/fresh.demo-evidence.json"))

        self.assertTrue(data["checks"]["source_clean_required"])
        self.assertTrue(data["checks"]["source_clean"])
        self.assertTrue(data["checks"]["source_clean_checked_before_output_creation"])
        self.assertIn("before fresh results/evidence files are created", " ".join(data["notes"]))

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
                    "source_head": "abcdef123456",
                    "source_head_short": "abcdef1",
                    "source_branch": "main",
                    "source_dirty": False,
                    "max_tokens": 100_000,
                    "timeout_secs": 1800,
                },
                {
                    "run_id": "fresh-demo-2",
                    "source_head": "abcdef123456",
                    "source_head_short": "abcdef1",
                    "source_branch": "main",
                    "source_dirty": False,
                    "max_tokens": 100_000,
                    "timeout_secs": 1800,
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

    def test_validate_fresh_results_rejects_stale_or_mismatched_rows(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            results = Path(tmpdir) / "fresh.jsonl"
            results.write_text(
                json.dumps(
                    {
                        "run_id": "old-demo-1",
                        "source_head": "abcdef123456",
                        "source_head_short": "abcdef1",
                        "source_branch": "main",
                        "source_dirty": False,
                        "max_tokens": 100_000,
                        "timeout_secs": 1800,
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
                        "source_head": "abcdef123456",
                        "source_head_short": "abcdef1",
                        "source_branch": "main",
                        "source_dirty": False,
                        "max_tokens": 100_000,
                        "timeout_secs": 1800,
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

    def test_validate_fresh_results_rejects_dirty_source(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            results = Path(tmpdir) / "fresh.jsonl"
            results.write_text(
                json.dumps(
                    {
                        "run_id": "fresh-demo-1",
                        "source_head": "abcdef123456",
                        "source_head_short": "abcdef1",
                        "source_branch": "main",
                        "source_dirty": True,
                        "max_tokens": 100_000,
                        "timeout_secs": 1800,
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
                        "source_head": "abcdef123456",
                        "source_head_short": "abcdef1",
                        "source_branch": "main",
                        "source_dirty": False,
                        "max_tokens": 99_999,
                        "timeout_secs": 1800,
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
