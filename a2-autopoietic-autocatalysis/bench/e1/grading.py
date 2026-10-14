"""Native grading primitives shared by the E1 visible-feedback verifier and the final grader."""

from __future__ import annotations

import filecmp
import os
import re
import shutil
import sys
import subprocess
import tempfile
import xml.etree.ElementTree as ET
from dataclasses import dataclass, field
from pathlib import Path

from common import is_test_path, junit_case_to_node_id

FIXED_GIT_ENV = {
    "GIT_AUTHOR_NAME": "e1",
    "GIT_AUTHOR_EMAIL": "e1@localhost",
    "GIT_COMMITTER_NAME": "e1",
    "GIT_COMMITTER_EMAIL": "e1@localhost",
    "GIT_AUTHOR_DATE": "2000-01-01T00:00:00Z",
    "GIT_COMMITTER_DATE": "2000-01-01T00:00:00Z",
}


class CandidateError(RuntimeError):
    """The candidate could not be materialized (e.g. its diff does not apply)."""


@dataclass
class PytestResult:
    returncode: int
    passed: set[str] = field(default_factory=set)
    failed: set[str] = field(default_factory=set)
    output: str = ""


def _git(cwd: Path, *args: str, check: bool = True) -> subprocess.CompletedProcess:
    env = {**os.environ, **FIXED_GIT_ENV}
    return subprocess.run(["git", *args], cwd=cwd, env=env, capture_output=True, text=True, check=check)


def init_history_free_repo(path: Path) -> None:
    """Replace any history with a single root commit of the current tree and no remotes."""
    shutil.rmtree(path / ".git", ignore_errors=True)
    _git(path, "init", "-q")
    _git(path, "config", "core.autocrlf", "false")
    _git(path, "add", "-A", "-f")
    _git(path, "commit", "-q", "--no-verify", "-m", "base")


def _ignore_git(_dir: str, names: list[str]) -> list[str]:
    return [n for n in names if n == ".git"]


def _relative_files(root: Path) -> set[str]:
    files = set()
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = [d for d in dirnames if d != ".git"]
        for name in filenames:
            files.add(str((Path(dirpath) / name).relative_to(root)))
    return files


DIFF_GIT_HEADER = re.compile(r"^diff --git a/(.+?) b/(.+)$")
DIFF_FILE_HEADER = re.compile(r"^(?:---|\+\+\+) (?:a|b)/(.+?)(?:\t.*)?$")


def paths_in_diff(diff: str) -> set[str]:
    paths: set[str] = set()
    for line in diff.splitlines():
        match = DIFF_GIT_HEADER.match(line)
        if match:
            paths.update(match.groups())
            continue
        match = DIFF_FILE_HEADER.match(line)
        if match:
            paths.add(match.group(1))
    return paths


def worktree_changed_paths(tree: Path) -> set[str]:
    """Tracked changes since the root commit (committed or not) plus every untracked file."""
    root = _git(tree, "rev-list", "--max-parents=0", "HEAD").stdout.split()[0]
    changed = set(_git(tree, "diff", "--name-only", "--no-renames", root).stdout.split("\n"))
    changed |= set(_git(tree, "ls-files", "--others").stdout.split("\n"))
    return {p for p in changed if p}


def revert_test_edits(tree: Path, base: Path, candidates: set[str] | None = None) -> list[str]:
    """Restore test paths in `tree` to their `base` state (official test.sh behaviour).

    With `candidates`, only those paths can differ from base; otherwise both trees are walked.
    """
    reverted = []
    if candidates is None:
        tree_tests = {p for p in _relative_files(tree) if is_test_path(p)}
        base_tests = {p for p in _relative_files(base) if is_test_path(p)}
    else:
        tests = {p for p in candidates if is_test_path(p)}
        tree_tests = {p for p in tests if (tree / p).is_file()}
        base_tests = {p for p in tests if (base / p).is_file()}
    for rel in sorted(tree_tests | base_tests):
        target, original = tree / rel, base / rel
        if rel in base_tests:
            if not target.exists() or not filecmp.cmp(original, target, shallow=False):
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(original, target)
                reverted.append(rel)
        elif target.exists():
            target.unlink()
            reverted.append(rel)
    return reverted


def _copy_tree(source: Path, dest: Path) -> None:
    """Copy-on-write clone on macOS (APFS), plain copy elsewhere; never keeps .git."""
    if sys.platform == "darwin":
        subprocess.run(["cp", "-cR", str(source), str(dest)], check=True)
        git_entry = dest / ".git"
        if git_entry.is_dir():
            shutil.rmtree(git_entry)
        elif git_entry.exists():
            git_entry.unlink()
    else:
        shutil.copytree(source, dest, symlinks=True, ignore=_ignore_git)


def apply_patch(tree: Path, patch_text: str) -> None:
    if not patch_text.strip():
        return
    with tempfile.NamedTemporaryFile("w", suffix=".patch", delete=False) as handle:
        handle.write(patch_text if patch_text.endswith("\n") else patch_text + "\n")
        patch_path = handle.name
    try:
        result = subprocess.run(
            ["git", "apply", "--whitespace=nowarn", patch_path], cwd=tree, capture_output=True, text=True
        )
    finally:
        os.unlink(patch_path)
    if result.returncode != 0:
        raise CandidateError(f"patch does not apply: {result.stderr.strip()[:2000]}")


def build_candidate_tree(base: Path, dest: Path, diff: str | None = None, source_tree: Path | None = None) -> list[str]:
    """Materialize a candidate as `base`+`diff` or as a copy of `source_tree`, then revert test edits."""
    if dest.exists():
        shutil.rmtree(dest)
    candidates = worktree_changed_paths(source_tree) if source_tree is not None else paths_in_diff(diff or "")
    _copy_tree(source_tree or base, dest)
    if source_tree is None and diff:
        apply_patch(dest, diff)
    return revert_test_edits(dest, base, candidates)


def run_pytest(
    tree: Path,
    venv: Path,
    args: list[str],
    pythonpath: list[str],
    timeout: int = 1200,
) -> PytestResult:
    python = venv / "bin" / "python"
    if not python.exists():
        python = venv / "bin" / "python3"
    with tempfile.TemporaryDirectory() as tmp:
        junit = Path(tmp) / "junit.xml"
        env = {
            "PATH": f"{venv / 'bin'}:/usr/bin:/bin",
            "HOME": tmp,
            "PYTHONDONTWRITEBYTECODE": "1",
            "PYTHONPATH": ":".join(str(tree / p) for p in pythonpath),
            "LANG": "C.UTF-8",
        }
        try:
            proc = subprocess.run(
                [str(python), "-m", "pytest", "-q", "-rfE", "--tb=short", "-p", "no:cacheprovider",
                 f"--junitxml={junit}", *args],
                cwd=tree, env=env, capture_output=True, text=True, timeout=timeout,
            )
            returncode, output = proc.returncode, proc.stdout + proc.stderr
        except subprocess.TimeoutExpired as exc:
            returncode = -9
            output = f"pytest timed out after {timeout}s\n" + ((exc.stdout or b"").decode() if isinstance(exc.stdout, bytes) else (exc.stdout or ""))
        result = PytestResult(returncode=returncode, output=output)
        if junit.exists():
            exists = lambda rel: (tree / rel).is_file()  # noqa: E731
            for case in ET.parse(junit).getroot().iter("testcase"):
                node = junit_case_to_node_id(case.get("classname", ""), case.get("name", ""), exists)
                if node is None:
                    continue
                tags = {child.tag for child in case}
                if tags & {"failure", "error"}:
                    result.failed.add(node)
                elif "skipped" not in tags:
                    result.passed.add(node)
        return result
