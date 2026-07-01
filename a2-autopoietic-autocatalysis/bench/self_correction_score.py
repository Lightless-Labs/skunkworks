#!/usr/bin/env python3
"""Score A² self-correction benchmark JSONL logs.

Unlike generic pass@k, this scorer distinguishes first-pass solves from actual
loop-shaped self-correction. A run only counts as self-corrected when attempt 1
fails and a later attempt with prior lineage visible resolves the task.
"""

from __future__ import annotations

import argparse
import contextlib
import hashlib
import io
import json
import sys
import tempfile
import unittest
from collections import defaultdict
from dataclasses import dataclass
from pathlib import Path
from typing import Any


@dataclass(frozen=True, init=False)
class SelfCorrectionRecord:
    task_id: str
    run_id: str
    attempt: int
    resolved: bool
    prior_lineage_present: bool
    anti_repeat_retry_enabled: bool | None = None
    a2_returncode: int | None = None
    verify_returncode: int | None = None
    verify_command: str | None = None
    touched_files: tuple[str, ...] = ()
    diff_added_lines: int | None = None
    diff_removed_lines: int | None = None
    lineage_records_before: int | None = None
    lineage_records_after: int | None = None
    lineage_reconciled_by_core: bool | None = None
    verifier_failure_evidence_present: bool | None = None
    verifier_failure_evidence_structured_present: bool = False
    promotion_evidence_present: bool = False
    promotion_structured_present: bool = False
    promotion_verifier_gated: bool | None = None
    promotion_structured_evidence_present: bool | None = None
    promotion_lineage_reconciled_by_core: bool | None = None
    promotion_verify_returncode: int | None = None

    def __init__(
        self,
        *,
        task_id: str,
        run_id: str,
        attempt: int,
        resolved: bool,
        prior_lineage_present: bool,
        anti_repeat_retry_enabled: bool | None = None,
        a2_returncode: int | None = None,
        verify_returncode: int | None = None,
        verify_command: str | None = None,
        touched_files: tuple[str, ...] = (),
        diff_added_lines: int | None = None,
        diff_removed_lines: int | None = None,
        lineage_records_before: int | None = None,
        lineage_records_after: int | None = None,
        lineage_reconciled_by_core: bool | None = None,
        verifier_failure_evidence_present: bool | None = None,
        verifier_failure_evidence_structured_present: bool = False,
        promotion_evidence_present: bool = False,
        promotion_structured_present: bool = False,
        promotion_verifier_gated: bool | None = None,
        promotion_structured_evidence_present: bool | None = None,
        promotion_lineage_reconciled_by_core: bool | None = None,
        promotion_verify_returncode: int | None = None,
    ) -> None:
        object.__setattr__(self, "task_id", task_id)
        object.__setattr__(self, "run_id", run_id)
        object.__setattr__(self, "attempt", attempt)
        object.__setattr__(self, "resolved", resolved)
        object.__setattr__(self, "prior_lineage_present", prior_lineage_present)
        object.__setattr__(
            self, "anti_repeat_retry_enabled", anti_repeat_retry_enabled
        )
        object.__setattr__(self, "a2_returncode", a2_returncode)
        object.__setattr__(self, "verify_returncode", verify_returncode)
        object.__setattr__(self, "verify_command", verify_command)
        object.__setattr__(self, "touched_files", touched_files)
        object.__setattr__(self, "diff_added_lines", diff_added_lines)
        object.__setattr__(self, "diff_removed_lines", diff_removed_lines)
        object.__setattr__(self, "lineage_records_before", lineage_records_before)
        object.__setattr__(self, "lineage_records_after", lineage_records_after)
        object.__setattr__(
            self, "lineage_reconciled_by_core", lineage_reconciled_by_core
        )
        object.__setattr__(
            self,
            "verifier_failure_evidence_present",
            verifier_failure_evidence_present,
        )
        object.__setattr__(
            self,
            "verifier_failure_evidence_structured_present",
            verifier_failure_evidence_structured_present,
        )
        object.__setattr__(
            self, "promotion_evidence_present", promotion_evidence_present
        )
        object.__setattr__(
            self, "promotion_structured_present", promotion_structured_present
        )
        object.__setattr__(self, "promotion_verifier_gated", promotion_verifier_gated)
        object.__setattr__(
            self,
            "promotion_structured_evidence_present",
            promotion_structured_evidence_present,
        )
        object.__setattr__(
            self,
            "promotion_lineage_reconciled_by_core",
            promotion_lineage_reconciled_by_core,
        )
        object.__setattr__(
            self, "promotion_verify_returncode", promotion_verify_returncode
        )


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("logfile", help="Path to self-correction JSONL results.")
    parser.add_argument(
        "--trajectories",
        action="store_true",
        help="Print per-run attempt trajectories with return codes and touched files.",
    )
    parser.add_argument(
        "--require-demo",
        action="store_true",
        help=(
            "Exit non-zero unless the log contains a complete self-correction demo: "
            "failed first attempt, archived verifier evidence, prior-lineage retry, "
            "later passing attempt, core lineage reconciliation, and verifier-gated "
            "promotion/apply evidence."
        ),
    )
    parser.add_argument(
        "--demo-evidence-json",
        type=Path,
        help=(
            "Write a machine-readable evidence map for complete demo trajectories, "
            "including JSONL row selectors and fields proving the causal chain."
        ),
    )
    return parser.parse_args(argv)


def optional_int(value: Any) -> int | None:
    if value is None:
        return None
    try:
        return int(value)
    except (TypeError, ValueError):
        return None


def optional_bool(value: Any) -> bool | None:
    if value is True or value is False:
        return value
    return None


def touched_files_from_payload(payload: dict[str, Any]) -> tuple[str, ...]:
    touched_files = payload.get("touched_files")
    if not isinstance(touched_files, list):
        return ()
    return tuple(str(path) for path in touched_files)


def payload_has_verifier_failure_evidence(payload: dict[str, Any]) -> bool | None:
    if "verifier_failure_evidence_present" not in payload:
        return None
    return payload["verifier_failure_evidence_present"] is True


def payload_has_verifier_failure_evidence_field(payload: dict[str, Any]) -> bool:
    return "verifier_failure_evidence_present" in payload


def payload_promotion(payload: dict[str, Any]) -> dict[str, Any]:
    promotion = payload.get("promotion")
    return promotion if isinstance(promotion, dict) else {}


def payload_has_promotion_object(payload: dict[str, Any]) -> bool:
    return isinstance(payload.get("promotion"), dict)


def payload_has_promotion_evidence(payload: dict[str, Any]) -> bool:
    promotion = payload_promotion(payload)
    if payload_has_promotion_object(payload):
        return (
            promotion.get("verifier_gated") is True
            and promotion.get("evidence_present") is True
        )
    if "promotion_evidence_present" in payload:
        return bool(payload["promotion_evidence_present"])
    output = "\n".join(
        str(payload.get(key) or "") for key in ("stdout", "stderr")
    ).lower()
    return "promote_germline" in output or "[applied and rebuilt:" in output


def load_records(path: Path) -> list[SelfCorrectionRecord]:
    records: list[SelfCorrectionRecord] = []
    with path.open("r", encoding="utf-8") as handle:
        for index, line in enumerate(handle, start=1):
            if not line.strip():
                continue
            payload = json.loads(line)
            if not isinstance(payload, dict):
                continue
            records.append(
                SelfCorrectionRecord(
                    task_id=str(payload.get("task_id") or f"task-{index:04d}"),
                    run_id=str(payload.get("run_id") or f"run-{index:04d}"),
                    attempt=max(int(payload.get("attempt") or 1), 1),
                    resolved=bool(payload.get("resolved")),
                    prior_lineage_present=bool(payload.get("prior_lineage_present")),
                    anti_repeat_retry_enabled=(
                        bool(payload["anti_repeat_retry_enabled"])
                        if "anti_repeat_retry_enabled" in payload
                        else None
                    ),
                    a2_returncode=optional_int(payload.get("a2_returncode")),
                    verify_returncode=optional_int(payload.get("verify_returncode")),
                    verify_command=(
                        str(payload["verify_command"])
                        if payload.get("verify_command")
                        else None
                    ),
                    touched_files=touched_files_from_payload(payload),
                    diff_added_lines=optional_int(payload.get("diff_added_lines")),
                    diff_removed_lines=optional_int(payload.get("diff_removed_lines")),
                    lineage_records_before=optional_int(
                        payload.get("lineage_records_before")
                    ),
                    lineage_records_after=optional_int(
                        payload.get("lineage_records_after")
                    ),
                    lineage_reconciled_by_core=(
                        bool(payload["lineage_reconciled_by_core"])
                        if "lineage_reconciled_by_core" in payload
                        else None
                    ),
                    verifier_failure_evidence_present=payload_has_verifier_failure_evidence(
                        payload
                    ),
                    verifier_failure_evidence_structured_present=payload_has_verifier_failure_evidence_field(
                        payload
                    ),
                    promotion_evidence_present=payload_has_promotion_evidence(
                        payload
                    ),
                    promotion_structured_present=payload_has_promotion_object(payload),
                    promotion_verifier_gated=optional_bool(
                        payload_promotion(payload).get("verifier_gated")
                    ),
                    promotion_structured_evidence_present=optional_bool(
                        payload_promotion(payload).get("evidence_present")
                    ),
                    promotion_lineage_reconciled_by_core=optional_bool(
                        payload_promotion(payload).get("lineage_reconciled_by_core")
                    ),
                    promotion_verify_returncode=optional_int(
                        payload_promotion(payload).get("verify_returncode")
                    ),
                )
            )
    return records


def format_rate(numerator: int, denominator: int) -> str:
    if denominator == 0:
        return "n/a"
    return f"{(numerator / denominator) * 100:.1f}% ({numerator}/{denominator})"


def group_records(
    records: list[SelfCorrectionRecord],
) -> dict[tuple[str, str], list[SelfCorrectionRecord]]:
    grouped: dict[tuple[str, str], list[SelfCorrectionRecord]] = defaultdict(list)
    for record in records:
        grouped[(record.run_id, record.task_id)].append(record)
    for attempts in grouped.values():
        attempts.sort(key=lambda record: record.attempt)
    return grouped


def score(records: list[SelfCorrectionRecord]) -> dict[str, int]:
    grouped = group_records(records)
    total = len(grouped)
    resolved = 0
    pass_at_1 = 0
    loop_exercised = 0
    self_corrected = 0

    for attempts in grouped.values():
        if any(record.resolved for record in attempts):
            resolved += 1
        first = attempts[0]
        if first.resolved:
            pass_at_1 += 1
        if any(record.prior_lineage_present for record in attempts):
            loop_exercised += 1
        if (
            not first.resolved
            and any(
                record.resolved and record.prior_lineage_present
                for record in attempts[1:]
            )
        ):
            self_corrected += 1

    return {
        "total": total,
        "resolved": resolved,
        "pass_at_1": pass_at_1,
        "loop_exercised": loop_exercised,
        "self_corrected": self_corrected,
    }


def cohort_label(record: SelfCorrectionRecord) -> str:
    if record.anti_repeat_retry_enabled is True:
        return "anti-repeat enabled"
    if record.anti_repeat_retry_enabled is False:
        return "anti-repeat disabled"
    return "anti-repeat unspecified"


def render_metrics(prefix: str, metrics: dict[str, int]) -> list[str]:
    total = metrics["total"]
    self_corrected = (
        "n/a (0 retrying runs)"
        if metrics["loop_exercised"] == 0
        else format_rate(metrics["self_corrected"], total)
    )
    return [
        prefix,
        f"  resolved             {format_rate(metrics['resolved'], total)}",
        f"  pass@1               {format_rate(metrics['pass_at_1'], total)}",
        f"  loop exercised       {format_rate(metrics['loop_exercised'], total)}",
        f"  self-corrected       {self_corrected}",
    ]


def format_optional_returncode(value: int | None) -> str:
    return "n/a" if value is None else str(value)


def format_diff(record: SelfCorrectionRecord) -> str:
    added = "?" if record.diff_added_lines is None else str(record.diff_added_lines)
    removed = "?" if record.diff_removed_lines is None else str(record.diff_removed_lines)
    return f"+{added}/-{removed}"


def format_touched_files(record: SelfCorrectionRecord) -> str:
    if not record.touched_files:
        return "[]"
    return "[" + ", ".join(record.touched_files) + "]"


def format_verify_command(record: SelfCorrectionRecord) -> str:
    return "n/a" if not record.verify_command else record.verify_command


def attempt_status(record: SelfCorrectionRecord) -> str:
    if record.resolved:
        return "resolved"
    if record.verify_returncode not in (None, 0):
        return "verify-failed"
    return "unresolved"


def verifier_failed_clean_exit(record: SelfCorrectionRecord) -> bool:
    return (
        not record.resolved
        and record.a2_returncode == 0
        and record.verify_returncode not in (None, 0)
    )


def has_archived_verifier_failure(
    record: SelfCorrectionRecord,
    *,
    require_structured_evidence: bool = False,
) -> bool:
    if require_structured_evidence and not record.verifier_failure_evidence_structured_present:
        return False
    return (
        not record.resolved
        and record.verify_returncode not in (None, 0)
        and record.lineage_records_after is not None
        and record.lineage_records_before is not None
        and record.lineage_records_after > record.lineage_records_before
        and record.verifier_failure_evidence_present is not False
    )


def has_verifier_gated_promotion(record: SelfCorrectionRecord) -> bool:
    if not (
        record.resolved
        and record.verify_returncode == 0
        and record.lineage_reconciled_by_core is True
    ):
        return False
    if record.promotion_structured_present:
        return (
            record.promotion_verifier_gated is True
            and record.promotion_structured_evidence_present is True
            and record.promotion_lineage_reconciled_by_core is True
            and record.promotion_verify_returncode == 0
        )
    return record.promotion_evidence_present


def has_retry_context_from_failure(
    first: SelfCorrectionRecord, retry: SelfCorrectionRecord
) -> bool:
    return (
        retry.prior_lineage_present
        and first.lineage_records_after is not None
        and retry.lineage_records_before is not None
        and retry.lineage_records_before >= first.lineage_records_after
    )


def has_retry_context_from_archived_failure(
    first: SelfCorrectionRecord,
    retry: SelfCorrectionRecord,
    *,
    require_structured_evidence: bool = False,
) -> bool:
    return has_archived_verifier_failure(
        first,
        require_structured_evidence=require_structured_evidence,
    ) and has_retry_context_from_failure(first, retry)


def demo_run_ids(records: list[SelfCorrectionRecord]) -> list[tuple[str, str]]:
    demo_runs: list[tuple[str, str]] = []
    for key, attempts in group_records(records).items():
        if len(attempts) < 2:
            continue
        first = attempts[0]
        later_attempts = attempts[1:]
        promotion_attempts = [
            record
            for record in later_attempts
            if has_verifier_gated_promotion(record)
            and has_retry_context_from_failure(first, record)
        ]
        requires_structured_failure_evidence = any(
            record.promotion_structured_present for record in promotion_attempts
        )
        if not has_archived_verifier_failure(
            first,
            require_structured_evidence=requires_structured_failure_evidence,
        ):
            continue
        promotion_attempts = [
            record
            for record in promotion_attempts
            if has_retry_context_from_archived_failure(
                first,
                record,
                require_structured_evidence=requires_structured_failure_evidence,
            )
        ]
        if not promotion_attempts:
            continue
        demo_runs.append(key)
    return demo_runs


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def normalized_evidence_row(record: SelfCorrectionRecord) -> dict[str, Any]:
    """Return schema-bounded row evidence used by the demo proof.

    The full JSONL row can contain verbose stdout/stderr. The demo evidence map
    embeds a fixed normalized field set that the scorer uses to prove the
    causal chain; the source JSONL remains the authoritative artifact and is
    bound by artifact_sha256.
    """

    return {
        "run_id": record.run_id,
        "task_id": record.task_id,
        "attempt": record.attempt,
        "resolved": record.resolved,
        "prior_lineage_present": record.prior_lineage_present,
        "a2_returncode": record.a2_returncode,
        "verify_returncode": record.verify_returncode,
        "verify_command": record.verify_command,
        "touched_files": list(record.touched_files),
        "diff_added_lines": record.diff_added_lines,
        "diff_removed_lines": record.diff_removed_lines,
        "lineage_records_before": record.lineage_records_before,
        "lineage_records_after": record.lineage_records_after,
        "lineage_reconciled_by_core": record.lineage_reconciled_by_core,
        "verifier_failure_evidence_present": record.verifier_failure_evidence_present,
        "verifier_failure_evidence_structured_present": record.verifier_failure_evidence_structured_present,
        "promotion_evidence_present": record.promotion_evidence_present,
        "promotion_structured_present": record.promotion_structured_present,
        "promotion_verifier_gated": record.promotion_verifier_gated,
        "promotion_structured_evidence_present": record.promotion_structured_evidence_present,
        "promotion_lineage_reconciled_by_core": record.promotion_lineage_reconciled_by_core,
        "promotion_verify_returncode": record.promotion_verify_returncode,
    }


def demo_evidence_map(
    records: list[SelfCorrectionRecord],
    *,
    artifact_label: str | None = None,
    artifact_sha256: str | None = None,
) -> dict[str, Any]:
    grouped = group_records(records)
    demos = demo_run_ids(records)
    evidence: dict[str, Any] = {
        "artifact": artifact_label,
        "artifact_sha256": artifact_sha256,
        "complete": bool(demos),
        "requirements": [
            "failed_first_attempt",
            "archived_verifier_failure_evidence",
            "retry_context_from_failure_evidence",
            "later_passing_attempt",
            "lineage_trajectory_recorded",
            "verifier_gated_germline_promotion",
        ],
        "demos": [],
    }
    for run_id, task_id in demos:
        attempts = grouped[(run_id, task_id)]
        first = attempts[0]
        promotion_candidates = [
            record
            for record in attempts[1:]
            if has_verifier_gated_promotion(record)
            and has_retry_context_from_failure(first, record)
        ]
        requires_structured_failure_evidence = any(
            record.promotion_structured_present for record in promotion_candidates
        )
        promotion_attempt = next(
            record
            for record in promotion_candidates
            if has_retry_context_from_archived_failure(
                first,
                record,
                require_structured_evidence=requires_structured_failure_evidence,
            )
        )
        retry_attempts = [
            record
            for record in attempts[1:]
            if has_retry_context_from_archived_failure(
                first,
                record,
                require_structured_evidence=requires_structured_failure_evidence,
            )
        ]
        evidence["demos"].append(
            {
                "run_id": run_id,
                "task_id": task_id,
                "causal_chain": [
                    {
                        "requirement": "failed_first_attempt",
                        "status": "proved",
                        "selector": {"run_id": run_id, "task_id": task_id, "attempt": first.attempt},
                        "evidence_row": normalized_evidence_row(first),
                        "check": "resolved is false and verify_returncode is non-zero",
                        "fields": {
                            "resolved": first.resolved,
                            "verify_returncode": first.verify_returncode,
                            "verify_command": first.verify_command,
                        },
                    },
                    {
                        "requirement": "archived_verifier_failure_evidence",
                        "status": "proved",
                        "selector": {"run_id": run_id, "task_id": task_id, "attempt": first.attempt},
                        "evidence_row": normalized_evidence_row(first),
                        "check": "failed row records verifier failure and advances lineage",
                        "fields": {
                            "lineage_records_before": first.lineage_records_before,
                            "lineage_records_after": first.lineage_records_after,
                            "lineage_advanced": (
                                first.lineage_records_after is not None
                                and first.lineage_records_before is not None
                                and first.lineage_records_after > first.lineage_records_before
                            ),
                            "verifier_failure_evidence_present": first.verifier_failure_evidence_present,
                            "verifier_failure_evidence_structured_present": first.verifier_failure_evidence_structured_present,
                        },
                    },
                    {
                        "requirement": "retry_context_from_failure_evidence",
                        "status": "proved",
                        "check": "retry lineage_records_before reaches the failed row lineage_records_after",
                        "failed_lineage_records_after": first.lineage_records_after,
                        "selectors": [
                            {"run_id": run_id, "task_id": task_id, "attempt": record.attempt}
                            for record in retry_attempts
                        ],
                        "evidence_rows": [
                            normalized_evidence_row(record) for record in retry_attempts
                        ],
                        "fields": [
                            {
                                "attempt": record.attempt,
                                "prior_lineage_present": record.prior_lineage_present,
                                "lineage_records_before": record.lineage_records_before,
                                "derived_from_failed_lineage": has_retry_context_from_failure(first, record),
                                "archived_verifier_failure_evidence": has_archived_verifier_failure(
                                    first,
                                    require_structured_evidence=requires_structured_failure_evidence,
                                ),
                                "retry_context_links_archived_failure": has_retry_context_from_archived_failure(
                                    first,
                                    record,
                                    require_structured_evidence=requires_structured_failure_evidence,
                                ),
                                "failed_attempt_selector": {
                                    "run_id": run_id,
                                    "task_id": task_id,
                                    "attempt": first.attempt,
                                },
                                "failed_verify_returncode": first.verify_returncode,
                                "failed_verifier_failure_evidence_present": first.verifier_failure_evidence_present,
                            }
                            for record in retry_attempts
                        ],
                    },
                    {
                        "requirement": "later_passing_attempt",
                        "status": "proved",
                        "selector": {
                            "run_id": run_id,
                            "task_id": task_id,
                            "attempt": promotion_attempt.attempt,
                        },
                        "evidence_row": normalized_evidence_row(promotion_attempt),
                        "check": "later attempt resolves and verify_returncode is zero",
                        "fields": {
                            "resolved": promotion_attempt.resolved,
                            "verify_returncode": promotion_attempt.verify_returncode,
                        },
                    },
                    {
                        "requirement": "lineage_trajectory_recorded",
                        "status": "proved",
                        "evidence_rows": [
                            normalized_evidence_row(record) for record in attempts
                        ],
                        "check": "same run/task advances lineage from failed first attempt through promotion",
                        "fields": {
                            "lineage_records_before": first.lineage_records_before,
                            "lineage_records_after": promotion_attempt.lineage_records_after,
                            "attempts": [record.attempt for record in attempts],
                        },
                    },
                    {
                        "requirement": "verifier_gated_germline_promotion",
                        "status": "proved",
                        "selector": {
                            "run_id": run_id,
                            "task_id": task_id,
                            "attempt": promotion_attempt.attempt,
                        },
                        "evidence_row": normalized_evidence_row(promotion_attempt),
                        "check": "promotion attempt passed verification, reconciled through core lineage, and has promotion/apply evidence",
                        "fields": {
                            "verify_returncode": promotion_attempt.verify_returncode,
                            "lineage_reconciled_by_core": promotion_attempt.lineage_reconciled_by_core,
                            "promotion_evidence_present": promotion_attempt.promotion_evidence_present,
                            "promotion_structured_present": promotion_attempt.promotion_structured_present,
                            "promotion_verifier_gated": promotion_attempt.promotion_verifier_gated,
                            "promotion_structured_evidence_present": promotion_attempt.promotion_structured_evidence_present,
                            "promotion_lineage_reconciled_by_core": promotion_attempt.promotion_lineage_reconciled_by_core,
                            "promotion_verify_returncode": promotion_attempt.promotion_verify_returncode,
                        },
                    },
                ],
            }
        )
    return evidence


def render_demo_check(
    records: list[SelfCorrectionRecord],
    *,
    artifact_label: str | None = None,
) -> list[str]:
    grouped = group_records(records)
    demos = demo_run_ids(records)
    lines = ["", "Reproducible demo check"]
    if artifact_label:
        lines.append(f"  artifact: {artifact_label}")
    if demos:
        lines.append("  PASS complete self-correction demo trajectory found")
        for run_id, task_id in demos:
            attempts = grouped[(run_id, task_id)]
            first = attempts[0]
            promotion_attempt = next(
                record
                for record in attempts[1:]
                if has_verifier_gated_promotion(record)
                and has_retry_context_from_failure(first, record)
            )
            retry_attempts = [
                record.attempt
                for record in attempts[1:]
                if has_retry_context_from_failure(first, record)
            ]
            retry_text = ",".join(str(attempt) for attempt in retry_attempts)
            artifact = artifact_label or "input JSONL"
            lines.append(f"    {run_id} / {task_id}")
            lines.append(
                "      [proved] failed first attempt: "
                f"attempt {first.attempt} has resolved=false, "
                f"verify_returncode={format_optional_returncode(first.verify_returncode)}, "
                f"verify_command={format_verify_command(first)} in {artifact}"
            )
            lines.append(
                "      [proved] archived verifier/failure evidence: "
                "same JSONL row records verifier failure and lineage "
                f"{format_optional_returncode(first.lineage_records_before)}"
                f"->{format_optional_returncode(first.lineage_records_after)}"
            )
            lines.append(
                "      [proved] retry context from failure evidence: "
                f"prior_lineage_present=true and lineage_records_before >= failed "
                f"lineage_records_after on attempt(s) [{retry_text}] for the same "
                "run_id/task_id"
            )
            lines.append(
                "      [proved] later passing attempt: "
                f"attempt {promotion_attempt.attempt} has resolved=true and "
                f"verify_returncode={format_optional_returncode(promotion_attempt.verify_returncode)}"
            )
            lines.append(
                "      [proved] lineage trajectory: "
                f"{run_id} / {task_id} advances from lineage "
                f"{format_optional_returncode(first.lineage_records_before)}"
                f"->{format_optional_returncode(promotion_attempt.lineage_records_after)}"
            )
            lines.append(
                "      [proved] verifier-gated promotion: "
                f"attempt {promotion_attempt.attempt} has "
                "verify_returncode=0, lineage_reconciled_by_core=true, and "
                "structured verifier_gated promotion fields or legacy apply markers"
            )
            lines.append(
                "      closed-loop evidence: "
                f"attempt {first.attempt} failed recorded verifier "
                f"(verify={format_optional_returncode(first.verify_returncode)}, "
                f"command={format_verify_command(first)}, lineage "
                f"{format_optional_returncode(first.lineage_records_before)}"
                f"->{format_optional_returncode(first.lineage_records_after)}) -> "
                f"prior-lineage retry attempts [{retry_text}] -> "
                f"attempt {promotion_attempt.attempt} later verified pass "
                f"(verify={format_optional_returncode(promotion_attempt.verify_returncode)}) -> "
                "core lineage reconciliation -> verifier-gated promotion/apply evidence"
            )
    else:
        lines.append(
            "  FAIL no run contains failed first attempt with archived verifier evidence, "
            "prior-lineage retry, later verified pass, core lineage reconciliation, "
            "and promotion/apply evidence"
        )
    return lines


def render_attempt_trajectories(records: list[SelfCorrectionRecord]) -> list[str]:
    grouped = group_records(records)
    lines = ["", "Attempt trajectories"]
    for run_id, task_id in sorted(grouped):
        lines.append(f"  {run_id} / {task_id}")
        for record in grouped[(run_id, task_id)]:
            flags: list[str] = []
            if record.prior_lineage_present:
                flags.append("prior-lineage")
            if verifier_failed_clean_exit(record):
                flags.append("clean-agent-exit")
            flag_text = f" flags={','.join(flags)}" if flags else ""
            lines.append(
                f"    attempt {record.attempt}: {attempt_status(record)} "
                f"resolved={str(record.resolved).lower()} "
                f"verify={format_optional_returncode(record.verify_returncode)} "
                f"a2={format_optional_returncode(record.a2_returncode)} "
                f"lineage={format_optional_returncode(record.lineage_records_before)}"
                f"->{format_optional_returncode(record.lineage_records_after)} "
                f"reconciled={str(record.lineage_reconciled_by_core).lower()} "
                f"promotion={str(record.promotion_evidence_present).lower()} "
                f"diff={format_diff(record)} "
                f"files={format_touched_files(record)}"
                f"{flag_text}"
            )
    lines.append(
        "  note: benchmark success is keyed by resolved/verify status; "
        "a2_returncode=0 only means the agent command exited cleanly."
    )
    return lines


def clean_agent_verifier_failures(records: list[SelfCorrectionRecord]) -> int:
    return sum(1 for record in records if verifier_failed_clean_exit(record))


def render(
    records: list[SelfCorrectionRecord],
    *,
    include_trajectories: bool = False,
    require_demo: bool = False,
    artifact_label: str | None = None,
) -> str:
    metrics = score(records)
    lines = render_metrics("Self-Correction Benchmark", metrics)
    lines.insert(1, f"  records             {len(records)} rows / {metrics['total']} runs")
    clean_failures = clean_agent_verifier_failures(records)
    if clean_failures:
        lines.append(
            f"  verifier-failed clean exits {clean_failures} attempts "
            "(a2_returncode=0, resolved=false)"
        )
    if metrics["pass_at_1"] and metrics["self_corrected"] == 0:
        lines.append(
            "  note: successful first attempts do not exercise prior-lineage self-correction"
        )

    cohorts: dict[str, list[SelfCorrectionRecord]] = defaultdict(list)
    for record in records:
        cohorts[cohort_label(record)].append(record)
    if len(cohorts) > 1:
        lines.append("")
        lines.append("Ablation cohorts")
        for label in sorted(cohorts):
            cohort_metrics = score(cohorts[label])
            lines.append(f"  {label}")
            for line in render_metrics(label, cohort_metrics)[1:]:
                lines.append(f"  {line}")

    if require_demo:
        lines.extend(render_demo_check(records, artifact_label=artifact_label))

    if include_trajectories:
        lines.extend(render_attempt_trajectories(records))

    return "\n".join(lines)


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    logfile = Path(args.logfile)
    records = load_records(logfile)
    if not records:
        print("No records found.")
        return 1
    print(
        render(
            records,
            include_trajectories=args.trajectories,
            require_demo=args.require_demo,
            artifact_label=str(logfile),
        )
    )
    if args.demo_evidence_json:
        args.demo_evidence_json.parent.mkdir(parents=True, exist_ok=True)
        args.demo_evidence_json.write_text(
            json.dumps(
                demo_evidence_map(
                    records,
                    artifact_label=str(logfile),
                    artifact_sha256=sha256_file(logfile),
                ),
                indent=2,
                sort_keys=True,
            )
            + "\n",
            encoding="utf-8",
        )
    if args.require_demo and not demo_run_ids(records):
        return 2
    return 0


class SelfCorrectionScoreTests(unittest.TestCase):
    def test_first_pass_success_is_not_self_correction(self) -> None:
        records = [
            SelfCorrectionRecord(
                task_id="task",
                run_id="run",
                attempt=1,
                resolved=True,
                prior_lineage_present=False,
            ),
        ]
        metrics = score(records)
        self.assertEqual(metrics["resolved"], 1)
        self.assertEqual(metrics["pass_at_1"], 1)
        self.assertEqual(metrics["loop_exercised"], 0)
        self.assertEqual(metrics["self_corrected"], 0)

        output = render(records)
        self.assertIn("self-corrected       n/a (0 retrying runs)", output)

    def test_later_success_with_prior_lineage_counts(self) -> None:
        records = [
            SelfCorrectionRecord(
                task_id="task",
                run_id="run",
                attempt=1,
                resolved=False,
                prior_lineage_present=False,
            ),
            SelfCorrectionRecord(
                task_id="task",
                run_id="run",
                attempt=2,
                resolved=True,
                prior_lineage_present=True,
            ),
        ]
        metrics = score(records)
        self.assertEqual(metrics["resolved"], 1)
        self.assertEqual(metrics["pass_at_1"], 0)
        self.assertEqual(metrics["loop_exercised"], 1)
        self.assertEqual(metrics["self_corrected"], 1)

    def test_render_reports_rows_and_grouped_runs(self) -> None:
        records = [
            SelfCorrectionRecord(
                task_id="task",
                run_id="run",
                attempt=1,
                resolved=False,
                prior_lineage_present=False,
            ),
            SelfCorrectionRecord(
                task_id="task",
                run_id="run",
                attempt=2,
                resolved=True,
                prior_lineage_present=True,
            ),
        ]

        output = render(records)

        self.assertIn("2 rows / 1 runs", output)

    def test_render_reports_anti_repeat_ablation_cohorts(self) -> None:
        records = [
            SelfCorrectionRecord(
                task_id="task",
                run_id="enabled",
                attempt=1,
                resolved=False,
                prior_lineage_present=False,
                anti_repeat_retry_enabled=True,
            ),
            SelfCorrectionRecord(
                task_id="task",
                run_id="enabled",
                attempt=2,
                resolved=True,
                prior_lineage_present=True,
                anti_repeat_retry_enabled=True,
            ),
            SelfCorrectionRecord(
                task_id="task",
                run_id="disabled",
                attempt=1,
                resolved=False,
                prior_lineage_present=False,
                anti_repeat_retry_enabled=False,
            ),
            SelfCorrectionRecord(
                task_id="task",
                run_id="disabled",
                attempt=2,
                resolved=False,
                prior_lineage_present=True,
                anti_repeat_retry_enabled=False,
            ),
        ]

        output = render(records)

        self.assertIn("Ablation cohorts", output)
        self.assertIn("anti-repeat enabled", output)
        self.assertIn("anti-repeat disabled", output)

    def test_render_flags_clean_agent_exit_verifier_failures(self) -> None:
        records = [
            SelfCorrectionRecord(
                task_id="task",
                run_id="run",
                attempt=1,
                resolved=False,
                prior_lineage_present=False,
                a2_returncode=0,
                verify_returncode=1,
            ),
            SelfCorrectionRecord(
                task_id="task",
                run_id="run",
                attempt=2,
                resolved=True,
                prior_lineage_present=True,
                a2_returncode=0,
                verify_returncode=0,
            ),
        ]

        output = render(records)

        self.assertIn("verifier-failed clean exits 1 attempts", output)

    def test_render_attempt_trajectories_show_resolved_and_return_codes(self) -> None:
        records = [
            SelfCorrectionRecord(
                task_id="task",
                run_id="run",
                attempt=1,
                resolved=False,
                prior_lineage_present=False,
                a2_returncode=0,
                verify_returncode=1,
                touched_files=("crates/example/src/lib.rs",),
                diff_added_lines=1,
                diff_removed_lines=1,
            ),
            SelfCorrectionRecord(
                task_id="task",
                run_id="run",
                attempt=2,
                resolved=True,
                prior_lineage_present=True,
                a2_returncode=0,
                verify_returncode=0,
            ),
        ]

        output = render(records, include_trajectories=True)

        self.assertIn("Attempt trajectories", output)
        self.assertIn("attempt 1: verify-failed resolved=false verify=1 a2=0", output)
        self.assertIn("clean-agent-exit", output)
        self.assertIn("prior-lineage", output)
        self.assertIn("a2_returncode=0 only means the agent command exited cleanly", output)

    def test_require_demo_passes_complete_promotion_trajectory(self) -> None:
        records = [
            SelfCorrectionRecord(
                task_id="task",
                run_id="run",
                attempt=1,
                resolved=False,
                prior_lineage_present=False,
                a2_returncode=0,
                verify_returncode=1,
                verify_command="cargo test -p demo hidden_regression",
                lineage_records_before=0,
                lineage_records_after=1,
            ),
            SelfCorrectionRecord(
                task_id="task",
                run_id="run",
                attempt=2,
                resolved=True,
                prior_lineage_present=True,
                a2_returncode=0,
                verify_returncode=0,
                lineage_records_before=1,
                lineage_records_after=2,
                lineage_reconciled_by_core=True,
                promotion_evidence_present=True,
            ),
        ]

        self.assertEqual(demo_run_ids(records), [("run", "task")])
        output = render(records, require_demo=True, artifact_label="demo.jsonl")
        self.assertIn("artifact: demo.jsonl", output)
        self.assertIn("PASS complete self-correction demo trajectory found", output)
        self.assertIn("[proved] failed first attempt", output)
        self.assertIn("[proved] archived verifier/failure evidence", output)
        self.assertIn("[proved] retry context from failure evidence", output)
        self.assertIn("[proved] later passing attempt", output)
        self.assertIn("[proved] lineage trajectory", output)
        self.assertIn("[proved] verifier-gated promotion", output)
        self.assertIn(
            "closed-loop evidence: attempt 1 failed recorded verifier "
            "(verify=1, command=cargo test -p demo hidden_regression, lineage "
            "0->1) -> prior-lineage retry attempts [2] -> attempt 2 later "
            "verified pass (verify=0) -> core lineage reconciliation -> "
            "verifier-gated promotion/apply evidence",
            output,
        )

    def test_demo_evidence_json_maps_causal_chain(self) -> None:
        records = [
            SelfCorrectionRecord(
                task_id="task",
                run_id="run",
                attempt=1,
                resolved=False,
                prior_lineage_present=False,
                a2_returncode=0,
                verify_returncode=1,
                verify_command="cargo test -p demo hidden_regression",
                lineage_records_before=0,
                lineage_records_after=1,
                verifier_failure_evidence_present=True,
                verifier_failure_evidence_structured_present=True,
            ),
            SelfCorrectionRecord(
                task_id="task",
                run_id="run",
                attempt=2,
                resolved=True,
                prior_lineage_present=True,
                a2_returncode=0,
                verify_returncode=0,
                lineage_records_before=1,
                lineage_records_after=2,
                lineage_reconciled_by_core=True,
                promotion_evidence_present=True,
            ),
        ]

        evidence = demo_evidence_map(
            records,
            artifact_label="demo.jsonl",
            artifact_sha256="abc123",
        )

        self.assertTrue(evidence["complete"])
        self.assertEqual(evidence["artifact"], "demo.jsonl")
        self.assertEqual(evidence["artifact_sha256"], "abc123")
        chain = evidence["demos"][0]["causal_chain"]
        self.assertEqual(
            [step["requirement"] for step in chain],
            [
                "failed_first_attempt",
                "archived_verifier_failure_evidence",
                "retry_context_from_failure_evidence",
                "later_passing_attempt",
                "lineage_trajectory_recorded",
                "verifier_gated_germline_promotion",
            ],
        )
        self.assertTrue(chain[1]["fields"]["lineage_advanced"])
        self.assertEqual(chain[1]["evidence_row"]["lineage_records_after"], 1)
        retry_field = chain[2]["fields"][0]
        self.assertTrue(retry_field["derived_from_failed_lineage"])
        self.assertTrue(retry_field["archived_verifier_failure_evidence"])
        self.assertTrue(retry_field["retry_context_links_archived_failure"])
        self.assertEqual(retry_field["failed_attempt_selector"]["attempt"], 1)
        self.assertEqual(retry_field["failed_verify_returncode"], 1)
        self.assertEqual(chain[2]["evidence_rows"][0]["lineage_records_before"], 1)
        self.assertTrue(chain[5]["fields"]["promotion_evidence_present"])
        self.assertTrue(chain[5]["evidence_row"]["promotion_evidence_present"])

    def test_demo_evidence_json_embeds_schema_bounded_normalized_rows(self) -> None:
        rows = [
            {
                "task_id": "task",
                "run_id": "run",
                "attempt": 1,
                "resolved": False,
                "prior_lineage_present": False,
                "a2_returncode": 0,
                "verify_returncode": 1,
                "verify_command": "cargo test -p demo hidden_regression",
                "lineage_records_before": 0,
                "lineage_records_after": 1,
                "verifier_failure_evidence_present": True,
                "stdout": "verbose failed attempt output",
                "stderr": "verbose failed attempt error",
            },
            {
                "task_id": "task",
                "run_id": "run",
                "attempt": 2,
                "resolved": True,
                "prior_lineage_present": True,
                "a2_returncode": 0,
                "verify_returncode": 0,
                "lineage_records_before": 1,
                "lineage_records_after": 2,
                "lineage_reconciled_by_core": True,
                "stdout": "[applied and rebuilt: ok] verbose promotion output",
                "stderr": "verbose promotion error",
            },
        ]
        with tempfile.TemporaryDirectory() as tmpdir:
            logfile = Path(tmpdir) / "demo.jsonl"
            logfile.write_text(
                "\n".join(json.dumps(row) for row in rows) + "\n",
                encoding="utf-8",
            )
            records = load_records(logfile)

        evidence = demo_evidence_map(records, artifact_label="demo.jsonl")
        promotion_row = evidence["demos"][0]["causal_chain"][5]["evidence_row"]

        expected_schema = {
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
        }
        self.assertEqual(set(promotion_row), expected_schema)
        self.assertTrue(promotion_row["promotion_evidence_present"])
        self.assertEqual(promotion_row["lineage_reconciled_by_core"], True)
        self.assertNotIn("stdout", promotion_row)
        self.assertNotIn("stderr", promotion_row)

    def test_demo_evidence_json_marks_incomplete_lineage_gap(self) -> None:
        records = [
            SelfCorrectionRecord(
                task_id="task",
                run_id="run",
                attempt=1,
                resolved=False,
                prior_lineage_present=False,
                a2_returncode=0,
                verify_returncode=1,
                lineage_records_before=0,
                lineage_records_after=2,
            ),
            SelfCorrectionRecord(
                task_id="task",
                run_id="run",
                attempt=2,
                resolved=True,
                prior_lineage_present=True,
                a2_returncode=0,
                verify_returncode=0,
                lineage_records_before=1,
                lineage_records_after=3,
                lineage_reconciled_by_core=True,
                promotion_evidence_present=True,
            ),
        ]

        evidence = demo_evidence_map(records, artifact_label="incomplete.jsonl")

        self.assertFalse(evidence["complete"])
        self.assertEqual(evidence["demos"], [])

    def test_sha256_file_hashes_artifact_bytes(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            artifact = Path(tmpdir) / "artifact.jsonl"
            artifact.write_text('{"row":1}\n', encoding="utf-8")

            digest = sha256_file(artifact)

        self.assertEqual(
            digest,
            hashlib.sha256(b'{"row":1}\n').hexdigest(),
        )

    def test_main_writes_incomplete_demo_evidence_json_when_require_demo_fails(self) -> None:
        row = {
            "task_id": "task",
            "run_id": "run",
            "attempt": 1,
            "resolved": True,
            "prior_lineage_present": False,
            "verify_returncode": 0,
            "lineage_records_before": 0,
            "lineage_records_after": 1,
            "lineage_reconciled_by_core": True,
            "stdout": "[applied and rebuilt: ok]",
        }
        with tempfile.TemporaryDirectory() as tmpdir:
            logfile = Path(tmpdir) / "pass-at-one.jsonl"
            evidence_file = Path(tmpdir) / "evidence.json"
            logfile.write_text(json.dumps(row) + "\n", encoding="utf-8")

            stdout = io.StringIO()
            with contextlib.redirect_stdout(stdout):
                code = main(
                    [
                        "--require-demo",
                        "--demo-evidence-json",
                        str(evidence_file),
                        str(logfile),
                    ]
                )
            evidence = json.loads(evidence_file.read_text(encoding="utf-8"))

        self.assertEqual(code, 2)
        self.assertFalse(evidence["complete"])
        self.assertEqual(evidence["demos"], [])
        self.assertEqual(
            evidence["artifact_sha256"],
            hashlib.sha256(json.dumps(row).encode() + b"\n").hexdigest(),
        )

    def test_require_demo_accepts_structured_verifier_gated_promotion(self) -> None:
        rows = [
            {
                "task_id": "task",
                "run_id": "run",
                "attempt": 1,
                "resolved": False,
                "prior_lineage_present": False,
                "a2_returncode": 0,
                "verify_returncode": 1,
                "lineage_records_before": 0,
                "lineage_records_after": 1,
                "verifier_failure_evidence_present": True,
            },
            {
                "task_id": "task",
                "run_id": "run",
                "attempt": 2,
                "resolved": True,
                "prior_lineage_present": True,
                "a2_returncode": 0,
                "verify_returncode": 0,
                "lineage_records_before": 1,
                "lineage_records_after": 2,
                "lineage_reconciled_by_core": True,
                "promotion": {
                    "verifier_gated": True,
                    "evidence_present": True,
                    "lineage_reconciled_by_core": True,
                    "verify_returncode": 0,
                },
            },
        ]
        with tempfile.NamedTemporaryFile("w+", encoding="utf-8") as handle:
            for row in rows:
                handle.write(json.dumps(row) + "\n")
            handle.flush()
            records = load_records(Path(handle.name))

        self.assertEqual(demo_run_ids(records), [("run", "task")])
        output = render(records, require_demo=True)
        self.assertIn("PASS complete self-correction demo trajectory found", output)
        self.assertIn("[proved] verifier-gated promotion", output)

    def test_require_demo_rejects_structured_promotion_without_structured_failure_evidence(self) -> None:
        rows = [
            {
                "task_id": "task",
                "run_id": "run",
                "attempt": 1,
                "resolved": False,
                "prior_lineage_present": False,
                "a2_returncode": 0,
                "verify_returncode": 1,
                "lineage_records_before": 0,
                "lineage_records_after": 1,
            },
            {
                "task_id": "task",
                "run_id": "run",
                "attempt": 2,
                "resolved": True,
                "prior_lineage_present": True,
                "a2_returncode": 0,
                "verify_returncode": 0,
                "lineage_records_before": 1,
                "lineage_records_after": 2,
                "lineage_reconciled_by_core": True,
                "promotion": {
                    "verifier_gated": True,
                    "evidence_present": True,
                    "lineage_reconciled_by_core": True,
                    "verify_returncode": 0,
                },
            },
        ]
        with tempfile.NamedTemporaryFile("w+", encoding="utf-8") as handle:
            for row in rows:
                handle.write(json.dumps(row) + "\n")
            handle.flush()
            records = load_records(Path(handle.name))

        self.assertFalse(records[0].verifier_failure_evidence_structured_present)
        self.assertEqual(demo_run_ids(records), [])
        output = render(records, require_demo=True)
        self.assertIn("FAIL no run contains", output)

    def test_require_demo_accepts_legacy_apply_marker_without_structured_promotion(self) -> None:
        rows = [
            {
                "task_id": "task",
                "run_id": "run",
                "attempt": 1,
                "resolved": False,
                "prior_lineage_present": False,
                "a2_returncode": 0,
                "verify_returncode": 1,
                "lineage_records_before": 0,
                "lineage_records_after": 1,
            },
            {
                "task_id": "task",
                "run_id": "run",
                "attempt": 2,
                "resolved": True,
                "prior_lineage_present": True,
                "a2_returncode": 0,
                "verify_returncode": 0,
                "lineage_records_before": 1,
                "lineage_records_after": 2,
                "lineage_reconciled_by_core": True,
                "stdout": "[applied and rebuilt: ok]",
            },
        ]
        with tempfile.NamedTemporaryFile("w+", encoding="utf-8") as handle:
            for row in rows:
                handle.write(json.dumps(row) + "\n")
            handle.flush()
            records = load_records(Path(handle.name))

        self.assertFalse(records[1].promotion_structured_present)
        self.assertTrue(records[1].promotion_evidence_present)
        self.assertEqual(demo_run_ids(records), [("run", "task")])

    def test_require_demo_rejects_retry_without_failed_lineage_context(self) -> None:
        rows = [
            {
                "task_id": "task",
                "run_id": "run",
                "attempt": 1,
                "resolved": False,
                "prior_lineage_present": False,
                "a2_returncode": 0,
                "verify_returncode": 1,
                "lineage_records_before": 0,
                "lineage_records_after": 2,
            },
            {
                "task_id": "task",
                "run_id": "run",
                "attempt": 2,
                "resolved": True,
                "prior_lineage_present": True,
                "a2_returncode": 0,
                "verify_returncode": 0,
                "lineage_records_before": 1,
                "lineage_records_after": 3,
                "lineage_reconciled_by_core": True,
                "stdout": "[applied and rebuilt: ok]",
            },
        ]
        with tempfile.NamedTemporaryFile("w+", encoding="utf-8") as handle:
            for row in rows:
                handle.write(json.dumps(row) + "\n")
            handle.flush()
            records = load_records(Path(handle.name))

        self.assertTrue(records[1].prior_lineage_present)
        self.assertFalse(has_retry_context_from_failure(records[0], records[1]))
        self.assertEqual(demo_run_ids(records), [])
        output = render(records, require_demo=True)
        self.assertIn("FAIL no run contains", output)

    def test_require_demo_rejects_promotion_without_failed_lineage_context(self) -> None:
        rows = [
            {
                "task_id": "task",
                "run_id": "run",
                "attempt": 1,
                "resolved": False,
                "prior_lineage_present": False,
                "a2_returncode": 0,
                "verify_returncode": 1,
                "lineage_records_before": 0,
                "lineage_records_after": 1,
            },
            {
                "task_id": "task",
                "run_id": "run",
                "attempt": 2,
                "resolved": False,
                "prior_lineage_present": True,
                "a2_returncode": 0,
                "verify_returncode": 1,
                "lineage_records_before": 1,
                "lineage_records_after": 2,
            },
            {
                "task_id": "task",
                "run_id": "run",
                "attempt": 3,
                "resolved": True,
                "prior_lineage_present": False,
                "a2_returncode": 0,
                "verify_returncode": 0,
                "lineage_records_before": 2,
                "lineage_records_after": 3,
                "lineage_reconciled_by_core": True,
                "stdout": "[applied and rebuilt: ok]",
            },
        ]
        with tempfile.NamedTemporaryFile("w+", encoding="utf-8") as handle:
            for row in rows:
                handle.write(json.dumps(row) + "\n")
            handle.flush()
            records = load_records(Path(handle.name))

        self.assertTrue(has_retry_context_from_failure(records[0], records[1]))
        self.assertFalse(has_retry_context_from_failure(records[0], records[2]))
        self.assertTrue(has_verifier_gated_promotion(records[2]))
        self.assertEqual(demo_run_ids(records), [])
        output = render(records, require_demo=True)
        self.assertIn("FAIL no run contains", output)

    def test_require_demo_rejects_pass_at_one_legacy_apply_marker(self) -> None:
        rows = [
            {
                "task_id": "task",
                "run_id": "run",
                "attempt": 1,
                "resolved": True,
                "prior_lineage_present": False,
                "a2_returncode": 0,
                "verify_returncode": 0,
                "lineage_records_before": 0,
                "lineage_records_after": 1,
                "lineage_reconciled_by_core": True,
                "stdout": "[applied and rebuilt: ok]",
            }
        ]
        with tempfile.NamedTemporaryFile("w+", encoding="utf-8") as handle:
            for row in rows:
                handle.write(json.dumps(row) + "\n")
            handle.flush()
            records = load_records(Path(handle.name))

        self.assertTrue(records[0].promotion_evidence_present)
        self.assertEqual(demo_run_ids(records), [])
        output = render(records, require_demo=True)
        self.assertIn("FAIL no run contains", output)

    def test_require_demo_rejects_missing_promotion_evidence(self) -> None:
        records = [
            SelfCorrectionRecord(
                task_id="task",
                run_id="run",
                attempt=1,
                resolved=False,
                prior_lineage_present=False,
                a2_returncode=0,
                verify_returncode=1,
                lineage_records_before=0,
                lineage_records_after=1,
            ),
            SelfCorrectionRecord(
                task_id="task",
                run_id="run",
                attempt=2,
                resolved=True,
                prior_lineage_present=True,
                a2_returncode=0,
                verify_returncode=0,
                lineage_records_before=1,
                lineage_records_after=2,
                lineage_reconciled_by_core=True,
                promotion_evidence_present=False,
            ),
        ]

        self.assertEqual(demo_run_ids(records), [])
        output = render(records, require_demo=True)
        self.assertIn("FAIL no run contains", output)

    def test_require_demo_rejects_promotion_field_without_verifier_gate(self) -> None:
        records = [
            SelfCorrectionRecord(
                task_id="task",
                run_id="run",
                attempt=1,
                resolved=False,
                prior_lineage_present=False,
                a2_returncode=0,
                verify_returncode=1,
                lineage_records_before=0,
                lineage_records_after=1,
            ),
            SelfCorrectionRecord(
                task_id="task",
                run_id="run",
                attempt=2,
                resolved=True,
                prior_lineage_present=True,
                a2_returncode=0,
                verify_returncode=0,
                lineage_records_before=1,
                lineage_records_after=2,
                lineage_reconciled_by_core=False,
                promotion_evidence_present=True,
            ),
        ]

        self.assertEqual(demo_run_ids(records), [])
        output = render(records, require_demo=True)
        self.assertIn("FAIL no run contains", output)

    def test_require_demo_rejects_structured_promotion_gate_false(self) -> None:
        rows = [
            {
                "task_id": "task",
                "run_id": "run",
                "attempt": 1,
                "resolved": False,
                "prior_lineage_present": False,
                "a2_returncode": 0,
                "verify_returncode": 1,
                "lineage_records_before": 0,
                "lineage_records_after": 1,
                "verifier_failure_evidence_present": True,
            },
            {
                "task_id": "task",
                "run_id": "run",
                "attempt": 2,
                "resolved": True,
                "prior_lineage_present": True,
                "a2_returncode": 0,
                "verify_returncode": 0,
                "lineage_records_before": 1,
                "lineage_records_after": 2,
                "lineage_reconciled_by_core": True,
                "promotion_evidence_present": True,
                "promotion": {
                    "verifier_gated": False,
                    "evidence_present": True,
                    "lineage_reconciled_by_core": True,
                    "verify_returncode": 0,
                },
            },
        ]
        with tempfile.NamedTemporaryFile("w+", encoding="utf-8") as handle:
            for row in rows:
                handle.write(json.dumps(row) + "\n")
            handle.flush()
            records = load_records(Path(handle.name))

        self.assertFalse(records[1].promotion_evidence_present)
        self.assertFalse(records[1].promotion_verifier_gated)
        self.assertEqual(demo_run_ids(records), [])
        output = render(records, require_demo=True)
        self.assertIn("FAIL no run contains", output)

    def test_require_demo_rejects_structured_promotion_missing_fields(self) -> None:
        rows = [
            {
                "task_id": "task",
                "run_id": "run",
                "attempt": 1,
                "resolved": False,
                "prior_lineage_present": False,
                "a2_returncode": 0,
                "verify_returncode": 1,
                "lineage_records_before": 0,
                "lineage_records_after": 1,
                "verifier_failure_evidence_present": True,
            },
            {
                "task_id": "task",
                "run_id": "run",
                "attempt": 2,
                "resolved": True,
                "prior_lineage_present": True,
                "a2_returncode": 0,
                "verify_returncode": 0,
                "lineage_records_before": 1,
                "lineage_records_after": 2,
                "lineage_reconciled_by_core": True,
                "promotion": {"verifier_gated": True, "evidence_present": True},
            },
        ]
        with tempfile.NamedTemporaryFile("w+", encoding="utf-8") as handle:
            for row in rows:
                handle.write(json.dumps(row) + "\n")
            handle.flush()
            records = load_records(Path(handle.name))

        self.assertTrue(records[1].promotion_structured_present)
        self.assertEqual(demo_run_ids(records), [])
        output = render(records, require_demo=True)
        self.assertIn("FAIL no run contains", output)

    def test_require_demo_rejects_structured_promotion_verify_failure(self) -> None:
        rows = [
            {
                "task_id": "task",
                "run_id": "run",
                "attempt": 1,
                "resolved": False,
                "prior_lineage_present": False,
                "a2_returncode": 0,
                "verify_returncode": 1,
                "lineage_records_before": 0,
                "lineage_records_after": 1,
                "verifier_failure_evidence_present": True,
            },
            {
                "task_id": "task",
                "run_id": "run",
                "attempt": 2,
                "resolved": True,
                "prior_lineage_present": True,
                "a2_returncode": 0,
                "verify_returncode": 0,
                "lineage_records_before": 1,
                "lineage_records_after": 2,
                "lineage_reconciled_by_core": True,
                "promotion": {
                    "verifier_gated": True,
                    "evidence_present": True,
                    "lineage_reconciled_by_core": True,
                    "verify_returncode": 1,
                },
            },
        ]
        with tempfile.NamedTemporaryFile("w+", encoding="utf-8") as handle:
            for row in rows:
                handle.write(json.dumps(row) + "\n")
            handle.flush()
            records = load_records(Path(handle.name))

        self.assertEqual(demo_run_ids(records), [])
        output = render(records, require_demo=True)
        self.assertIn("FAIL no run contains", output)

    def test_require_demo_rejects_stringly_false_evidence(self) -> None:
        rows = [
            {
                "task_id": "task",
                "run_id": "run",
                "attempt": 1,
                "resolved": False,
                "prior_lineage_present": False,
                "a2_returncode": 0,
                "verify_returncode": 1,
                "lineage_records_before": 0,
                "lineage_records_after": 1,
                "verifier_failure_evidence_present": "false",
            },
            {
                "task_id": "task",
                "run_id": "run",
                "attempt": 2,
                "resolved": True,
                "prior_lineage_present": True,
                "a2_returncode": 0,
                "verify_returncode": 0,
                "lineage_records_before": 1,
                "lineage_records_after": 2,
                "lineage_reconciled_by_core": True,
                "promotion": {
                    "verifier_gated": "true",
                    "evidence_present": "true",
                    "lineage_reconciled_by_core": "true",
                    "verify_returncode": 0,
                },
            },
        ]
        with tempfile.NamedTemporaryFile("w+", encoding="utf-8") as handle:
            for row in rows:
                handle.write(json.dumps(row) + "\n")
            handle.flush()
            records = load_records(Path(handle.name))

        self.assertFalse(records[0].verifier_failure_evidence_present)
        self.assertIsNone(records[1].promotion_verifier_gated)
        self.assertEqual(demo_run_ids(records), [])
        output = render(records, require_demo=True)
        self.assertIn("FAIL no run contains", output)

    def test_require_demo_rejects_stringly_false_promotion_gate(self) -> None:
        rows = [
            {
                "task_id": "task",
                "run_id": "run",
                "attempt": 1,
                "resolved": False,
                "prior_lineage_present": False,
                "a2_returncode": 0,
                "verify_returncode": 1,
                "lineage_records_before": 0,
                "lineage_records_after": 1,
                "verifier_failure_evidence_present": True,
            },
            {
                "task_id": "task",
                "run_id": "run",
                "attempt": 2,
                "resolved": True,
                "prior_lineage_present": True,
                "a2_returncode": 0,
                "verify_returncode": 0,
                "lineage_records_before": 1,
                "lineage_records_after": 2,
                "lineage_reconciled_by_core": True,
                "promotion": {
                    "verifier_gated": "false",
                    "evidence_present": True,
                    "lineage_reconciled_by_core": True,
                    "verify_returncode": 0,
                },
            },
        ]
        with tempfile.NamedTemporaryFile("w+", encoding="utf-8") as handle:
            for row in rows:
                handle.write(json.dumps(row) + "\n")
            handle.flush()
            records = load_records(Path(handle.name))

        self.assertTrue(records[1].promotion_structured_present)
        self.assertIsNone(records[1].promotion_verifier_gated)
        self.assertEqual(demo_run_ids(records), [])
        output = render(records, require_demo=True)
        self.assertIn("FAIL no run contains", output)

    def test_require_demo_rejects_self_attested_promotion_without_gate(self) -> None:
        rows = [
            {
                "task_id": "task",
                "run_id": "run",
                "attempt": 1,
                "resolved": False,
                "prior_lineage_present": False,
                "a2_returncode": 0,
                "verify_returncode": 1,
                "lineage_records_before": 0,
                "lineage_records_after": 1,
            },
            {
                "task_id": "task",
                "run_id": "run",
                "attempt": 2,
                "resolved": True,
                "prior_lineage_present": True,
                "a2_returncode": 0,
                "verify_returncode": 0,
                "lineage_records_before": 1,
                "lineage_records_after": 2,
                "lineage_reconciled_by_core": False,
                "promotion": {"verifier_gated": True, "evidence_present": True},
            },
        ]
        with tempfile.NamedTemporaryFile("w+", encoding="utf-8") as handle:
            for row in rows:
                handle.write(json.dumps(row) + "\n")
            handle.flush()
            records = load_records(Path(handle.name))

        self.assertTrue(records[1].promotion_evidence_present)
        self.assertEqual(demo_run_ids(records), [])
        output = render(records, require_demo=True)
        self.assertIn("FAIL no run contains", output)

    def test_require_demo_rejects_missing_archived_failure_evidence(self) -> None:
        records = [
            SelfCorrectionRecord(
                task_id="task",
                run_id="run",
                attempt=1,
                resolved=False,
                prior_lineage_present=False,
                a2_returncode=0,
                verify_returncode=1,
                lineage_records_before=0,
                lineage_records_after=1,
                verifier_failure_evidence_present=False,
            ),
            SelfCorrectionRecord(
                task_id="task",
                run_id="run",
                attempt=2,
                resolved=True,
                prior_lineage_present=True,
                a2_returncode=0,
                verify_returncode=0,
                lineage_records_before=1,
                lineage_records_after=2,
                lineage_reconciled_by_core=True,
                promotion_evidence_present=True,
            ),
        ]

        self.assertTrue(has_retry_context_from_failure(records[0], records[1]))
        self.assertFalse(has_retry_context_from_archived_failure(records[0], records[1]))
        self.assertEqual(demo_run_ids(records), [])
        output = render(records, require_demo=True)
        self.assertIn("FAIL no run contains", output)

    def test_demo_evidence_json_rejects_retry_context_without_archived_failure_link(self) -> None:
        records = [
            SelfCorrectionRecord(
                task_id="task",
                run_id="run",
                attempt=1,
                resolved=False,
                prior_lineage_present=False,
                a2_returncode=0,
                verify_returncode=1,
                lineage_records_before=0,
                lineage_records_after=1,
                verifier_failure_evidence_present=False,
                verifier_failure_evidence_structured_present=True,
            ),
            SelfCorrectionRecord(
                task_id="task",
                run_id="run",
                attempt=2,
                resolved=True,
                prior_lineage_present=True,
                a2_returncode=0,
                verify_returncode=0,
                lineage_records_before=1,
                lineage_records_after=2,
                lineage_reconciled_by_core=True,
                promotion_structured_present=True,
                promotion_verifier_gated=True,
                promotion_structured_evidence_present=True,
                promotion_lineage_reconciled_by_core=True,
                promotion_verify_returncode=0,
            ),
        ]

        self.assertTrue(has_retry_context_from_failure(records[0], records[1]))
        self.assertFalse(has_retry_context_from_archived_failure(records[0], records[1]))
        evidence = demo_evidence_map(records, artifact_label="missing-failure-evidence.jsonl")
        self.assertFalse(evidence["complete"])
        self.assertEqual(evidence["demos"], [])

    def test_clean_agent_exit_flag_requires_verifier_failure(self) -> None:
        records = [
            SelfCorrectionRecord(
                task_id="task",
                run_id="run",
                attempt=1,
                resolved=False,
                prior_lineage_present=False,
                a2_returncode=0,
                verify_returncode=0,
            ),
        ]

        output = render(records, include_trajectories=True)

        self.assertNotIn("verifier-failed clean exits", output)
        self.assertNotIn("clean-agent-exit", output)


if __name__ == "__main__":
    if sys.argv[1:2] == ["--self-test"]:
        sys.argv = [sys.argv[0]]
        raise SystemExit(unittest.main())
    raise SystemExit(main(sys.argv[1:]))
