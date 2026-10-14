"""Native grading primitives shared by the E1 visible-feedback verifier and the final grader."""

from __future__ import annotations

import filecmp
import os
import shutil
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


def revert_test_edits(tree: Path, base: Path) -> list[str]:
    """Restore every test path in `tree` to its `base` state (official test.sh behaviour)."""
    reverted = []
    tree_tests = {p for p in _relative_files(tree) if is_test_path(p)}
    base_tests = {p for p in _relative_files(base) if is_test_path(p)}
    for rel in sorted(tree_tests | base_tests):
        target, original = tree / rel, base / rel
        if rel in base_tests:
            if not target.exists() or not filecmp.cmp(original, target, shallow=False):
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(original, target)
                reverted.append(rel)
        else:
            target.unlink()
            reverted.append(rel)
    return reverted


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
    shutil.copytree(source_tree or base, dest, symlinks=True, ignore=_ignore_git)
    if source_tree is None and diff:
        apply_patch(dest, diff)
    return revert_test_edits(dest, base)


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
