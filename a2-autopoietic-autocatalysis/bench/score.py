#!/usr/bin/env python3
"""Score A² benchmark results from a JSONL log file.

Usage:
    python3 bench/score.py bench/results.jsonl
"""

import json
import sys
from collections import defaultdict

def score(results):
    total = len(results)
    if total == 0:
        print("No results to score.")
        return

    resolved = sum(1 for r in results if r.get("resolved", False))
    applied = sum(1 for r in results if r.get("applied", False))

    print(f"Results: {total} tasks")
    print(f"Resolved: {resolved}/{total} ({100*resolved/total:.0f}%)")
    print(f"Applied:  {applied}/{total} ({100*applied/total:.0f}%)")
    print()

    # By model
    by_model = defaultdict(lambda: {"total": 0, "resolved": 0})
    for r in results:
        model = r.get("model", "unknown")
        by_model[model]["total"] += 1
        if r.get("resolved"):
            by_model[model]["resolved"] += 1

    if len(by_model) > 1:
        print("By model:")
        for model, stats in sorted(by_model.items()):
            pct = 100 * stats["resolved"] / stats["total"] if stats["total"] else 0
            print(f"  {model}: {stats['resolved']}/{stats['total']} ({pct:.0f}%)")
        print()

    # Pass@k
    # Group by task_id, check if any attempt resolved
    by_task = defaultdict(list)
    for r in results:
        tid = r.get("task_id", r.get("title", "unknown"))
        by_task[tid].append(r.get("resolved", False))

    for k in [1, 3]:
        passed = sum(1 for attempts in by_task.values() if any(attempts[:k]))
        total_tasks = len(by_task)
        if total_tasks:
            print(f"Pass@{k}: {passed}/{total_tasks} ({100*passed/total_tasks:.0f}%)")

if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("Usage: python3 bench/score.py <results.jsonl>")
        sys.exit(1)

    results = []
    with open(sys.argv[1]) as f:
        for line in f:
            line = line.strip()
            if line:
                results.append(json.loads(line))

    score(results)
