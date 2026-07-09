#!/usr/bin/env python3
"""Generate benchmark tasks for A² from standardized datasets.

Modes:
  --source self              Use A²'s own bench/tasks/*.toml (default)
  --source humaneval         Pull from HumanEval (164 Python tasks)
  --source swebench          Pull from SWE-bench Lite (300 real-world tasks)
  --source senior-swe-bench  Load a local Senior SWE Bench / SWE-Bench Pro export via --dataset-path
  --source swe-bench-pro     Alias for --source senior-swe-bench

Default output remains one task description per line. For benchmark evidence,
prefer --jsonl: every emitted task carries no_external_solution_search=true and
network_policy=Isolated so a2ctl run requests the restricted network boundary
instead of relying on prompt text or operator memory. Full benchmark evidence
still requires audited sandbox/provider allowlist coverage on every launch path.

Usage:
    python3 bench/generate_tasks.py --source self
    python3 bench/generate_tasks.py --source humaneval --limit 10 --jsonl | cargo run -p a2ctl -- run --provider codex
    python3 bench/generate_tasks.py --source senior-swe-bench --dataset-path senior-swe-export.jsonl --jsonl | cargo run -p a2ctl -- run --provider codex
    python3 bench/generate_tasks.py --source swe-bench-pro --dataset-path senior-swe-export.jsonl --jsonl | cargo run -p a2ctl -- run --provider codex
    python3 bench/generate_tasks.py --source self --jsonl | cargo run -p a2ctl -- run --provider codex
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
import tempfile
from pathlib import Path
from typing import Any, Optional

DEFAULT_NETWORK_POLICY = "Isolated"
SENIOR_SWE_OPTIONAL_METADATA_FIELDS = (
    "repo",
    "task_url",
    "issue_number",
    "base_commit",
    "source_head",
    "commit_sha",
    "revision",
)


def task_payload(
    task_id: str,
    problem_statement: str,
    *,
    benchmark_source: Optional[str] = None,
) -> dict[str, Any]:
    payload: dict[str, Any] = {
        "task_id": task_id,
        "problem_statement": problem_statement,
        "no_external_solution_search": True,
        "network_policy": DEFAULT_NETWORK_POLICY,
    }
    if benchmark_source:
        payload["benchmark_source"] = benchmark_source
    return payload


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
        tasks.append(task_payload(f.stem, statement, benchmark_source="self"))

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
        tasks.append(
            task_payload(
                f"humaneval-{entry_point}",
                task_desc,
                benchmark_source="humaneval",
            )
        )

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
        tasks.append(
            task_payload(
                instance_id,
                f"[{instance_id}] {problem}",
                benchmark_source="swebench_lite",
            )
        )

    return tasks


def load_json_or_jsonl(path: Path) -> Any:
    text = path.read_text(encoding="utf-8")
    if path.suffix == ".jsonl":
        return [json.loads(line) for line in text.splitlines() if line.strip()]
    try:
        return json.loads(text)
    except json.JSONDecodeError:
        return [json.loads(line) for line in text.splitlines() if line.strip()]


def export_rows_from_path(path: Path) -> list[tuple[int, Any]]:
    text = path.read_text(encoding="utf-8")
    if path.suffix == ".jsonl":
        return [
            (line_number, json.loads(line))
            for line_number, line in enumerate(text.splitlines(), start=1)
            if line.strip()
        ]
    try:
        return [
            (index, task)
            for index, task in enumerate(tasks_from_export_data(json.loads(text)), start=1)
        ]
    except json.JSONDecodeError:
        return [
            (line_number, json.loads(line))
            for line_number, line in enumerate(text.splitlines(), start=1)
            if line.strip()
        ]


def tasks_from_export_data(data: Any) -> list[Any]:
    if isinstance(data, list):
        return data
    if isinstance(data, dict):
        for key in ("tasks", "items", "data", "rows"):
            value = data.get(key)
            if isinstance(value, list):
                return value
        return [data]
    raise ValueError("Senior SWE Bench export must be a JSON object, JSON array, or JSONL file")


def senior_swe_task_id(raw_task: dict[str, Any], index: int) -> str:
    raw_id = (
        raw_task.get("task_id")
        or raw_task.get("id")
        or raw_task.get("instance_id")
        or raw_task.get("name")
        or f"local-{index}"
    )
    task_id = str(raw_id).strip()
    if task_id.startswith("senior-swe-bench-"):
        return task_id
    return f"senior-swe-bench-{task_id}"


def senior_swe_problem_statement(raw_task: dict[str, Any]) -> str:
    for key in ("problem_statement", "prompt", "instructions", "description"):
        value = raw_task.get(key)
        if isinstance(value, str) and value.strip():
            title = raw_task.get("title")
            statement = value.strip()
            if isinstance(title, str) and title.strip() and title.strip() not in statement:
                return f"{title.strip()}\n\n{statement}"
            return statement
    title = raw_task.get("title")
    if isinstance(title, str) and title.strip():
        return title.strip()
    raise ValueError(
        "Senior SWE Bench export row is missing problem_statement/prompt/instructions/description"
    )


def senior_swe_metadata_from_raw(raw_task: dict[str, Any]) -> dict[str, Any]:
    metadata: dict[str, Any] = {}
    for field in SENIOR_SWE_OPTIONAL_METADATA_FIELDS:
        value = raw_task.get(field)
        if isinstance(value, str) and value.strip():
            metadata[f"senior_swe_bench_{field}"] = value.strip()
        elif field == "issue_number" and isinstance(value, int) and value > 0:
            metadata[f"senior_swe_bench_{field}"] = value
    return metadata


def senior_swe_problem_with_metadata(statement: str, metadata: dict[str, Any]) -> str:
    lines = []
    if metadata:
        lines.append("Senior SWE Bench metadata:")
        for key in sorted(metadata):
            label = key.removeprefix("senior_swe_bench_")
            lines.append(f"- {label}: {metadata[key]}")
        lines.append("")
    lines.append(statement)
    return "\n".join(lines).strip()


def load_senior_swe_bench_export(path: Path, limit: int) -> list[dict[str, Any]]:
    export_bytes = path.read_bytes()
    export_sha256 = hashlib.sha256(export_bytes).hexdigest()
    tasks = []
    for index, raw_task in export_rows_from_path(path):
        if len(tasks) >= limit:
            break
        metadata: dict[str, Any] = {}
        if isinstance(raw_task, str):
            task_id = f"senior-swe-bench-local-{index}"
            statement = raw_task.strip()
        elif isinstance(raw_task, dict):
            task_id = senior_swe_task_id(raw_task, index)
            statement = senior_swe_problem_statement(raw_task)
            metadata = senior_swe_metadata_from_raw(raw_task)
        else:
            raise ValueError(f"Senior SWE Bench export row {index} is not an object or string")
        if not statement:
            raise ValueError(f"Senior SWE Bench export row {index} has an empty problem statement")
        task = task_payload(
            task_id,
            senior_swe_problem_with_metadata(statement, metadata),
            benchmark_source="senior-swe-bench",
        )
        task["senior_swe_bench_export_sha256"] = export_sha256
        task["senior_swe_bench_export_row_index"] = index
        task.update(metadata)
        tasks.append(task)
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
    tasks = [task_payload("self-test-task", "Synthetic self task", benchmark_source="self")]
    assert_benchmark_payloads_have_policy(tasks)
    jsonl = "\n".join(json.dumps(task, sort_keys=True) for task in tasks)
    for line in jsonl.splitlines():
        parsed = json.loads(line)
        assert parsed["network_policy"] == DEFAULT_NETWORK_POLICY
        assert parsed["no_external_solution_search"] is True

    sample_export = [
        {
            "id": "repo-issue-1",
            "title": "Fix durable audit rows",
            "problem_statement": "Repair the verifier without consulting public patches.",
            "repo": "example/project",
            "task_url": "https://senior-swe-bench.snorkel.ai/tasks/repo-issue-1",
            "issue_number": 9187,
            "base_commit": "0123456789abcdef0123456789abcdef01234567",
        },
        {"task_id": "senior-swe-bench-repo-issue-2", "prompt": "Add a regression test."},
    ]
    with tempfile.NamedTemporaryFile("w", suffix=".json", encoding="utf-8") as handle:
        json.dump({"tasks": sample_export}, handle)
        handle.flush()
        senior_tasks = load_senior_swe_bench_export(Path(handle.name), limit=10)
    assert_benchmark_payloads_have_policy(senior_tasks)
    assert senior_tasks[0]["task_id"] == "senior-swe-bench-repo-issue-1"
    assert senior_tasks[1]["task_id"] == "senior-swe-bench-repo-issue-2"
    assert senior_tasks[0]["benchmark_source"] == "senior-swe-bench"
    assert senior_tasks[0]["senior_swe_bench_export_row_index"] == 1
    assert senior_tasks[1]["senior_swe_bench_export_row_index"] == 2
    assert isinstance(senior_tasks[0]["senior_swe_bench_export_sha256"], str)
    assert len(senior_tasks[0]["senior_swe_bench_export_sha256"]) == 64
    assert senior_tasks[0]["senior_swe_bench_repo"] == "example/project"
    assert senior_tasks[0]["senior_swe_bench_task_url"].endswith("/tasks/repo-issue-1")
    assert senior_tasks[0]["senior_swe_bench_issue_number"] == 9187
    assert senior_tasks[0]["senior_swe_bench_base_commit"] == "0123456789abcdef0123456789abcdef01234567"
    assert "Senior SWE Bench metadata:" in senior_tasks[0]["problem_statement"]
    assert "repo: example/project" in senior_tasks[0]["problem_statement"]
    assert (
        senior_tasks[0]["senior_swe_bench_export_sha256"]
        == senior_tasks[1]["senior_swe_bench_export_sha256"]
    )
    assert "Fix durable audit rows" in senior_tasks[0]["problem_statement"]
    for task in senior_tasks:
        assert task["network_policy"] == DEFAULT_NETWORK_POLICY
        assert task["no_external_solution_search"] is True

    with tempfile.NamedTemporaryFile("w", suffix=".jsonl", encoding="utf-8") as handle:
        handle.write(json.dumps({"id": "jsonl-1", "description": "First task"}) + "\n")
        handle.write("\n")
        handle.write(json.dumps({"id": "jsonl-2", "description": "Second task"}) + "\n")
        handle.flush()
        limited_jsonl_tasks = load_senior_swe_bench_export(Path(handle.name), limit=2)
    assert len(limited_jsonl_tasks) == 2
    assert limited_jsonl_tasks[0]["task_id"] == "senior-swe-bench-jsonl-1"
    assert limited_jsonl_tasks[0]["senior_swe_bench_export_row_index"] == 1
    assert limited_jsonl_tasks[1]["task_id"] == "senior-swe-bench-jsonl-2"
    assert limited_jsonl_tasks[1]["senior_swe_bench_export_row_index"] == 3
    assert limited_jsonl_tasks[0]["benchmark_source"] == "senior-swe-bench"
    assert limited_jsonl_tasks[0]["network_policy"] == DEFAULT_NETWORK_POLICY
    assert limited_jsonl_tasks[0]["no_external_solution_search"] is True

    alias_args = parse_args([
        "--source",
        "swe-bench-pro",
        "--dataset-path",
        "senior-swe-export.jsonl",
        "--jsonl",
    ])
    assert alias_args.source == "swe-bench-pro"


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Generate benchmark tasks for A²")
    parser.add_argument(
        "--source",
        choices=["self", "humaneval", "swebench", "senior-swe-bench", "swe-bench-pro", "swebench-pro"],
        default="self",
    )
    parser.add_argument("--limit", type=int, default=10)
    parser.add_argument(
        "--dataset-path",
        type=Path,
        help=(
            "Local JSON/JSONL export for --source senior-swe-bench. "
            "Use an offline export; do not let benchmark agents fetch public solutions."
        ),
    )
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
    elif args.source in {"senior-swe-bench", "swe-bench-pro", "swebench-pro"}:
        if args.dataset_path is None:
            raise SystemExit("--source senior-swe-bench requires --dataset-path with a local export")
        tasks = load_senior_swe_bench_export(args.dataset_path, args.limit)
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
