#!/usr/bin/env python3
"""E1 task lifecycle: select candidates, prepare checkouts/venvs, run the gold/empty validity gate, evict.

Layout (outside the repository; the private dir is denied to agent sandboxes):
  $A2_E1_HOME/private/<opaque>/  manifest.json, gold.patch, test.patch, venv/, gate.json
  $A2_E1_HOME/work/<opaque>/repo  history-free base checkout the agent works on
Selection reads the dataset parquet through `uv run --with pyarrow` (see `load_rows`).
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import common  # noqa: E402
import grading  # noqa: E402

DATASET_REVISION = "2d52cb3df914a3fcf80c7f66738b3a88ae37fc50"
DATASET_SHA256 = "1067f72c1f43bbc578276888625db3b836cebb535546c9eaf824c5d5dd104a7f"
DATASET_URL = (
    f"https://huggingface.co/datasets/ScaleAI/SWE-bench_Pro/resolve/{DATASET_REVISION}"
    "/data/default/test-00000-of-00001.parquet"
)
REPO_PROFILES = {
    "ansible/ansible": {
        "requirements": ["requirements.txt"],
        # Unit-test requirements moved over ansible's history; the first existing file is used.
        "unit_requirements": [
            "test/units/requirements.txt",
            "test/lib/ansible_test/_data/requirements/units.txt",
            "test/runner/requirements/units.txt",
        ],
        "extra": ["pytest", "pytest-mock", "mock"],
        "pythonpath": ["lib", "test/lib"],
    },
    "internetarchive/openlibrary": {
        "requirements": ["requirements.txt"],
        "unit_requirements": ["requirements_test.txt"],
        "extra": ["pytest", "pytest-mock"],
        "pythonpath": ["."],
        "submodules": ["vendor/infogami"],
        # The repo pins its interpreter in its Docker base image (3.11 in 2024, 3.12 by 2025).
        "python_from": "docker",
    },
    "qutebrowser/qutebrowser": {
        "requirements": ["requirements.txt"],
        "unit_requirements": [
            "misc/requirements/requirements-tests.txt",
            "misc/requirements/requirements-tests-git.txt",
        ],
        "pyqt_requirements": "misc/requirements/requirements-pyqt*.txt",
        "extra": ["pytest", "pytest-mock", "pytest-qt"],
        "pythonpath": ["."],
        "env": {"QT_QPA_PLATFORM": "offscreen"},
        "fallback_constraints": ["PyQt5<5.16", "PyQtWebEngine<5.16"],
        # Lockfiles pin x86-only wheels; the fallback stages use the unpinned *-raw sources instead.
        "raw_requirements": {"requirements.txt": "misc/requirements/requirements-qutebrowser.txt-raw"},
        "raw_suffix": "-raw",
        # Old PyQt sdists need qmake; arm64 wheels exist from 5.15.10, so take binaries only.
        "fallback_only_binary": ["pyqt5", "pyqt5-sip", "pyqt5-qt5", "pyqtwebengine", "pyqtwebengine-qt5"],
    },
}


def e1_home() -> Path:
    return Path(os.environ.get("A2_E1_HOME", Path.home() / ".a2-e1")).expanduser()


def private_dir(opaque: str | None = None) -> Path:
    root = e1_home() / "private"
    return root / opaque if opaque else root


def work_dir(opaque: str | None = None) -> Path:
    root = e1_home() / "work"
    return root / opaque if opaque else root


def private_tmp() -> Path:
    """Scratch space for trees that contain the test patch; agent sandboxes cannot read it."""
    path = private_dir() / "tmp"
    path.mkdir(parents=True, exist_ok=True)
    return path


def load_manifest(opaque: str) -> dict:
    return json.loads((private_dir(opaque) / "manifest.json").read_text())


def dataset_path() -> Path:
    path = private_dir() / "dataset" / "swe-bench-pro-v2.parquet"
    if not path.exists():
        path.parent.mkdir(parents=True, exist_ok=True)
        subprocess.run(["curl", "-sSfL", "-o", str(path), DATASET_URL], check=True)
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    if digest != DATASET_SHA256:
        raise SystemExit(f"dataset digest mismatch: {digest}")
    return path


def load_rows(parquet: Path) -> list[dict]:
    code = (
        "import json,sys,pyarrow.parquet as pq;"
        "json.dump(pq.read_table(sys.argv[1]).to_pylist(), sys.stdout)"
    )
    out = subprocess.run(
        ["uv", "run", "-q", "--no-project", "--with", "pyarrow==21.0.0", "python", "-I", "-c", code, str(parquet)],
        check=True, capture_output=True, text=True,
    ).stdout
    return json.loads(out)


def select(args: argparse.Namespace) -> None:
    requested_repos = getattr(args, "repo", None)
    allowed_repos = set(requested_repos) if requested_repos else set(REPO_PROFILES)
    rows = [r for r in load_rows(dataset_path()) if r["repo"] in allowed_repos and r["repo"] in REPO_PROFILES]
    selected = []
    for row in sorted(rows, key=lambda r: r["instance_id"]):
        fail_to_pass = json.loads(row["fail_to_pass"])
        if len(set(fail_to_pass)) < 2:
            continue
        opaque = common.opaque_task_id(row["instance_id"])
        visible, hidden = common.split_fail_to_pass(fail_to_pass)
        manifest = {
            "opaque_id": opaque,
            "instance_id": row["instance_id"],
            "repo": row["repo"],
            "base_commit": row["base_commit"],
            "instruction": common.instruction_text(row),
            "problem_statement": common.instruction_text(row, issue_only=True),
            "fail_to_pass": sorted(set(fail_to_pass)),
            "pass_to_pass": sorted(set(json.loads(row["pass_to_pass"]))),
            "visible_fail_to_pass": visible,
            "hidden_fail_to_pass": hidden,
            "selected_test_files": json.loads(row["selected_test_files_to_run"]),
        }
        dest = private_dir(opaque)
        dest.mkdir(parents=True, exist_ok=True)
        existing = dest / "manifest.json"
        if existing.exists():  # keep fields learned during prepare (base_date, install_mode)
            manifest = {**json.loads(existing.read_text()), **manifest}
        existing.write_text(json.dumps(manifest, indent=1, sort_keys=True))
        (dest / "gold.patch").write_text(row["patch"])
        (dest / "test.patch").write_text(row["test_patch"])
        selected.append({"opaque_id": opaque, "instance_id": row["instance_id"]})
    Path(args.out).write_text(json.dumps(selected, indent=1) + "\n")
    print(f"selected {len(selected)} tasks -> {args.out}")


def requirement_files(profile: dict, exists) -> list[str]:
    files = [r for r in profile["requirements"] if exists(r)]
    files += [r for r in profile.get("unit_requirements", []) if exists(r)][:1]
    return files


def install_requirements(venv: Path, repo: Path, profile: dict, base_date: str) -> str:
    """Pre-registered two-stage install: date-pinned first; if that fails, the same files unpinned."""
    files = requirement_files(profile, lambda rel: (repo / rel).is_file())
    pattern = profile.get("pyqt_requirements")
    if pattern:
        matches = list(repo.glob(pattern))
        if matches:
            def version_key(path: Path) -> tuple:
                return tuple(int(part) for part in re.findall(r"\d+", path.name))
            files.append(str(max(matches, key=version_key).relative_to(repo)))
    reqs = [arg for r in files for arg in ("-r", str(repo / r))]
    base = ["uv", "pip", "install", "-q", "--python", str(venv / "bin" / "python"), *reqs, *profile["extra"]]
    pinned = subprocess.run([*base, "--exclude-newer", common.exclude_newer_for_base_date(base_date)],
                            capture_output=True, text=True)
    if pinned.returncode == 0:
        return "date-pinned"
    constraints = profile.get("fallback_constraints", [])
    if profile.get("raw_suffix"):
        return _install_raw(venv, repo, profile, files, base_date, constraints)
    if constraints:
        with tempfile.NamedTemporaryFile(mode="w", encoding="utf-8", prefix="e1-constraints-", suffix=".txt") as stream:
            stream.write("\n".join(constraints) + "\n")
            stream.flush()
            subprocess.run([*base, "-c", stream.name], check=True, capture_output=True, text=True)
    else:
        subprocess.run(base, check=True, capture_output=True, text=True)
    return "unpinned-fallback"


def raw_requirement_files(profile: dict, files: list[str], exists) -> list[str]:
    """Unpinned sources of pinned lockfiles: an explicit mapping, else `<file><raw_suffix>`."""
    raw = []
    for rel in files:
        candidate = profile.get("raw_requirements", {}).get(rel, rel + profile["raw_suffix"])
        raw.append(candidate if exists(candidate) else rel)
    return raw


def _install_raw(venv: Path, repo: Path, profile: dict, files: list[str], base_date: str, constraints: list[str]) -> str:
    """Stages 2-3 for lockfile repos: raw sources date-pinned, then raw sources unpinned (both constrained)."""
    raw = raw_requirement_files(profile, files, lambda rel: (repo / rel).is_file())
    reqs = [arg for r in raw for arg in ("-r", str(repo / r))]
    base = ["uv", "pip", "install", "-q", "--python", str(venv / "bin" / "python"), *reqs, *profile["extra"]]
    for name in profile.get("fallback_only_binary", []):
        base += ["--only-binary", name]
    with tempfile.NamedTemporaryFile(mode="w", encoding="utf-8", prefix="e1-constraints-", suffix=".txt") as stream:
        stream.write("\n".join(constraints) + "\n")
        stream.flush()
        dated = subprocess.run([*base, "-c", stream.name, "--exclude-newer", common.exclude_newer_for_base_date(base_date)],
                               capture_output=True, text=True)
        if dated.returncode == 0:
            return "raw-date-pinned"
        subprocess.run([*base, "-c", stream.name], check=True, capture_output=True, text=True)
    return "raw-unpinned"


def python_version(profile: dict, repo: Path, base_date: str) -> str:
    """The base-year rule, unless the profile names a directory whose Dockerfiles pin `FROM python:X.Y`."""
    source = profile.get("python_from")
    if source:
        versions = set()
        for dockerfile in sorted((repo / source).rglob("Dockerfile*")):
            versions.update(re.findall(r"^FROM\s+python:(\d+\.\d+)", dockerfile.read_text(errors="ignore"), re.MULTILINE))
        if versions:
            return max(versions, key=lambda v: tuple(int(x) for x in v.split(".")))
    return common.python_for_base_date(base_date)


def prepare(opaque: str) -> dict:
    manifest = load_manifest(opaque)
    profile = REPO_PROFILES[manifest["repo"]]
    repo = work_dir(opaque) / "repo"
    if not (repo / ".git").exists():
        shutil.rmtree(repo, ignore_errors=True)
        repo.mkdir(parents=True)
        subprocess.run(["git", "init", "-q"], cwd=repo, check=True)
        subprocess.run(
            ["git", "fetch", "-q", "--depth", "1", f"https://github.com/{manifest['repo']}", manifest["base_commit"]],
            cwd=repo, check=True,
        )
        subprocess.run(["git", "checkout", "-q", "FETCH_HEAD"], cwd=repo, check=True)
        for submodule_path in profile.get("submodules", []):
            _fetch_submodule(repo, submodule_path)
        base_date = subprocess.run(
            ["git", "log", "-1", "--format=%cs"], cwd=repo, check=True, capture_output=True, text=True
        ).stdout.strip()
        grading.init_history_free_repo(repo)
        manifest["base_date"] = base_date
        (private_dir(opaque) / "manifest.json").write_text(json.dumps(manifest, indent=1, sort_keys=True))
    venv = private_dir(opaque) / "venv"
    if not (venv / "bin" / "python").exists():
        python = python_version(profile, repo, manifest["base_date"])
        subprocess.run(["uv", "venv", "-q", "--python", python, str(venv)], check=True)
        try:
            manifest["install_mode"] = install_requirements(venv, repo, profile, manifest["base_date"])
        except subprocess.CalledProcessError:
            shutil.rmtree(venv, ignore_errors=True)
            raise
        (private_dir(opaque) / "manifest.json").write_text(json.dumps(manifest, indent=1, sort_keys=True))
    return manifest


def _fetch_submodule(repo: Path, submodule_path: str) -> None:
    """Fetch the exact gitlink commit without invoking recursive submodule networking."""
    tree = subprocess.run(
        ["git", "ls-tree", "HEAD", submodule_path], cwd=repo, check=True, capture_output=True, text=True
    ).stdout.strip()
    if not tree:
        raise RuntimeError(f"missing gitlink for submodule {submodule_path}")
    metadata, _ = tree.split("\t", 1)
    mode, kind, commit = metadata.split()
    if mode != "160000" or kind != "commit":
        raise RuntimeError(f"{submodule_path} is not a gitlink")

    paths = subprocess.run(
        ["git", "config", "-f", ".gitmodules", "--get-regexp", r"^submodule\..*\.path$"],
        cwd=repo, check=True, capture_output=True, text=True,
    ).stdout.splitlines()
    section = None
    for line in paths:
        key, value = line.split(None, 1)
        if value == submodule_path:
            section = key[: -len(".path")]
            break
    if section is None:
        raise RuntimeError(f"missing .gitmodules entry for {submodule_path}")
    url = subprocess.run(
        ["git", "config", "-f", ".gitmodules", "--get", f"{section}.url"],
        cwd=repo, check=True, capture_output=True, text=True,
    ).stdout.strip()

    destination = repo / submodule_path
    destination.mkdir(parents=True, exist_ok=True)
    subprocess.run(["git", "init", "-q"], cwd=destination, check=True)
    subprocess.run(["git", "fetch", "--depth", "1", url, commit], cwd=destination, check=True)
    subprocess.run(["git", "checkout", "-q", "--detach", "FETCH_HEAD"], cwd=destination, check=True)
    shutil.rmtree(destination / ".git")


def profile_env(manifest: dict) -> dict:
    return dict(REPO_PROFILES[manifest["repo"]].get("env", {}))


def grade_diff(opaque: str, diff: str | None) -> dict:
    """Official-rule grading of base+diff: revert test edits, apply test patch, run selected files."""
    manifest = load_manifest(opaque)
    profile = REPO_PROFILES[manifest["repo"]]
    base = work_dir(opaque) / "repo"
    with tempfile.TemporaryDirectory(prefix="e1-grade-", dir=private_tmp()) as tmp:
        tree = Path(tmp) / "tree"
        try:
            grading.build_candidate_tree(base, tree, diff=diff)
        except grading.CandidateError as exc:
            return {"applied": False, "resolved": False, "hidden_resolved": False, "error": str(exc)}
        grading.apply_patch(tree, (private_dir(opaque) / "test.patch").read_text())
        result = grading.run_pytest(tree, private_dir(opaque) / "venv", manifest["selected_test_files"], profile["pythonpath"])
    required = set(manifest["fail_to_pass"]) | set(manifest["pass_to_pass"])
    hidden_required = set(manifest["hidden_fail_to_pass"]) | set(manifest["pass_to_pass"])
    return {
        "applied": True,
        "resolved": common.resolved(required, result.passed),
        "hidden_resolved": common.resolved(hidden_required, result.passed),
        "required": len(required),
        "required_passed": len(required & result.passed),
        "pytest_returncode": result.returncode,
    }


def gate(opaque: str) -> dict:
    manifest = prepare(opaque)
    gold = (private_dir(opaque) / "gold.patch").read_text()
    result = {"opaque_id": opaque, "install_mode": manifest.get("install_mode"),
              "gold": grade_diff(opaque, gold), "empty": grade_diff(opaque, None)}
    result["passed"] = bool(result["gold"]["resolved"] and not result["empty"]["resolved"])
    (private_dir(opaque) / "gate.json").write_text(json.dumps(result, indent=1, sort_keys=True))
    return result


def split_sample(gate_summary: list[dict], seed: int, calibration: int, max_sample: int) -> dict:
    """Seeded shuffle of gate-passing tasks: the first `calibration` calibrate, the next `max_sample` form E1."""
    import random

    passing = sorted(item["opaque_id"] for item in gate_summary if item.get("passed"))
    random.Random(seed).shuffle(passing)
    return {
        "seed": seed,
        "gate_passed": len(passing),
        "gate_total": len(gate_summary),
        "calibration": passing[:calibration],
        "sample": passing[calibration:calibration + max_sample],
        "reserve": passing[calibration + max_sample:],
    }


def evict(opaque: str) -> None:
    shutil.rmtree(work_dir(opaque), ignore_errors=True)
    shutil.rmtree(private_dir(opaque) / "venv", ignore_errors=True)


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="cmd", required=True)
    s = sub.add_parser("select")
    s.add_argument("--out", required=True, help="where to write the selected task list (opaque + instance ids)")
    s.add_argument("--repo", action="append", help="limit selection to this supported repo (repeatable; default: all)")
    for name in ("prepare", "gate", "evict"):
        p = sub.add_parser(name)
        p.add_argument("--task", required=True)
    g = sub.add_parser("gate-all")
    g.add_argument("--tasks", required=True, help="task list written by `select`")
    g.add_argument("--out", required=True, help="where to write the gate summary JSON")
    sp = sub.add_parser("split-sample")
    sp.add_argument("--gate-summary", required=True)
    sp.add_argument("--seed", type=int, required=True)
    sp.add_argument("--calibration", type=int, default=5)
    sp.add_argument("--max-sample", type=int, default=60)
    sp.add_argument("--out", required=True)
    args = parser.parse_args(argv)
    if args.cmd == "select":
        select(args)
    elif args.cmd == "prepare":
        print(json.dumps({"opaque_id": args.task, "base_date": prepare(args.task)["base_date"]}))
    elif args.cmd == "gate":
        print(json.dumps(gate(args.task), sort_keys=True))
    elif args.cmd == "evict":
        evict(args.task)
    elif args.cmd == "split-sample":
        split = split_sample(json.loads(Path(args.gate_summary).read_text()), args.seed, args.calibration, args.max_sample)
        Path(args.out).write_text(json.dumps(split, indent=1) + "\n")
        print(json.dumps({k: (len(v) if isinstance(v, list) else v) for k, v in split.items()}))
    elif args.cmd == "gate-all":
        summary = []
        for item in json.loads(Path(args.tasks).read_text()):
            try:
                outcome = gate(item["opaque_id"])
            except Exception as exc:  # setup failures exclude the task; they are recorded, not hidden
                outcome = {"opaque_id": item["opaque_id"], "passed": False, "setup_error": repr(exc)[:500]}
            finally:
                evict(item["opaque_id"])
            summary.append({**item, **outcome})
            print(json.dumps({"opaque_id": item["opaque_id"], "passed": outcome["passed"]}), flush=True)
        Path(args.out).write_text(json.dumps(summary, indent=1, sort_keys=True) + "\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
