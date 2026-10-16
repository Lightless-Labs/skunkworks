#!/usr/bin/env python3
"""Unattended, resumable harness-evolution orchestrator."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path

E1_DIR = Path(__file__).resolve().parents[1] / "e1"
if str(E1_DIR) not in sys.path:
    sys.path.insert(0, str(E1_DIR))

# Run as a script (python3 bench/evolve/loop.py): make `bench` importable from the repo root.
sys.path.insert(0, str(Path(__file__).resolve().parents[2]))

from bench.e1 import tasks
from bench.evolve import episode, lock, researcher, stats

REPO_ROOT = Path(__file__).resolve().parents[2]
STATE_VERSION = 1
LOG_REL = "docs/benchmark-results/evolve/log.jsonl"


def _write_state(path: Path, value: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(".json.tmp")
    temporary.write_text(json.dumps(value, indent=1, sort_keys=True) + "\n")
    temporary.replace(path)


def _read_json_list(value) -> list:
    if isinstance(value, (list, tuple)):
        return list(value)
    path = Path(value)
    raw = path.read_text() if path.is_file() else str(value)
    result = json.loads(raw)
    if not isinstance(result, list):
        raise ValueError("expected a JSON list")
    return result


def _aggregate_rates(rows: list[dict]) -> dict:
    counts = {}
    for row in rows:
        if row.get("infra_failure") or not row.get("opaque_id"):
            continue
        task = row["opaque_id"]
        good, total = counts.get(task, (0, 0))
        counts[task] = (good + int(bool((row.get("grade") or {}).get("resolved"))), total + 1)
    return counts


def _merge_rates(left: dict, right: dict) -> dict:
    result = dict(left)
    for task, (good, total) in right.items():
        old_good, old_total = result.get(task, (0, 0))
        result[task] = (old_good + good, old_total + total)
    return result


def _rows_for_run(run_id: str) -> list[dict]:
    root = tasks.private_dir() / "evolve-runs" / run_id
    return stats.load_rows(root) if root.exists() else []


def _evaluate(cfg, genome: Path, task_ids: list[str], run_id: str, label: str, repeat: int = 1) -> list[dict]:
    """Run all evaluator shards, then return their rows. Dry-run uses synthetic rows only."""
    if getattr(cfg, "dry_run", False):
        return _dry_evaluate(cfg, genome, task_ids, run_id, label, repeat)
    run_root = tasks.private_dir() / "evolve-runs" / run_id
    run_root.mkdir(parents=True, exist_ok=True)
    task_file = run_root / "tasks.json"
    task_file.write_text(json.dumps(task_ids) + "\n")
    commands = []
    for shard in range(cfg.shards):
        commands.append([sys.executable, str(Path(__file__).with_name("evaluate.py")),
                         "--genome", str(genome), "--tasks", str(task_file), "--run-id", run_id,
                         "--label", label, "--shard", "{}/{}".format(shard, cfg.shards),
                         "--repeat", str(repeat)])
    processes = [subprocess.Popen(command, cwd=REPO_ROOT) for command in commands]
    errors = []
    for process in processes:
        code = process.wait()
        if code:
            errors.append("evaluate shard exited {}".format(code))
    if errors:
        raise RuntimeError("; ".join(errors))
    return stats.load_rows(run_root, label)


def _dry_evaluate(cfg, genome: Path, task_ids: list[str], run_id: str, label: str, repeat: int = 1) -> list[dict]:
    """Synthetic evaluator used only by --dry-run; it never calls external programs."""
    scenario = getattr(cfg, "_dry_scenario", "keep")
    fingerprint = hashlib.sha256((str(genome) + label).encode()).digest()[0]
    resolved = scenario == "keep" or (scenario == "discard" and fingerprint % 2 == 0)
    rows = []
    for task in task_ids:
        for rep in range(repeat):
            rows.append({"opaque_id": task, "rep": rep, "run_id": run_id, "label": label,
                         "infra_failure": None, "tokens_in": 1000, "tokens_out": 1000,
                         "wall_secs": 1, "grade": {"resolved": bool(resolved)}})
    return rows


def _dry_run_researcher(cfg, workspace: Path, exp_id: str, run_dir: Path, scenario: str | None = None) -> dict:
    """Create a small synthetic policy patch for exercising the state machine."""
    scenario = scenario or "keep"
    if scenario == "invalid":
        return {"patch_diff": "", "tokens_in": 10, "tokens_out": 10, "returncode": 0}
    old = (workspace / "genome" / "policy.py").read_text()
    new = "# synthetic candidate\n" + old
    import difflib
    policy_patch = "".join(difflib.unified_diff(old.splitlines(keepends=True), new.splitlines(keepends=True),
                                                fromfile="a/genome/policy.py", tofile="b/genome/policy.py"))
    diff = ("diff --git a/genome/policy.py b/genome/policy.py\n" + policy_patch +
            "diff --git a/genome/HYPOTHESIS.txt b/genome/HYPOTHESIS.txt\nnew file mode 100644\n"
            "--- /dev/null\n+++ b/genome/HYPOTHESIS.txt\n@@ -0,0 +1 @@\n+Synthetic structural hypothesis\n")
    return {"patch_diff": diff, "tokens_in": 10, "tokens_out": 10, "returncode": 0}


def _commit_experiment(repo: Path, exp_id: str, decision: str, hypothesis: str, paths: list[str]) -> str:
    exact_paths = list(paths)
    subprocess.run(["git", "add", "--", *exact_paths], cwd=repo, check=True)
    message = "evolve(a2): {} {} — {}".format(exp_id, decision, hypothesis.replace("\n", " "))
    subprocess.run(["git", "commit", "-m", message, "--", *exact_paths], cwd=repo, check=True)
    env = {**os.environ, "SSH_AUTH_SOCK": str(Path.home() / ".ssh" / "agent.sock")}
    try:
        subprocess.run(["ssh-add", "--apple-load-keychain"], cwd=repo, env=env, check=True,
                       stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    except Exception as exc:
        return "ssh agent setup {}: {}".format(type(exc).__name__, exc)
    try:
        subprocess.run(["git", "push", "origin", "main"], cwd=repo, env=env, check=True,
                       stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        return ""
    except Exception as exc:
        return "{}: {}".format(type(exc).__name__, exc)


def _candidate_diff(champion: Path, candidate: Path) -> str:
    """Build a textual genome-only diff without invoking git."""
    import difflib
    pieces = []
    names = sorted(set(p.relative_to(champion).as_posix() for p in champion.rglob("*") if p.is_file()) |
                   set(p.relative_to(candidate).as_posix() for p in candidate.rglob("*") if p.is_file()))
    for name in names:
        if name == "HYPOTHESIS.txt":
            continue
        before = (champion / name).read_text().splitlines(keepends=True) if (champion / name).is_file() else []
        after = (candidate / name).read_text().splitlines(keepends=True) if (candidate / name).is_file() else []
        if before != after:
            pieces.append("diff --git a/genome/{} b/genome/{}\n".format(name, name))
            pieces.extend(difflib.unified_diff(before, after, fromfile="a/genome/" + name,
                                               tofile="b/genome/" + name, n=3))
    return "".join(pieces)


def _load_champion_data(cfg, state):
    if state.get("champion_rows") is not None:
        rows = state["champion_rows"]
    else:
        rows = []
        for run_id in cfg.champion_runs:
            rows.extend(_rows_for_run(run_id))
    rates = {key: tuple(value) for key, value in state.get("champion_rates", {}).items()}
    if not rates:
        rates = _aggregate_rates(rows)
    return rows, rates


def _stats_ready_rates(rates: dict) -> dict:
    return {key: tuple(value) for key, value in rates.items()}


def _new_state(cfg, repo: Path, champion_rows: list[dict], rates: dict) -> dict:
    return {"version": STATE_VERSION, "next_exp": 1, "completed": 0, "tokens": 0,
            "champion_genome": str(repo / "genome"),
            "champion_exp_id": "G0",
            "champion_rows": champion_rows,
            "champion_rates": {key: list(value) for key, value in rates.items()},
            "last_control": time.time(), "log_rows": [], "current": None,
            "provider_error_streak": 0, "provider_backoffs": 0}


def _begin_experiment(state: dict, cfg, screen: list[str], full_tasks: list[str], run_root: Path) -> dict:
    index = int(state["next_exp"])
    exp_id = "exp-{:04d}".format(index)
    candidate = run_root / exp_id / "candidate"
    current = {"exp_id": exp_id, "stage": "researcher", "candidate": str(candidate),
               "parent": state.get("champion_exp_id", "G0"), "tokens": 0,
               "screen_tasks": screen, "full_tasks": full_tasks,
               "started": time.time()}
    if getattr(cfg, "dry_run", False):
        scenarios = getattr(cfg, "dry_run_scenarios", [])
        current["dry_scenario"] = scenarios[index - 1] if index - 1 < len(scenarios) else "keep"
    # Outside the private dir: the researcher's Codex is denied ~/.a2-e1/private, and the workspace
    # holds only researcher-visible material (genome copy, program.md, log, digests).
    current["workspace"] = str(tasks.e1_home() / "evolve-workspaces" / exp_id / "workspace")
    current["researcher_run_dir"] = str(run_root / exp_id / "researcher")
    return current


def _record_failure(current: dict, exc: Exception) -> dict:
    return {"exp_id": current["exp_id"], "parent": current.get("parent"),
            "hypothesis": current.get("hypothesis", ""), "decision": "invalid",
            "reason": "experiment exception: {}".format(exc), "delta_A": None, "delta_B": None,
            "p_B": None, "full44_delta": None, "tokens": int(current.get("tokens", 0)),
            "wall_secs": round(time.time() - current.get("started", time.time()), 1)}


def _finish_experiment(state: dict, current: dict, row: dict, diff: str, repo: Path,
                       dry_run: bool = False) -> None:
    archive_rel = "docs/benchmark-results/evolve/archive/{}.patch".format(current["exp_id"])
    log_path = repo / LOG_REL
    archive_path = repo / archive_rel
    log_path.parent.mkdir(parents=True, exist_ok=True)
    archive_path.parent.mkdir(parents=True, exist_ok=True)
    with log_path.open("a") as stream:
        stream.write(json.dumps(row, sort_keys=True) + "\n")
    archive_path.write_text(diff)
    paths = [LOG_REL, archive_rel]
    if row["decision"] == "keep":
        destination = repo / "genome"
        backup = destination.with_name("genome.evolve-copy")
        if backup.exists():
            shutil.rmtree(backup)
        shutil.copytree(destination, backup)
        shutil.rmtree(destination)
        shutil.copytree(current["candidate"], destination)
        (destination / "HYPOTHESIS.txt").unlink(missing_ok=True)
        shutil.rmtree(backup)
        paths.append("genome")
        rows = current.get("stage_b_rows", []) + current.get("full_rows", [])
        state["champion_rows"] = rows
        state["champion_rates"] = {key: list(value) for key, value in _aggregate_rates(rows).items()}
        state["champion_genome"] = str(destination)
        state["champion_exp_id"] = current["exp_id"]
    if dry_run:
        row["commit_status"] = "skipped in dry-run"
    else:
        try:
            push_error = _commit_experiment(repo, current["exp_id"], row["decision"], row.get("hypothesis", ""), paths)
            if push_error:
                row["push_error"] = push_error
        except Exception as exc:
            row["commit_error"] = str(exc)
    if "push_error" in row or "commit_error" in row or "commit_status" in row:
        lines = log_path.read_text().splitlines()
        if lines:
            lines[-1] = json.dumps(row, sort_keys=True)
            log_path.write_text("\n".join(lines) + "\n")
    state.setdefault("log_rows", []).append(row)
    state["completed"] = int(state.get("completed", 0)) + 1
    state["next_exp"] = int(state.get("next_exp", 1)) + 1
    state["tokens"] = int(state.get("tokens", 0)) + int(row.get("tokens") or 0)
    state["current"] = None


def _update_infra_guard(state: dict, rows: list[dict]) -> bool:
    for row in rows:
        if row.get("infra_failure") == "provider_error":
            state["provider_error_streak"] = int(state.get("provider_error_streak", 0)) + 1
        else:
            state["provider_error_streak"] = 0
        if state["provider_error_streak"] >= 3:
            if state.get("provider_backoffs", 0) >= 3:
                return True
            state["provider_backoffs"] = int(state.get("provider_backoffs", 0)) + 1
            time.sleep(1800)
            state["provider_error_streak"] = 0
    return False


def run_loop(cfg) -> dict:
    private = tasks.private_dir()
    state_path = private / "evolve-loop" / "state.json"
    repo = Path(getattr(cfg, "repo_root", REPO_ROOT)).resolve()
    if not getattr(cfg, "dry_run", False):
        ok, mismatches = lock.verify()
        if not ok:
            raise RuntimeError("evaluator lock mismatch: " + ", ".join(mismatches))
    screen = [str(item) for item in _read_json_list(cfg.screen)]
    full_tasks = [str(item) for item in _read_json_list(cfg.dev_tasks)]
    if state_path.exists():
        state = json.loads(state_path.read_text())
    else:
        champion_rows = []
        for run_id in cfg.champion_runs:
            champion_rows.extend(_rows_for_run(run_id))
        state = _new_state(cfg, repo, champion_rows, _aggregate_rates(champion_rows))
        if getattr(cfg, "dry_run", False) and not state["champion_rates"]:
            state["champion_rates"] = {task: [0, 1] for task in full_tasks}
            state["champion_rows"] = [{"opaque_id": task, "grade": {"resolved": False}, "label": "initial"}
                                       for task in full_tasks]
    run_root = private / "evolve-loop" / "runs"
    run_root.mkdir(parents=True, exist_ok=True)
    stop_reason = None
    while state.get("completed", 0) < cfg.max_experiments:
        if int(state.get("tokens", 0)) >= cfg.nightly_token_cap:
            stop_reason = "nightly token cap exceeded"
            break
        if not getattr(cfg, "dry_run", False):
            ok, mismatches = lock.verify()
            if not ok:
                stop_reason = "evaluator lock mismatch: " + ", ".join(mismatches)
                break
        now = time.time()
        if state.get("current") is None and now - float(state.get("last_control", 0)) >= cfg.control_every_hours * 3600:
            try:
                champion = Path(state["champion_genome"])
                control_id = "control-{}".format(time.strftime("%Y%m%dT%H%M%SZ", time.gmtime(now)))
                control_rows = _evaluate(cfg, champion, screen, control_id, control_id, 1)
                state["champion_rows"].extend(control_rows)
                state["champion_rates"] = {key: list(value) for key, value in
                                            _merge_rates(_stats_ready_rates(state["champion_rates"]),
                                                         _aggregate_rates(control_rows)).items()}
                state["last_control"] = now
                if _update_infra_guard(state, control_rows):
                    stop_reason = "provider_error persisted after three backoffs"
                    break
                _write_state(state_path, state)
            except Exception as exc:
                state["last_control"] = now
                state["control_error"] = str(exc)
                _write_state(state_path, state)
        if state.get("current") is None:
            state["current"] = _begin_experiment(state, cfg, screen, full_tasks, run_root)
            _write_state(state_path, state)
        current = state["current"]
        try:
            if current["stage"] == "researcher":
                parent_genome = Path(state["champion_genome"])
                latest_rows = state.get("champion_rows", [])
                manifests = {}
                for row in latest_rows:
                    opaque = row.get("opaque_id")
                    if opaque:
                        try:
                            manifests[opaque] = tasks.load_manifest(opaque)
                        except Exception:
                            manifests[opaque] = {}
                digests = researcher.make_digests(latest_rows, manifests,
                                                  time.strftime("%Y%m%d", time.gmtime()))
                workspace = researcher.build_workspace(current["exp_id"], parent_genome,
                                                       state.get("log_rows", []), digests,
                                                       Path(current["workspace"]))
                scenario = current.get("dry_scenario")
                if getattr(cfg, "dry_run", False):
                    answer = _dry_run_researcher(cfg, workspace, current["exp_id"],
                                                 Path(current["researcher_run_dir"]), scenario)
                else:
                    answer = researcher.run_researcher(cfg, workspace, current["exp_id"],
                                                       Path(current["researcher_run_dir"]))
                current["tokens"] += int(answer.get("tokens_in", 0)) + int(answer.get("tokens_out", 0))
                if answer.get("returncode"):
                    raise RuntimeError("researcher exited {}".format(answer["returncode"]))
                candidate = Path(current["candidate"])
                applied = researcher.apply_candidate(parent_genome, answer.get("patch_diff", ""), candidate)
                current["hypothesis"] = applied.get("hypothesis", "")
                texts = [item.get("problem_statement", "") for item in digests]
                replay_states = getattr(cfg, "replay_states", [])
                reasons = [] if applied["ok"] else [applied["reason"]]
                if applied["ok"]:
                    reasons.extend(researcher.validate_candidate(candidate, parent_genome, texts, replay_states))
                current["candidate_diff"] = answer.get("patch_diff", "")
                if scenario == "invalid" and not reasons:
                    reasons.append("dry-run invalid candidate")
                if reasons:
                    current["reason"] = "; ".join(reasons)
                    current["stage"] = "finish_invalid"
                else:
                    current["stage"] = "stage_a"
                _write_state(state_path, state)
            if current["stage"] == "stage_a":
                run_id = current.setdefault("stage_a_run_id", current["exp_id"] + "-A")
                cfg._dry_scenario = current.get("dry_scenario")
                stage_rows = current.get("stage_a_rows")
                if stage_rows is None:
                    stage_rows = _evaluate(cfg, Path(current["candidate"]), screen, run_id,
                                           current["exp_id"] + "-A", 1)
                    current["stage_a_rows"] = stage_rows
                    current["tokens"] += sum(int(row.get("tokens_in") or 0) + int(row.get("tokens_out") or 0)
                                              for row in stage_rows)
                    if _update_infra_guard(state, stage_rows):
                        raise RuntimeError("provider_error persisted after three backoffs")
                decision = stats.screen_decision(stage_rows, _stats_ready_rates(state["champion_rates"]), screen)
                current["stage_a"] = decision
                current["stage"] = "stage_b" if decision.get("pass") else "finish_discard"
                _write_state(state_path, state)
            if current["stage"] == "stage_b":
                cfg._dry_scenario = current.get("dry_scenario")
                b_id = current.setdefault("stage_b_run_id", current["exp_id"] + "-B")
                f_id = current.setdefault("full_run_id", current["exp_id"] + "-full")
                if current.get("stage_b_rows") is None:
                    current["stage_b_rows"] = _evaluate(cfg, Path(current["candidate"]), screen, b_id,
                                                        current["exp_id"] + "-B", 2)
                    current["tokens"] += sum(int(row.get("tokens_in") or 0) + int(row.get("tokens_out") or 0)
                                              for row in current["stage_b_rows"])
                    _write_state(state_path, state)
                    if _update_infra_guard(state, current["stage_b_rows"]):
                        raise RuntimeError("provider_error persisted after three backoffs")
                if current.get("full_rows") is None:
                    current["full_rows"] = _evaluate(cfg, Path(current["candidate"]), full_tasks, f_id,
                                                     current["exp_id"] + "-full", 1)
                    current["tokens"] += sum(int(row.get("tokens_in") or 0) + int(row.get("tokens_out") or 0)
                                              for row in current["full_rows"])
                    _write_state(state_path, state)
                    if _update_infra_guard(state, current["full_rows"]):
                        raise RuntimeError("provider_error persisted after three backoffs")
                decision = stats.stage_b_decision(current["stage_b_rows"], current["full_rows"],
                                                 _stats_ready_rates(state["champion_rates"]), screen, full_tasks)
                current["stage_b"] = decision
                current["stage"] = "finish_keep" if decision.get("keep") else "finish_discard"
                _write_state(state_path, state)
            if current["stage"].startswith("finish_"):
                decision_name = {"finish_invalid": "invalid", "finish_discard": "discard",
                                 "finish_keep": "keep"}[current["stage"]]
                a = current.get("stage_a", {})
                b = current.get("stage_b", {})
                reason = current.get("reason", "")
                if decision_name == "discard":
                    reason = reason or ("stage A failed" if not b else "stage B failed")
                row = {"exp_id": current["exp_id"], "parent": current.get("parent"),
                       "hypothesis": current.get("hypothesis", ""),
                       "genome_diff_sha256": hashlib.sha256(current.get("candidate_diff", "").encode()).hexdigest(),
                       "evaluator_sha256": lock.compute().get("evaluator_sha256") if not cfg.dry_run else "dry-run",
                       "rows_sha256": hashlib.sha256(json.dumps(current.get("stage_a_rows", []) +
                                                                  current.get("stage_b_rows", []) +
                                                                  current.get("full_rows", []),
                                                                  sort_keys=True).encode()).hexdigest(),
                       "delta_A": a.get("delta"), "delta_B": b.get("delta"), "p_B": b.get("p_value"),
                       "full44_delta": b.get("full_delta"), "tokens": current.get("tokens", 0),
                       "wall_secs": round(time.time() - current.get("started", time.time()), 1),
                       "decision": decision_name, "reason": reason}
                _finish_experiment(state, current, row, current.get("candidate_diff", ""), repo,
                                   getattr(cfg, "dry_run", False))
                _write_state(state_path, state)
        except Exception as exc:
            failed = _record_failure(current, exc)
            _finish_experiment(state, current, failed, current.get("candidate_diff", ""), repo,
                               getattr(cfg, "dry_run", False))
            _write_state(state_path, state)
            if state.get("provider_backoffs", 0) >= 3 and state.get("provider_error_streak", 0) >= 3:
                stop_reason = "provider_error persisted after three backoffs"
                break
    return {"rows": state.get("log_rows", []), "champion_rates": _stats_ready_rates(state.get("champion_rates", {})),
            "tokens": state.get("tokens", 0), "stop_reason": stop_reason,
            "last_commit_paths": ( [LOG_REL, "docs/benchmark-results/evolve/archive/{}.patch".format(
                state.get("log_rows", [{}])[-1].get("exp_id", ""))] +
                (["genome"] if state.get("log_rows", [{}])[-1].get("decision") == "keep" else [])
                if state.get("log_rows") else [])}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dev-tasks", required=True)
    parser.add_argument("--screen", required=True)
    parser.add_argument("--champion-runs", nargs="+", required=True)
    parser.add_argument("--shards", type=int, default=4)
    parser.add_argument("--max-experiments", type=int, required=True)
    parser.add_argument("--nightly-token-cap", type=int, default=150000000)
    parser.add_argument("--control-every-hours", type=float, default=12)
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args(argv)
    if args.shards < 1 or args.max_experiments < 0 or args.nightly_token_cap < 0:
        parser.error("shards must be positive and experiment/token limits non-negative")
    try:
        result = run_loop(args)
    except RuntimeError as exc:
        print(str(exc), file=sys.stderr)
        return 2
    print(json.dumps({"completed": len(result["rows"]), "tokens": result["tokens"],
                      "stop_reason": result["stop_reason"]}, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
