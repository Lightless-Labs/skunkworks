#!/usr/bin/env python3
"""Emit BigCodeBench hard tasks as JSONL for A² evaluation."""

from __future__ import annotations

import argparse
import ast
import json
import sys
from pathlib import Path
from typing import Any, Iterable

DEFAULT_DATASET = "bigcode/bigcodebench-hard"
DEFAULT_SPLIT = "v0.1.4"
DEFAULT_LIMIT = 20

COMMON_STDLIB_MODULES = {
    "abc",
    "argparse",
    "array",
    "bisect",
    "collections",
    "csv",
    "datetime",
    "decimal",
    "enum",
    "fractions",
    "functools",
    "heapq",
    "itertools",
    "json",
    "math",
    "operator",
    "os",
    "pathlib",
    "random",
    "re",
    "statistics",
    "string",
    "sys",
    "time",
    "typing",
    "unittest",
    "urllib",
    "uuid",
}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--dataset",
        default=DEFAULT_DATASET,
        help=f"Hugging Face dataset id (default: {DEFAULT_DATASET})",
    )
    parser.add_argument(
        "--split",
        default=DEFAULT_SPLIT,
        help=f"Dataset split/version to load (default: {DEFAULT_SPLIT})",
    )
    parser.add_argument(
        "--limit",
        type=int,
        default=DEFAULT_LIMIT,
        help=f"Number of tasks to emit (default: {DEFAULT_LIMIT})",
    )
    parser.add_argument(
        "--dataset-path",
        help="Optional local JSON/JSONL export to use instead of Hugging Face datasets.",
    )
    parser.add_argument(
        "--repo-root",
        default=".",
        help="Repository root used to materialize per-task workspaces (default: cwd).",
    )
    parser.add_argument(
        "--workspace-dir",
        default="bench/workspaces",
        help="Directory under repo root for generated task workspaces.",
    )
    return parser.parse_args()


def load_local_dataset(path: Path) -> list[dict[str, Any]]:
    text = path.read_text(encoding="utf-8")
    if path.suffix.lower() == ".json":
        data = json.loads(text)
        if isinstance(data, dict):
            rows = data.get("rows") or data.get("data") or []
        else:
            rows = data
        if not isinstance(rows, list):
            raise ValueError(f"unsupported JSON dataset shape in {path}")
        return [row for row in rows if isinstance(row, dict)]

    rows: list[dict[str, Any]] = []
    for line in text.splitlines():
        if not line.strip():
            continue
        item = json.loads(line)
        if isinstance(item, dict):
            rows.append(item)
    return rows


def load_hf_dataset(dataset_name: str, split: str) -> Iterable[dict[str, Any]]:
    try:
        from datasets import load_dataset
    except ImportError as exc:  # noqa: BLE001
        raise SystemExit(
            "The `datasets` package is required unless --dataset-path is used.\n"
            "Install it with: python3 -m pip install datasets"
        ) from exc

    return load_dataset(dataset_name, split=split)


def normalize_libs(raw: Any) -> list[str]:
    if raw is None:
        return []
    if isinstance(raw, list):
        return [str(item).strip() for item in raw if str(item).strip()]
    if isinstance(raw, str):
        text = raw.strip()
        if not text:
            return []
        for parser in (json.loads, ast.literal_eval):
            try:
                parsed = parser(text)
                if isinstance(parsed, list):
                    return [str(item).strip() for item in parsed if str(item).strip()]
            except Exception:  # noqa: BLE001
                continue
        return [part.strip() for part in text.split(",") if part.strip()]
    return [str(raw).strip()]


def choose_category(libs: list[str]) -> str:
    if not libs:
        return "stdlib"
    for lib in libs:
        normalized = lib.strip().lower()
        if normalized and normalized not in COMMON_STDLIB_MODULES:
            return normalized
    return libs[0].strip().lower() or "stdlib"


def sanitize_task_id(task_id: str) -> str:
    return "".join(ch if ch.isalnum() or ch in {"-", "_"} else "_" for ch in task_id)


def build_setup_script(
    code_prompt: str,
    test_code: str,
    libs: list[str],
    entry_point: str,
) -> str:
    requirements = "\n".join(libs) + ("\n" if libs else "")
    import_line = (
        "from solution import task_func"
        if entry_point == "task_func"
        else f"from solution import {entry_point} as task_func"
    )
    test_body = f"{import_line}\n\n{test_code.rstrip()}\n"
    if "__name__ == '__main__'" not in test_body and '__name__ == "__main__"' not in test_body:
        test_body += (
            "\nif __name__ == '__main__':\n"
            "    import unittest\n"
            "    unittest.main()\n"
        )

    lines = [
        "from pathlib import Path",
        "",
        "root = Path('.')",
        "root.mkdir(parents=True, exist_ok=True)",
        "solution = root / 'solution.py'",
        "if not solution.exists():",
        f"    solution.write_text({code_prompt!r}.rstrip() + '\\n', encoding='utf-8')",
        f"(root / 'test_task.py').write_text({test_body!r}, encoding='utf-8')",
        f"(root / 'requirements.txt').write_text({requirements!r}, encoding='utf-8')",
    ]

    return "python3 - <<'PY'\n" + "\n".join(lines) + "\nPY"


def build_problem_statement(
    task_id: str,
    workspace_relpath: str,
    entry_point: str,
    instruct_prompt: str,
    code_prompt: str,
    libs: list[str],
) -> str:
    libraries = ", ".join(libs) if libs else "none specified"
    return (
        "Solve this BigCodeBench task in the repository workspace.\n\n"
        f"Task ID: {task_id}\n"
        f"Workspace: {workspace_relpath}\n"
        f"Create or edit `{workspace_relpath}/solution.py`.\n"
        f"Keep the callable entry point named `{entry_point}`.\n"
        f"The evaluator will run `python3 -m unittest -q test_task.py` from `{workspace_relpath}`.\n"
        f"Available libraries noted by the benchmark: {libraries}.\n\n"
        "Task:\n"
        f"{instruct_prompt.strip()}\n\n"
        "Starter code:\n"
        "```python\n"
        f"{code_prompt.rstrip()}\n"
        "```"
    )


def task_to_payload(task: dict[str, Any], repo_root: Path, workspace_dir: Path) -> dict[str, Any]:
    task_id = str(task.get("task_id") or task.get("question_id") or task.get("q_idx") or "unknown")
    safe_task_id = sanitize_task_id(task_id)
    workspace_path = (repo_root / workspace_dir / safe_task_id).resolve()
    workspace_relpath = (workspace_dir / safe_task_id).as_posix()

    instruct_prompt = str(task.get("instruct_prompt") or task.get("question") or "").strip()
    code_prompt = str(task.get("code_prompt") or "")
    test_code = str(task.get("test") or "")
    entry_point = str(task.get("entry_point") or "task_func")
    libs = normalize_libs(task.get("libs"))
    category = choose_category(libs)

    return {
        "benchmark": "bigcodebench-hard",
        "task_id": task_id,
        "category": category,
        "difficulty": "hard",
        "repo_path": str(workspace_path),
        "problem_statement": build_problem_statement(
            task_id=task_id,
            workspace_relpath=workspace_relpath,
            entry_point=entry_point,
            instruct_prompt=instruct_prompt,
            code_prompt=code_prompt,
            libs=libs,
        ),
        "setup_script": build_setup_script(
            code_prompt=code_prompt,
            test_code=test_code,
            libs=libs,
            entry_point=entry_point,
        ),
        "test_command": "python3 -m unittest -q test_task.py",
    }


def main() -> int:
    args = parse_args()
    repo_root = Path(args.repo_root).resolve()
    workspace_dir = Path(args.workspace_dir)

    if args.dataset_path:
        rows = load_local_dataset(Path(args.dataset_path))
    else:
        rows = list(load_hf_dataset(args.dataset, args.split))

    emitted = 0
    for task in rows:
        if emitted >= args.limit:
            break

        try:
            payload = task_to_payload(task, repo_root=repo_root, workspace_dir=workspace_dir)
        except Exception as exc:  # noqa: BLE001
            print(f"Skipping malformed task: {exc}", file=sys.stderr)
            continue

        print(json.dumps(payload))
        emitted += 1

    if emitted == 0:
        print("No tasks emitted.", file=sys.stderr)
        return 1

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
