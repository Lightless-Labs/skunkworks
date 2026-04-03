#!/usr/bin/env python3
"""Generate benchmark tasks for A² from standardized datasets.

Modes:
  --source self        Use A²'s own bench/tasks/*.toml (default)
  --source humaneval   Pull from HumanEval (164 Python tasks)
  --source swebench    Pull from SWE-bench Lite (300 real-world tasks)

Output: one task description per line, suitable for piping to a2ctl run.

Usage:
    python3 bench/generate_tasks.py --source self
    python3 bench/generate_tasks.py --source humaneval --limit 10
    python3 bench/generate_tasks.py --source humaneval --limit 10 | cargo run -p a2ctl -- run --provider codex
"""

import argparse
import json
import sys
import os
from pathlib import Path

def load_self_tasks(limit):
    """Load tasks from bench/tasks/*.toml"""
    bench_dir = Path(__file__).parent / "tasks"
    tasks = []
    try:
        import tomllib
    except ImportError:
        try:
            import tomli as tomllib
        except ImportError:
            print("Error: need Python 3.11+ (tomllib) or pip install tomli", file=sys.stderr)
            sys.exit(1)

    for f in sorted(bench_dir.glob("*.toml")):
        with open(f, "rb") as fh:
            data = tomllib.load(fh)
        task = data.get("task", {})
        tasks.append(f"{task.get('title', '')} — {task.get('description', '')}")

    return tasks[:limit]

def load_humaneval(limit):
    """Load tasks from HumanEval dataset."""
    try:
        from datasets import load_dataset
    except ImportError:
        print("Error: pip install datasets", file=sys.stderr)
        sys.exit(1)

    ds = load_dataset("openai/openai_humaneval", split="test")
    tasks = []
    for item in ds:
        if len(tasks) >= limit:
            break
        prompt = item["prompt"]
        entry_point = item["entry_point"]
        task_desc = (
            f"Implement the Python function {entry_point}. "
            f"Here is the signature and docstring:\n\n{prompt}\n\n"
            f"Write only the function body. It must pass the provided test cases."
        )
        tasks.append(task_desc)

    return tasks

def load_swebench_lite(limit):
    """Load tasks from SWE-bench Lite."""
    try:
        from datasets import load_dataset
    except ImportError:
        print("Error: pip install datasets", file=sys.stderr)
        sys.exit(1)

    ds = load_dataset("princeton-nlp/SWE-bench_Lite", split="test")
    tasks = []
    for item in ds:
        if len(tasks) >= limit:
            break
        instance_id = item["instance_id"]
        problem = item["problem_statement"]
        # Truncate long problem statements
        if len(problem) > 500:
            problem = problem[:497] + "..."
        tasks.append(f"[{instance_id}] {problem}")

    return tasks

def main():
    parser = argparse.ArgumentParser(description="Generate benchmark tasks for A²")
    parser.add_argument("--source", choices=["self", "humaneval", "swebench"], default="self")
    parser.add_argument("--limit", type=int, default=10)
    parser.add_argument("--json", action="store_true", help="Output as JSON array instead of lines")
    args = parser.parse_args()

    if args.source == "self":
        tasks = load_self_tasks(args.limit)
    elif args.source == "humaneval":
        tasks = load_humaneval(args.limit)
    elif args.source == "swebench":
        tasks = load_swebench_lite(args.limit)

    if args.json:
        print(json.dumps(tasks, indent=2))
    else:
        for task in tasks:
            # Replace newlines with spaces for piping to a2ctl run
            print(task.replace("\n", " "))

if __name__ == "__main__":
    main()
