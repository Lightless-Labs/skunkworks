#!/usr/bin/env python3
"""E1 arm driver: run raw / stateless-retry / a2-full on prepared tasks through `a2ctl run` and grade once.

All arms share one execution path (`a2ctl run` candidate-only) and differ only in loop features:
  raw              --attempts 1, --lineage-db none
  stateless-retry  driver loop of --attempts 1 --lineage-db none runs with a fresh task id each attempt;
                   the previous attempt's verifier output is appended to the problem statement
  a2-full          --attempts k, fresh lineage db per (task, arm), unredacted verifier feedback
Every arm starts from its own fresh copy of the history-free base repo. Rows are written under the
private run directory (denied to agent sandboxes) and graded with the official resolution rule.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import random
import shutil
import subprocess
import sys
import time
import uuid
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import tasks  # noqa: E402

REPO_ROOT = Path(__file__).resolve().parents[2]
VERIFY_SCRIPT = REPO_ROOT / "bench" / "e1" / "verify.py"
ARMS = ("raw", "stateless-retry", "a2-full")
CODEX_CONFIG_TEMPLATE = Path(__file__).with_name("codex-home.config.toml")
# Files Codex itself creates at runtime in its home; anything else (AGENTS.md, skills, plugins) is ambient.
CODEX_HOME_RUNTIME_PREFIXES = ("auth.json", "config.toml", "goals_", "logs_", "memories_", "queue_", "state_",
                               ".sqlite", ".sandbox_migration", "cache", "log", "models_cache.json",
                               "installation_id", "tmp", "shell_snapshots", "mcp-oauth-locks")
FEEDBACK_TAIL_CHARS = 12_000
STATELESS_FEEDBACK_HEADER = (
    "\n\n---\nA previous attempt at this task was rejected by the verifier. "
    "Start again from the original code. Verifier output from that attempt:\n"
)


def task_payload(opaque: str, instruction: str, task_key: str, endpoints: list[str]) -> dict:
    return {
        "problem_statement": instruction,
        "task_id": task_key,
        "verification_commands": [{"command": f"python3 {VERIFY_SCRIPT} --task {opaque}", "expect_exit": 0}],
        "no_external_solution_search": True,
        "network_policy": {"AllowList": endpoints},
    }


def arm_order(sample: list[str], seed: int) -> dict[str, list[str]]:
    """Deterministic per-task arm order from the pre-registered seed."""
    rng = random.Random(seed)
    order = {}
    for opaque in sample:
        arms = list(ARMS)
        rng.shuffle(arms)
        order[opaque] = arms
    return order


def assert_no_private_leak(text: str, private_markers: list[str]) -> None:
    for marker in private_markers:
        if marker and marker in text:
            raise RuntimeError(f"private material marker leaked into agent-visible text: {marker[:40]!r}")


def ensure_codex_home(home: Path, user_auth: Path) -> Path:
    """Isolated CODEX_HOME: the committed config template plus a symlink to the operator's auth.

    The auth file is symlinked, never copied, so a token refresh cannot fork the operator's login.
    """
    home.mkdir(parents=True, exist_ok=True)
    unexpected = [p.name for p in home.iterdir() if not p.name.startswith(CODEX_HOME_RUNTIME_PREFIXES)]
    skills = home / "skills"
    if skills.is_dir() and {c.name for c in skills.iterdir()} <= {".system"}:
        unexpected.remove("skills")  # Codex's bundled system skills only; user skills are ambient.
    if unexpected:
        raise RuntimeError(f"isolated CODEX_HOME contains ambient files: {sorted(unexpected)}")
    auth = home / "auth.json"
    if not auth.is_symlink() or auth.resolve() != user_auth.resolve():
        auth.unlink(missing_ok=True)
        auth.symlink_to(user_auth)
    config = home / "config.toml"
    template = CODEX_CONFIG_TEMPLATE.read_text()
    if not config.exists() or config.read_text() != template:
        config.write_text(template)
    return home


def fresh_workspace(opaque: str, arm: str) -> Path:
    base = tasks.work_dir(opaque) / "repo"
    dest = tasks.work_dir(opaque) / f"arm-{arm}" / "repo"
    shutil.rmtree(dest.parent, ignore_errors=True)
    dest.parent.mkdir(parents=True)
    if sys.platform == "darwin":
        # APFS copy-on-write clone, including .git (worktrees are created from it).
        subprocess.run(["cp", "-cR", str(base), str(dest)], check=True)
    else:
        shutil.copytree(base, dest, symlinks=True)
    return dest


def run_a2ctl(cfg: argparse.Namespace, workspace: Path, payload: dict, attempts: int, lineage: str,
              emit: Path, log: Path) -> int:
    argv = [
        str(cfg.a2ctl), "run",
        "--provider", cfg.provider,
        "--attempts", str(attempts),
        "--max-tokens", str(cfg.max_tokens),
        "--timeout", str(cfg.timeout),
        "--network-policy", "allowlist:" + ",".join(cfg.endpoint),
        "--egress-proxy", f"127.0.0.1:{cfg.proxy_port}",
        "--workspace", str(workspace),
        "--lineage-db", lineage,
        "--verifier-feedback", "raw",
        "--prompt-profile", "external",
        "--emit-jsonl", str(emit),
        "--deny-read", str(tasks.private_dir()),
    ]
    with log.open("a") as handle:
        handle.write(json.dumps({"argv": argv[1:], "task_id": payload["task_id"]}) + "\n")
        handle.flush()
        env = {**os.environ, "CODEX_HOME": str(cfg.codex_home), "A2_CODEX_REASONING_EFFORT": cfg.reasoning_effort}
        proc = subprocess.run(argv, input=json.dumps(payload) + "\n", text=True, stdout=handle, stderr=handle,
                              cwd=REPO_ROOT, env=env, timeout=cfg.timeout * attempts + 900)
    return proc.returncode


def read_rows(emit: Path) -> list[dict]:
    """Rows of one a2ctl invocation (each invocation writes its own JSONL file)."""
    if not emit.exists():
        return []
    return [json.loads(line) for line in emit.read_text().splitlines() if line.strip()]


def verifier_passed(row: dict) -> bool:
    checks = row.get("worktree_verifications") or []
    return bool(checks) and all(c.get("passed") for c in checks)


def verifier_output(row: dict) -> str:
    checks = row.get("worktree_verifications") or []
    text = "\n".join((c.get("stdout_tail") or "") + (c.get("stderr_tail") or "") for c in checks)
    return text[-FEEDBACK_TAIL_CHARS:]


def run_arm(cfg: argparse.Namespace, opaque: str, arm: str, run_dir: Path) -> dict:
    manifest = tasks.load_manifest(opaque)
    instruction = manifest["instruction"]
    markers = [(tasks.private_dir(opaque) / "gold.patch").read_text()[:200], str(tasks.private_dir())]
    markers += manifest["hidden_fail_to_pass"]
    log = run_dir / f"{opaque}.{arm}.log"
    workspace = fresh_workspace(opaque, arm)
    started = time.time()
    attempts: list[dict] = []
    returncodes: list[int] = []
    if arm in ("raw", "a2-full"):
        key = f"{opaque}:{arm}:{cfg.run_id}"
        payload = task_payload(opaque, instruction, key, cfg.endpoint)
        assert_no_private_leak(json.dumps(payload), markers)
        lineage = "none" if arm == "raw" else str(run_dir / f"{opaque}.{arm}.lineage.sqlite")
        emit = run_dir / f"{opaque}.{arm}.attempts.jsonl"
        emit.unlink(missing_ok=True)
        returncodes.append(run_a2ctl(cfg, workspace, payload, 1 if arm == "raw" else cfg.attempts, lineage, emit, log))
        attempts = read_rows(emit)
    else:
        feedback = ""
        for index in range(1, cfg.attempts + 1):
            key = f"{opaque}:{arm}:{cfg.run_id}:{index}:{uuid.uuid4()}"
            text = instruction + (STATELESS_FEEDBACK_HEADER + feedback if feedback else "")
            payload = task_payload(opaque, text, key, cfg.endpoint)
            assert_no_private_leak(json.dumps(payload), markers)
            # A fresh base copy per attempt: leftover worktree branches must not carry prior attempts.
            workspace = fresh_workspace(opaque, arm)
            emit = run_dir / f"{opaque}.{arm}.attempt{index}.jsonl"
            emit.unlink(missing_ok=True)
            returncodes.append(run_a2ctl(cfg, workspace, payload, 1, "none", emit, log))
            rows = read_rows(emit)
            attempts.extend(rows)
            if not rows or verifier_passed(rows[-1]):
                break
            feedback = verifier_output(rows[-1])
    final = next((r for r in attempts if verifier_passed(r)), attempts[-1] if attempts else None)
    infra = None
    if final is None:
        infra = "no_attempt_row"
    elif any(rc != 0 for rc in returncodes) and not attempts:
        infra = "a2ctl_failed"
    diff = (final or {}).get("patch_diff") or ""
    grade = tasks.grade_diff(opaque, diff) if final is not None else None
    shutil.rmtree(workspace.parent, ignore_errors=True)
    return {
        "opaque_id": opaque,
        "arm": arm,
        "run_id": cfg.run_id,
        "provider": cfg.provider,
        "reasoning_effort": cfg.reasoning_effort,
        "attempts_used": len(attempts),
        "visible_passed_attempt": next((i + 1 for i, r in enumerate(attempts) if verifier_passed(r)), None),
        "tokens_in": sum(int(r.get("tokens_in") or 0) for r in attempts),
        "tokens_out": sum(int(r.get("tokens_out") or 0) for r in attempts),
        "wall_secs": round(time.time() - started, 1),
        "a2ctl_returncodes": returncodes,
        "final_diff_sha256": hashlib.sha256(diff.encode()).hexdigest(),
        "final_diff_empty": not diff.strip(),
        "grade": grade,
        "infra_failure": infra,
    }


def shard_spec(value: str) -> tuple[int, int]:
    index, _, count = value.partition("/")
    i, n = int(index), int(count)
    if n < 1 or not 0 <= i < n:
        raise ValueError(f"invalid shard {value!r}")
    return i, n


def start_proxy(cfg: argparse.Namespace, run_dir: Path, tag: str) -> subprocess.Popen:
    port_file = run_dir / f"proxy.{tag}.port"
    port_file.unlink(missing_ok=True)
    allow = [arg for e in cfg.endpoint for arg in ("--allow", e.removeprefix("https://").split("/")[0] + ":443")]
    proc = subprocess.Popen(
        [sys.executable, str(Path(__file__).with_name("egress_proxy.py")), *allow,
         "--log", str(run_dir / f"proxy.{tag}.jsonl"), "--port-file", str(port_file)],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    )
    for _ in range(100):
        if port_file.exists() and port_file.read_text().strip():
            cfg.proxy_port = int(port_file.read_text())
            return proc
        time.sleep(0.1)
    proc.kill()
    raise RuntimeError("egress proxy did not start")


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sample", required=True, help="JSON list of opaque task ids, in run order")
    parser.add_argument("--arms", default=",".join(ARMS))
    parser.add_argument("--run-id", required=True)
    parser.add_argument("--seed", type=int, required=True)
    parser.add_argument("--attempts", type=int, default=3)
    parser.add_argument("--provider", default="codex/gpt-6-luna")
    parser.add_argument("--endpoint", action="append", default=None, help="model endpoint https URL (repeatable)")
    # Non-binding ceiling: Codex input counts include cached context; the per-attempt timeout is the real limit.
    parser.add_argument("--max-tokens", type=int, default=2_000_000)
    parser.add_argument("--reasoning-effort", default="high",
                        choices=["none", "minimal", "low", "medium", "high", "xhigh"])
    parser.add_argument("--timeout", type=int, default=1500)
    parser.add_argument("--a2ctl", type=Path, default=REPO_ROOT / "target" / "debug" / "a2ctl")
    parser.add_argument("--shard", default="0/1", help="i/n: run only tasks whose sample index % n == i")
    cfg = parser.parse_args(argv)
    cfg.endpoint = cfg.endpoint or ["https://chatgpt.com", "https://auth.openai.com"]
    cfg.codex_home = ensure_codex_home(tasks.e1_home() / "codex-home", Path.home() / ".codex" / "auth.json")
    arms = [a for a in cfg.arms.split(",") if a]
    unknown = set(arms) - set(ARMS)
    if unknown:
        raise SystemExit(f"unknown arms: {sorted(unknown)}")
    full_sample = json.loads(Path(cfg.sample).read_text())
    shard_index, shard_count = shard_spec(cfg.shard)
    sample = [t for i, t in enumerate(full_sample) if i % shard_count == shard_index]
    run_dir = tasks.private_dir() / "runs" / cfg.run_id
    run_dir.mkdir(parents=True, exist_ok=True)
    results = run_dir / f"results.shard{shard_index}of{shard_count}.jsonl"
    done = set()
    if results.exists():
        for line in results.read_text().splitlines():
            row = json.loads(line)
            done.add((row["opaque_id"], row["arm"]))
    order = arm_order(full_sample, cfg.seed)
    proxy = start_proxy(cfg, run_dir, f"shard{shard_index}of{shard_count}")
    try:
        for opaque in sample:
            pending = [a for a in order[opaque] if a in arms and (opaque, a) not in done]
            if not pending:
                continue
            tasks.prepare(opaque)
            try:
                for arm in pending:
                    row = run_arm(cfg, opaque, arm, run_dir)
                    with results.open("a") as handle:
                        handle.write(json.dumps(row, sort_keys=True) + "\n")
                    print(json.dumps({k: row[k] for k in ("opaque_id", "arm", "attempts_used", "infra_failure")}
                                     | {"resolved": (row["grade"] or {}).get("resolved")}), flush=True)
            finally:
                tasks.evict(opaque)
    finally:
        proxy.terminate()
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
