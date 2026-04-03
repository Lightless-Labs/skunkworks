#!/usr/bin/env python3
"""Aggregate benchmark evaluation JSONL logs."""

from __future__ import annotations

import argparse
import json
from collections import defaultdict
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path
from typing import Any


@dataclass
class ResultRecord:
    task_id: str
    category: str
    run_id: str
    attempt: int
    resolved: bool
    evaluated_at: str | None


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("logfile", help="Path to a JSONL result log.")
    return parser.parse_args()


def parse_timestamp(value: str | None) -> datetime | None:
    if not value:
        return None
    try:
        return datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError:
        return None


def normalize_run_id(item: dict[str, Any], fallback_index: int) -> str:
    for key in ("run_id", "run", "session_id"):
        value = item.get(key)
        if value:
            return str(value)

    timestamp = parse_timestamp(item.get("evaluated_at") or item.get("timestamp"))
    if timestamp:
        return timestamp.strftime("%Y-%m-%d")

    return f"run-{fallback_index:04d}"


def normalize_attempt(item: dict[str, Any]) -> int:
    for key in ("attempt", "sample_index", "candidate_index"):
        value = item.get(key)
        if value is None:
            continue
        try:
            return max(int(value), 1)
        except (TypeError, ValueError):
            continue
    return 1


def load_records(path: Path) -> list[ResultRecord]:
    records: list[ResultRecord] = []
    with path.open("r", encoding="utf-8") as handle:
        for index, line in enumerate(handle, start=1):
            if not line.strip():
                continue
            payload = json.loads(line)
            if not isinstance(payload, dict):
                continue

            task_id = str(payload.get("task_id") or payload.get("id") or f"task-{index:04d}")
            category = str(payload.get("category") or "uncategorized")
            run_id = normalize_run_id(payload, index)
            attempt = normalize_attempt(payload)
            resolved = bool(payload.get("resolved"))
            evaluated_at = payload.get("evaluated_at") or payload.get("timestamp")

            records.append(
                ResultRecord(
                    task_id=task_id,
                    category=category,
                    run_id=run_id,
                    attempt=attempt,
                    resolved=resolved,
                    evaluated_at=evaluated_at if isinstance(evaluated_at, str) else None,
                )
            )

    return records


def format_rate(numerator: int, denominator: int) -> str:
    if denominator == 0:
        return "n/a"
    return f"{(numerator / denominator) * 100:.1f}% ({numerator}/{denominator})"


def first_attempt_groups(records: list[ResultRecord]) -> dict[tuple[str, str], ResultRecord]:
    first: dict[tuple[str, str], ResultRecord] = {}
    for record in sorted(records, key=lambda item: (item.run_id, item.task_id, item.attempt)):
        key = (record.run_id, record.task_id)
        first.setdefault(key, record)
    return first


def pass_at_k(records: list[ResultRecord], k: int) -> tuple[int, int]:
    grouped: dict[tuple[str, str], list[ResultRecord]] = defaultdict(list)
    for record in records:
        grouped[(record.run_id, record.task_id)].append(record)

    passed = 0
    total = len(grouped)
    for attempts in grouped.values():
        attempts.sort(key=lambda item: item.attempt)
        if any(record.resolved for record in attempts[:k]):
            passed += 1
    return passed, total


def print_category_breakdown(records: list[ResultRecord]) -> None:
    first = first_attempt_groups(records)
    grouped: dict[str, list[ResultRecord]] = defaultdict(list)
    for record in first.values():
        grouped[record.category].append(record)

    print("Category Breakdown")
    for category in sorted(grouped):
        entries = grouped[category]
        passed = sum(1 for entry in entries if entry.resolved)
        print(f"  {category:<20} {format_rate(passed, len(entries))}")


def sort_run_keys(run_records: dict[str, list[ResultRecord]]) -> list[str]:
    def sort_key(run_id: str) -> tuple[datetime, str]:
        timestamps = [
            ts
            for ts in (parse_timestamp(record.evaluated_at) for record in run_records[run_id])
            if ts is not None
        ]
        if timestamps:
            return min(timestamps), run_id
        return datetime.min, run_id

    return sorted(run_records, key=sort_key)


def print_trend(records: list[ResultRecord]) -> None:
    run_records: dict[str, list[ResultRecord]] = defaultdict(list)
    for record in records:
        run_records[record.run_id].append(record)

    if len(run_records) <= 1:
        print("Trend Over Time")
        print("  single run in log; no trend to report")
        return

    print("Trend Over Time")
    for run_id in sort_run_keys(run_records):
        entries = run_records[run_id]
        first = first_attempt_groups(entries)
        passed = sum(1 for entry in first.values() if entry.resolved)
        print(f"  {run_id:<20} {format_rate(passed, len(first))}")


def main() -> int:
    args = parse_args()
    records = load_records(Path(args.logfile))
    if not records:
        print("No records found.")
        return 1

    overall_passed = sum(1 for record in records if record.resolved)
    pass1_passed, pass1_total = pass_at_k(records, 1)
    pass3_passed, pass3_total = pass_at_k(records, 3)

    print("Overall")
    print(f"  pass rate  {format_rate(overall_passed, len(records))}")
    print(f"  pass@1     {format_rate(pass1_passed, pass1_total)}")
    print(f"  pass@3     {format_rate(pass3_passed, pass3_total)}")
    print()
    print_category_breakdown(records)
    print()
    print_trend(records)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
