#!/usr/bin/env python3
"""E1 visible-feedback verifier. A² runs it in the candidate worktree as a task verification command.

It copies the worktree, reverts edits to test paths, applies the task's test patch, and runs only the
visible fail-to-pass tests plus pass-to-pass tests. Exit 0 iff all of them pass. Output never names
hidden tests. The private manifest is read from $A2_E1_HOME/private, which agent sandboxes cannot read.
"""

from __future__ import annotations

import argparse
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import grading  # noqa: E402
import tasks  # noqa: E402

MAX_OUTPUT_CHARS = 12_000


def render_report(visible: list[str], pass_to_pass: list[str], passed: set[str], output: str) -> tuple[bool, str]:
    checked = list(visible) + [t for t in pass_to_pass if t not in visible]
    failing = [t for t in checked if t not in passed]
    lines = [f"E1 visible verification: {len(checked) - len(failing)}/{len(checked)} required visible tests passed."]
    if failing:
        lines.append("Failing or not run:")
        lines.extend(f"  - {t}" for t in failing)
        tail = output[-MAX_OUTPUT_CHARS:]
        lines.append("pytest output (tail):")
        lines.append(tail)
    return not failing, "\n".join(lines)


def verify(opaque: str, worktree: Path) -> tuple[bool, str]:
    manifest = tasks.load_manifest(opaque)
    profile = tasks.REPO_PROFILES[manifest["repo"]]
    base = tasks.work_dir(opaque) / "repo"
    with tempfile.TemporaryDirectory(prefix="e1-verify-", dir=tasks.private_tmp()) as tmp:
        tree = Path(tmp) / "tree"
        grading.build_candidate_tree(base, tree, source_tree=worktree)
        grading.apply_patch(tree, (tasks.private_dir(opaque) / "test.patch").read_text())
        node_ids = sorted(set(manifest["visible_fail_to_pass"]) | set(manifest["pass_to_pass"]))
        result = grading.run_pytest(tree, tasks.private_dir(opaque) / "venv", node_ids, profile["pythonpath"])
    return render_report(manifest["visible_fail_to_pass"], manifest["pass_to_pass"], result.passed, result.output)


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--task", required=True)
    args = parser.parse_args(argv)
    try:
        ok, report = verify(args.task, Path.cwd())
    except grading.CandidateError as exc:
        print(f"E1 visible verification could not apply the candidate: {exc}")
        return 1
    print(report)
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
