#!/usr/bin/env python3
"""Generate benchmark tasks for A² from standardized datasets.

Modes:
  --source self        Use A²'s own bench/tasks/*.toml (default)
  --source humaneval   Pull from HumanEval (164 Python tasks)
  --source swebench    Pull from SWE-bench Lite (300 real-world tasks)

Default output remains one task description per line. For benchmark evidence,
prefer --jsonl: every emitted task carries no_external_solution_search=true and
network_policy=Isolated so a2ctl run reaches the fail-closed launch gate instead
of relying on prompt text or operator memory.

Usage:
    python3 bench/generate_tasks.py --source self
    python3 bench/generate_tasks.py --source humaneval --limit 10 --jsonl | cargo run -p a2ctl -- run --provider codex
    python3 bench/generate_tasks.py --source self --jsonl | cargo run -p a2ctl -- run --provider codex
"""

import argparse
import json
import sys
from pathlib import Path
from typing import Any

DEFAULT_NETWORK_POLICY = "Isolated"


def task_payload(task_id: str, problem_statement: str) -> dict[str, Any]:
    return {
        "task_id": task_id,
        "problem_statement": problem_statement,
        "no_external_solution_search": True,
        "network_policy": DEFAULT_NETWORK_POLICY,
    }


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
        statement = f"{task.get('title', '')} — {task.get('description', '')}"
        tasks.append(task_payload(f.stem, statement))

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
        tasks.append(task_payload(f"humaneval-{entry_point}", task_desc))

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
        tasks.append(task_payload(instance_id, f"[{instance_id}] {problem}"))

    return tasks

def assert_benchmark_payloads_have_policy(tasks: list[dict[str, Any]]) -> None:
    if not tasks:
        raise AssertionError("expected at least one generated task")
    for task in tasks:
        if task.get("no_external_solution_search") is not True:
            raise AssertionError(f"{task.get('task_id')} missing no_external_solution_search=true")
        if task.get("network_policy") != DEFAULT_NETWORK_POLICY:
            raise AssertionError(f"{task.get('task_id')} missing network_policy={DEFAULT_NETWORK_POLICY}")
        if not task.get("problem_statement"):
            raise AssertionError(f"{task.get('task_id')} missing problem_statement")


def run_self_test() -> None:
    tasks = load_self_tasks(2)
    assert_benchmark_payloads_have_policy(tasks)
    jsonl = "\n".join(json.dumps(task, sort_keys=True) for task in tasks)
    for line in jsonl.splitlines():
        parsed = json.loads(line)
        assert parsed["network_policy"] == DEFAULT_NETWORK_POLICY
        assert parsed["no_external_solution_search"] is True


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Generate benchmark tasks for A²")
    parser.add_argument("--source", choices=["self", "humaneval", "swebench"], default="self")
    parser.add_argument("--limit", type=int, default=10)
    parser.add_argument("--json", action="store_true", help="Output the legacy JSON array of task descriptions")
    parser.add_argument("--jsonl", action="store_true", help="Output one policy-bearing JSON task object per line for a2ctl run")
    parser.add_argument("--self-test", action="store_true", help="Run local payload policy checks")
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    if args.self_test:
        run_self_test()
        print("PASS generate_tasks self-test: JSON/JSONL payloads carry benchmark network policy")
        return 0
    if args.json and args.jsonl:
        raise SystemExit("--json and --jsonl are mutually exclusive")

    if args.source == "self":
        tasks = load_self_tasks(args.limit)
    elif args.source == "humaneval":
        tasks = load_humaneval(args.limit)
    elif args.source == "swebench":
        tasks = load_swebench_lite(args.limit)
    else:
        raise AssertionError(f"unhandled source: {args.source}")

    if args.json:
        print(json.dumps([task["problem_statement"] for task in tasks], indent=2))
    elif args.jsonl:
        for task in tasks:
            print(json.dumps(task, sort_keys=True))
    else:
        for task in tasks:
            # Replace newlines with spaces for piping to a2ctl run. Plain-text mode
            # cannot embed the network policy; pair it with `a2ctl run --network-policy isolated`.
            print(task["problem_statement"].replace("\n", " "))
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
