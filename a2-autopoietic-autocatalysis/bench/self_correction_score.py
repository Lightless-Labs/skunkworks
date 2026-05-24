#!/usr/bin/env python3
"""Score A² self-correction benchmark JSONL logs.

Unlike generic pass@k, this scorer distinguishes first-pass solves from actual
loop-shaped self-correction. A run only counts as self-corrected when attempt 1
fails and a later attempt with prior lineage visible resolves the task.
"""

from __future__ import annotations

import argparse
import json
import sys
import unittest
from collections import defaultdict
from dataclasses import dataclass
from pathlib import Path
from typing import Any


@dataclass(frozen=True)
class SelfCorrectionRecord:
    task_id: str
    run_id: str
    attempt: int
    resolved: bool
    prior_lineage_present: bool
    anti_repeat_retry_enabled: bool | None = None


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("logfile", help="Path to self-correction JSONL results.")
    return parser.parse_args(argv)


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
    return [
        prefix,
        f"  resolved             {format_rate(metrics['resolved'], total)}",
        f"  pass@1               {format_rate(metrics['pass_at_1'], total)}",
        f"  loop exercised       {format_rate(metrics['loop_exercised'], total)}",
        f"  self-corrected       {format_rate(metrics['self_corrected'], total)}",
    ]


def render(records: list[SelfCorrectionRecord]) -> str:
    metrics = score(records)
    lines = render_metrics("Self-Correction Benchmark", metrics)
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

    return "\n".join(lines)


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    records = load_records(Path(args.logfile))
    if not records:
        print("No records found.")
        return 1
    print(render(records))
    return 0


class SelfCorrectionScoreTests(unittest.TestCase):
    def test_first_pass_success_is_not_self_correction(self) -> None:
        records = [
            SelfCorrectionRecord("task", "run", 1, True, False),
        ]
        metrics = score(records)
        self.assertEqual(metrics["resolved"], 1)
        self.assertEqual(metrics["pass_at_1"], 1)
        self.assertEqual(metrics["loop_exercised"], 0)
        self.assertEqual(metrics["self_corrected"], 0)

    def test_later_success_with_prior_lineage_counts(self) -> None:
        records = [
            SelfCorrectionRecord("task", "run", 1, False, False),
            SelfCorrectionRecord("task", "run", 2, True, True),
        ]
        metrics = score(records)
        self.assertEqual(metrics["resolved"], 1)
        self.assertEqual(metrics["pass_at_1"], 0)
        self.assertEqual(metrics["loop_exercised"], 1)
        self.assertEqual(metrics["self_corrected"], 1)

    def test_render_reports_anti_repeat_ablation_cohorts(self) -> None:
        records = [
            SelfCorrectionRecord("task", "enabled", 1, False, False, True),
            SelfCorrectionRecord("task", "enabled", 2, True, True, True),
            SelfCorrectionRecord("task", "disabled", 1, False, False, False),
            SelfCorrectionRecord("task", "disabled", 2, False, True, False),
        ]

        output = render(records)

        self.assertIn("Ablation cohorts", output)
        self.assertIn("anti-repeat enabled", output)
        self.assertIn("anti-repeat disabled", output)


if __name__ == "__main__":
    if sys.argv[1:2] == ["--self-test"]:
        sys.argv = [sys.argv[0]]
        raise SystemExit(unittest.main())
    raise SystemExit(main(sys.argv[1:]))
