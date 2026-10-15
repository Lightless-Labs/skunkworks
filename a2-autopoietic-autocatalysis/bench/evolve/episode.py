"""Frozen driver for one task/genome episode."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import resource
import shutil
import subprocess
import sys
import sysconfig
import tempfile
import threading
import time
import uuid
from pathlib import Path

E1_DIR = Path(__file__).resolve().parents[1] / "e1"
if str(E1_DIR) not in sys.path:
    sys.path.insert(0, str(E1_DIR))

from bench.e1 import grading, run as e1run, tasks
from bench.evolve import codex_homes, hardened, seatbelt

MAX_STEPS = 8
MAX_AGENT_CALLS = 3
MAX_POLICY_STDOUT = 200000
MAX_POLICY_STDERR = 200000
POLICY_WALL_TIMEOUT = 120
MAX_DIFF_CHARS = 30000
DIFF_TRUNCATION_MARKER = "\n... [diff truncated by evaluator] ...\n"
STATELESS_FEEDBACK_HEADER = (
    "\n\n---\nA previous attempt at this task was rejected by the verifier. "
    "Start again from the original code. Verifier output from that attempt:\n"
)


def _read_capped(stream, limit: int) -> bytes:
    chunks = []
    size = 0
    while True:
        block = stream.read(65536)
        if not block:
            break
        if size < limit:
            keep = block[:limit - size]
            chunks.append(keep)
            size += len(keep)
    return b"".join(chunks)


def _policy_profile(genome_dir: Path, base_snapshot: Path) -> str:
    """Same shape as the grader profile: the whole home denied except the genome and the base
    snapshot, no network, no writes; the evaluator's Python runtime stays readable."""
    runtime = {Path(sys.base_prefix), Path(sys.prefix), Path(sys.executable).parent}
    return seatbelt.pytest_profile([genome_dir, base_snapshot, *runtime], [], Path.home())


def run_policy(genome_dir: str | Path, state: dict, base_snapshot: str | Path) -> tuple[str, str, int]:
    """Run an isolated policy and return bounded stdout/stderr and its exit status."""
    genome, snapshot = Path(genome_dir).resolve(), Path(base_snapshot).resolve()
    script = genome / "policy.py"
    profile = _policy_profile(genome, snapshot)

    def limit_cpu() -> None:
        resource.setrlimit(resource.RLIMIT_CPU, (60, 60))

    profile_path = None
    with tempfile.NamedTemporaryFile("w", prefix="evolve-policy-", suffix=".sb", delete=False) as handle:
        handle.write(profile)
        profile_path = handle.name
    try:
        proc = subprocess.Popen(["/usr/bin/sandbox-exec", "-f", profile_path, sys.executable, "-I", str(script)],
                                cwd=str(snapshot), stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                stderr=subprocess.PIPE, preexec_fn=limit_cpu)
        input_bytes = json.dumps(state, ensure_ascii=False).encode() + b"\n"
        stdout_result, stderr_result = [], []
        out_thread = threading.Thread(target=lambda: stdout_result.append(_read_capped(proc.stdout, MAX_POLICY_STDOUT)))
        err_thread = threading.Thread(target=lambda: stderr_result.append(_read_capped(proc.stderr, MAX_POLICY_STDERR)))
        out_thread.start(); err_thread.start()
        try:
            try:
                proc.stdin.write(input_bytes)
            except BrokenPipeError:
                pass
            proc.stdin.close()
            proc.wait(timeout=POLICY_WALL_TIMEOUT)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait()
            out_thread.join(); err_thread.join()
            return stdout_result[0].decode(errors="replace"), f"policy timed out after {POLICY_WALL_TIMEOUT} seconds", -9
        finally:
            if proc.stdin and not proc.stdin.closed:
                proc.stdin.close()
        out_thread.join(); err_thread.join()
        return stdout_result[0].decode(errors="replace"), err_thread and stderr_result[0].decode(errors="replace"), proc.returncode
    finally:
        if profile_path:
            Path(profile_path).unlink(missing_ok=True)


def _copy_readonly_tree(source: Path, dest: Path) -> None:
    _remove_tree(dest)
    shutil.copytree(source, dest, symlinks=True, ignore=shutil.ignore_patterns(".git"))
    for path in [dest, *dest.rglob("*")]:
        if path.is_dir():
            path.chmod(0o555)
        else:
            path.chmod(0o444)


def _remove_tree(path: Path) -> None:
    if not path.exists():
        return
    for child in [path, *path.rglob("*")]:
        try:
            child.chmod(0o755 if child.is_dir() else 0o644)
        except OSError:
            pass
    shutil.rmtree(path, ignore_errors=True)


def _new_workspace(base: Path, dest: Path) -> Path:
    shutil.rmtree(dest.parent, ignore_errors=True)
    dest.parent.mkdir(parents=True, exist_ok=True)
    if sys.platform == "darwin":
        subprocess.run(["cp", "-cR", str(base), str(dest)], check=True)
    else:
        shutil.copytree(base, dest, symlinks=True)
    return dest


def _git(cwd: Path, *args: str, check: bool = True, index: Path | None = None, stdin: str | None = None):
    env = {**os.environ, **grading.FIXED_GIT_ENV}
    if index is not None:
        env["GIT_INDEX_FILE"] = str(index)
    return subprocess.run(["git", "-c", "commit.gpgsign=false", *args], cwd=cwd, env=env, input=stdin,
                          capture_output=True, text=True, check=check, timeout=600)


def _index_with(workspace: Path, patch: str) -> Path:
    """A throwaway index holding HEAD plus `patch`, built without scanning the working tree
    (APFS clones invalidate git's stat cache; `git add -A` re-hashes the whole repository)."""
    index = workspace / ".git" / "evolve-index"
    index.unlink(missing_ok=True)
    _git(workspace, "read-tree", "HEAD", index=index)
    if patch.strip():
        applied = _git(workspace, "apply", "--cached", "--whitespace=nowarn", check=False, index=index,
                       stdin=patch if patch.endswith("\n") else patch + "\n")
        if applied.returncode:
            raise RuntimeError("patch could not be applied: " + applied.stderr[:500])
    return index


def _commit_base(workspace: Path, patch: str) -> None:
    """Make `patch` the workspace's HEAD (and its files), so a2ctl's diff is relative to it."""
    index = _index_with(workspace, patch)
    tree = _git(workspace, "write-tree", index=index).stdout.strip()
    parent = _git(workspace, "rev-parse", "HEAD").stdout.strip()
    commit = _git(workspace, "commit-tree", tree, "-p", parent, "-m", "candidate base").stdout.strip()
    _git(workspace, "update-ref", "HEAD", commit)
    _git(workspace, "checkout-index", "-a", "-f", index=index)
    index.unlink(missing_ok=True)


def _candidate_diff(workspace: Path, root_commit: str, patch_diff: str) -> str:
    """Cumulative diff from the root commit: the workspace HEAD (base candidate) plus a2ctl's diff."""
    try:
        index = _index_with(workspace, patch_diff)
    except RuntimeError:
        index = _index_with(workspace, "")  # unappliable agent diff: the candidate is its base
    try:
        return _git(workspace, "diff", "--cached", "--binary", root_commit, index=index).stdout
    finally:
        index.unlink(missing_ok=True)


def _diff_files(diff: str) -> list[str]:
    return sorted(set(re.findall(r"^diff --git a/(.*?) b/", diff, flags=re.MULTILINE)))


def _truncate_diff(diff: str) -> str:
    if len(diff) <= MAX_DIFF_CHARS:
        return diff
    room = MAX_DIFF_CHARS - len(DIFF_TRUNCATION_MARKER)
    return diff[:room] + DIFF_TRUNCATION_MARKER


def _agent_message(row: dict) -> str:
    for key in ("final_message", "agent_message", "message", "final_response"):
        value = row.get(key)
        if isinstance(value, str):
            return value[:8000]
    return ""


def _state(nonce: str, problem: str, snapshot: Path, calls: int, steps: int, candidates: list[dict]) -> dict:
    result = []
    for i, item in enumerate(candidates):
        result.append({"index": i, "base": item["base"], "effort": item["effort"],
                       "agent_message": _agent_message(item["row"]), "diff": _truncate_diff(item["diff"]),
                       "diff_stat": _diff_files(item["diff"]), "verify": item["verify"],
                       "tokens": int(item["row"].get("tokens_in") or 0) + int(item["row"].get("tokens_out") or 0)})
    return {"nonce": nonce, "problem_statement": problem, "base_snapshot": str(snapshot),
            "budget": {"agent_calls_left": max(0, MAX_AGENT_CALLS - calls),
                       "steps_left": max(0, MAX_STEPS - steps)}, "candidates": result}


def _parse_action(stdout: str) -> dict:
    value = json.loads(stdout)
    if not isinstance(value, dict) or len(value) != 1:
        raise ValueError("policy must print exactly one action object")
    if "agent" in value:
        action = value["agent"]
        if not isinstance(action, dict) or set(action) != {"base", "prompt", "effort"}:
            raise ValueError("invalid agent action fields")
        base, prompt, effort = action["base"], action["prompt"], action["effort"]
        if not (base == "fresh" or (type(base) is int and base >= 0)):
            raise ValueError("invalid agent base")
        if not isinstance(prompt, str) or len(prompt) > 50000:
            raise ValueError("invalid agent prompt")
        if effort not in ("low", "medium", "high"):
            raise ValueError("invalid agent effort")
        return {"kind": "agent", **action}
    if "submit" in value:
        action = value["submit"]
        if not isinstance(action, dict) or set(action) != {"candidate"} or type(action["candidate"]) is not int:
            raise ValueError("invalid submit action")
        return {"kind": "submit", **action}
    raise ValueError("unknown action")


def deny_read_args(sealed_root: Path) -> list[str]:
    """Paths every evolution-loop Codex process is denied: evaluator secrets, the sealed pool, all
    project checkouts (sibling projects are off limits), and SSH material."""
    paths = [tasks.private_dir(), sealed_root, Path.home() / "Projects", Path.home() / ".ssh"]
    return [arg for path in paths if path.exists() for arg in ("--deny-read", str(path))]


def _run_agent_call(cfg, opaque: str, workspace: Path, payload: dict, emit: Path, log: Path,
                    sealed_root: Path) -> int:
    argv = [str(cfg.a2ctl), "run", "--provider", cfg.provider, "--attempts", "1",
            "--max-tokens", str(cfg.max_tokens), "--timeout", str(cfg.timeout),
            "--network-policy", "allowlist:" + ",".join(cfg.endpoint),
            "--egress-proxy", f"127.0.0.1:{cfg.proxy_port}", "--workspace", str(workspace),
            "--lineage-db", "none", "--verifier-feedback", "raw", "--prompt-profile", "external",
            "--emit-jsonl", str(emit), *deny_read_args(sealed_root)]
    with log.open("a") as handle:
        handle.write(json.dumps({"argv": argv[1:], "task_id": payload["task_id"]}) + "\n")
        handle.flush()
        env = {**os.environ, "CODEX_HOME": str(cfg.codex_home), "A2_CODEX_REASONING_EFFORT": cfg.reasoning_effort}
        return subprocess.run(argv, input=json.dumps(payload) + "\n", text=True, stdout=handle, stderr=handle,
                              cwd=e1run.REPO_ROOT, env=env, timeout=cfg.timeout + 900).returncode


def run_episode(cfg: argparse.Namespace, opaque: str, genome_dir: str | Path,
                run_dir: str | Path, nonce: str) -> dict:
    run_path, genome = Path(run_dir), Path(genome_dir)
    run_path.mkdir(parents=True, exist_ok=True)
    output_dir = run_path / hashlib.sha256(nonce.encode()).hexdigest()[:16]
    output_dir.mkdir(parents=True, exist_ok=True)
    manifest = tasks.load_manifest(opaque)
    problem = getattr(cfg, "problem_statement", manifest.get("problem_statement", ""))
    base = Path(getattr(cfg, "base_snapshot", tasks.work_dir(opaque) / "repo")).resolve()
    # Keyed by nonce only: paths reach agent messages and policy state, and must not name the task.
    episode_work = tasks.work_dir() / "episodes" / nonce
    _remove_tree(episode_work)
    episode_work.mkdir(parents=True, exist_ok=True)
    snapshot = episode_work / "base_snapshot"
    _copy_readonly_tree(base, snapshot)
    markers = e1run.private_markers((tasks.private_dir(opaque) / "gold.patch").read_text(),
                                    manifest.get("hidden_fail_to_pass", []))
    sealed_root = tasks.e1_home() / "sealed"
    sealed_root.mkdir(parents=True, exist_ok=True)
    candidates, attempts, returncodes, actions = [], [], [], []
    calls, policy_error, submitted = 0, None, None
    started = time.time()
    for step in range(MAX_STEPS):
        state = _state(nonce, problem, snapshot, calls, step, candidates)
        e1run.assert_no_private_leak(json.dumps(state), [opaque, *markers])
        try:
            stdout, stderr, code = run_policy(genome, state, snapshot)
            if code != 0:
                raise ValueError("policy exited unsuccessfully: " + stderr[:500])
            action = _parse_action(stdout)
        except Exception as exc:
            policy_error = str(exc)[:1000]
            submitted = len(candidates) - 1
            break
        if action["kind"] == "submit":
            if action["candidate"] < 0 or action["candidate"] >= len(candidates):
                policy_error, submitted = "submit candidate index is out of range", len(candidates) - 1
            else:
                submitted = action["candidate"]
            actions.append({"kind": "submit", "base": None, "effort": None,
                            "prompt_sha256": None, "prompt_chars": 0})
            break
        if calls >= MAX_AGENT_CALLS:
            policy_error, submitted = "agent call budget exhausted", len(candidates) - 1
            break
        base_index = action["base"]
        if base_index != "fresh" and base_index >= len(candidates):
            policy_error, submitted = "agent base candidate index is out of range", len(candidates) - 1
            break
        workspace = _new_workspace(base, episode_work / "work" / str(calls) / "repo")
        root_commit = _git(workspace, "rev-list", "--max-parents=0", "HEAD").stdout.strip()
        if base_index != "fresh":
            _commit_base(workspace, candidates[base_index]["diff"])
        payload = e1run.task_payload(opaque, action["prompt"], f"{opaque}:evolve:{nonce}:{calls}:{uuid.uuid4()}", cfg.endpoint)
        payload["verification_commands"] = []
        e1run.assert_no_private_leak(json.dumps(payload), markers)
        emit = output_dir / f"call-{calls}.jsonl"
        log = output_dir / f"call-{calls}.log"
        cfg.reasoning_effort = action["effort"]
        try:
            call_rc = _run_agent_call(cfg, opaque, workspace, payload, emit, log, sealed_root)
        except (OSError, subprocess.TimeoutExpired):
            call_rc = -1
        returncodes.append(call_rc)
        rows = e1run.read_rows(emit)
        attempts.extend(rows)
        a2row = rows[-1] if rows else {}
        patch_diff = a2row.get("patch_diff") or ""
        cumulative = _candidate_diff(workspace, root_commit, patch_diff)
        verification = hardened.verify_structured(opaque, cumulative)
        candidates.append({"base": base_index, "effort": action["effort"], "diff": cumulative,
                           "verify": verification, "row": a2row})
        actions.append({"kind": "agent", "base": base_index, "effort": action["effort"],
                        "prompt_sha256": hashlib.sha256(action["prompt"].encode()).hexdigest(),
                        "prompt_chars": len(action["prompt"])})
        calls += 1
    else:
        policy_error = "step budget exhausted"
        submitted = len(candidates) - 1
    if submitted is None:
        submitted = len(candidates) - 1
    final_diff = candidates[submitted]["diff"] if submitted >= 0 else ""
    grade = hardened.grade_hardened(opaque, final_diff)
    codex_homes.purge_sessions(cfg.codex_home)
    _remove_tree(episode_work)
    return {"opaque_id": opaque, "nonce": nonce, "genome_sha256": _genome_sha(genome),
            "steps": min(MAX_STEPS, step + 1 if 'step' in locals() else 0), "agent_calls": calls,
            "actions": actions, "tokens_in": sum(int(r.get("tokens_in") or 0) for r in attempts),
            "tokens_out": sum(int(r.get("tokens_out") or 0) for r in attempts),
            "wall_secs": round(time.time() - started, 1), "submitted_candidate": submitted if submitted >= 0 else None,
            "final_diff_sha256": hashlib.sha256(final_diff.encode()).hexdigest(),
            "final_diff_empty": not final_diff.strip(), "policy_error": policy_error,
            "infra_failure": e1run.classify_infra(attempts, returncodes), "grade": grade,
            "verify_summaries": [{"passed": bool(c["verify"].get("passed")),
                                  "failing": [t.get("name", t.get("index")) for t in c["verify"].get("tests", []) if t.get("outcome") != "pass"],
                                  "rejected": bool(c["verify"].get("rejected"))} for c in candidates],
            "diff_stat": _diff_files(final_diff)}


def _genome_sha(genome: Path) -> str:
    digest = hashlib.sha256()
    for path in sorted(p for p in genome.rglob("*") if p.is_file()):
        digest.update(path.relative_to(genome).as_posix().encode())
        digest.update(path.read_bytes())
    return digest.hexdigest()
