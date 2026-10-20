#!/usr/bin/env python3
"""Run or analyse the single pre-registered sealed confirmation."""

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

E1_DIR = Path(__file__).resolve().parents[1] / "e1"
if str(E1_DIR) not in sys.path:
    sys.path.insert(0, str(E1_DIR))
REPO_ROOT = Path(__file__).resolve().parents[2]
if str(REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(REPO_ROOT))

from bench.e1 import decide as e1decide, run as e1run, tasks
from bench.evolve import codex_homes, episode, hardened, lock

DEFAULT_SAMPLE = REPO_ROOT / "docs/benchmark-results/evolve/sealed-sample.json"
ARMS = ("champion", "a2-full", "g0")
ENDPOINTS = ["https://chatgpt.com", "https://auth.openai.com"]
POWER_TABLE = {
    "baseline_win_rate": 0.04,
    "n=80": {"power_plus_10_pp": 0.61, "power_plus_15_pp": 0.87},
    "n=100": {"power_plus_10_pp": 0.72, "power_plus_15_pp": 0.93},
    "note": "A failed confirmation is reported with this table, never as 'no effect'.",
}


class Config(argparse.Namespace):
    pass


def assert_sealed_home() -> Path:
    raw = os.environ.get("A2_E1_HOME", "")
    home = Path(raw).expanduser()
    assert raw and home.as_posix().endswith("/.a2-e1/sealed"), (
        "confirmation requires A2_E1_HOME=~/.a2-e1/sealed"
    )
    return home


def deny_read_paths() -> list[Path]:
    home = Path.home()
    sealed = assert_sealed_home()
    paths = [tasks.private_dir(), home / ".a2-e1" / "private", home / "Projects", home / ".ssh"]
    return list(dict.fromkeys(path for path in paths if path.exists()))


def arm_order(sample: list[str], seed: int = 1201) -> dict[str, list[str]]:
    rng = random.Random(seed)
    order = {}
    for opaque in sample:
        arms = list(ARMS)
        rng.shuffle(arms)
        order[opaque] = arms
    return order


def validate_genomes(champion: Path, g0: Path) -> None:
    if champion.resolve() == g0.resolve() or (
        champion.is_dir() and g0.is_dir() and _genome_sha(champion) == _genome_sha(g0)
    ):
        raise SystemExit("refusing confirmation: champion genome is G0")
    if not champion.is_dir() or not g0.is_dir():
        raise SystemExit("champion and G0 genomes must be existing directories")


def _genome_sha(genome: Path) -> str:
    digest = hashlib.sha256()
    for path in sorted(p for p in genome.rglob("*") if p.is_file()):
        digest.update(path.relative_to(genome).as_posix().encode())
        digest.update(b"\0")
        digest.update(path.read_bytes())
    return digest.hexdigest()


def read_done(run_dir: Path) -> set[tuple[str, str]]:
    done = set()
    for path in sorted(run_dir.glob("rows.shard*.jsonl")):
        for line in path.read_text().splitlines():
            if line.strip():
                row = json.loads(line)
                done.add((row["opaque_id"], row["arm"]))
    return done


def run_with_infra_retry(runner, cfg, opaque: str, arm: str, run_dir: Path) -> dict:
    row = runner(cfg, opaque, arm, run_dir)
    if row.get("infra_failure"):
        first = row["infra_failure"]
        row = runner(cfg, opaque, arm, run_dir)
        row["infra_retry_of"] = first
    return row


def fresh_workspace(opaque: str, arm: str) -> Path:
    base = tasks.work_dir(opaque) / "repo"
    dest = tasks.work_dir(opaque) / f"confirm-{arm}" / "repo"
    shutil.rmtree(dest.parent, ignore_errors=True)
    dest.parent.mkdir(parents=True)
    if sys.platform == "darwin":
        subprocess.run(["cp", "-cR", str(base), str(dest)], check=True)
    else:
        shutil.copytree(base, dest, symlinks=True)
    return dest


def fresh_lineage(run_dir: Path, opaque: str, arm: str) -> str:
    path = run_dir / f"{opaque}.{arm}.lineage.sqlite"
    for suffix in ("", "-wal", "-shm"):
        Path(f"{path}{suffix}").unlink(missing_ok=True)
    return str(path)


def task_payload(opaque: str, issue: str) -> dict:
    return {
        "problem_statement": issue,
        "task_id": f"{opaque}:confirm:{uuid.uuid4()}",
        "verification_commands": [{
            "command": f"python3 {REPO_ROOT / 'bench/confirm/verify_cmd.py'} --task {opaque}",
            "expect_exit": 0,
        }],
        "no_external_solution_search": True,
        "network_policy": {"AllowList": ENDPOINTS},
    }


def run_a2ctl(cfg: Config, workspace: Path, payload: dict, arm: str,
              emit: Path, log: Path, lineage: str) -> tuple[int, list[dict]]:
    attempts = 1 if arm == "raw" else 3
    opaque = str(payload.get("task_id", "")).split(":", 1)[0]
    payload["verification_commands"] = [{
        "command": f"python3 {REPO_ROOT / 'bench/confirm/verify_cmd.py'} --task {opaque}",
        "expect_exit": 0,
    }]
    argv = [str(cfg.a2ctl), "run", "--provider", cfg.provider,
            "--attempts", str(attempts), "--max-tokens", str(cfg.max_tokens),
            "--timeout", str(cfg.timeout), "--network-policy", "allowlist:" + ",".join(cfg.endpoint),
            "--egress-proxy", f"127.0.0.1:{cfg.proxy_port}", "--workspace", str(workspace),
            "--lineage-db", lineage, "--verifier-feedback", "raw", "--prompt-profile", "external",
            "--emit-jsonl", str(emit)]
    for path in cfg.deny_read:
        argv.extend(("--deny-read", str(path)))
    with log.open("a") as handle:
        handle.write(json.dumps({"argv": argv[1:], "task_id": payload["task_id"]}) + "\n")
        handle.flush()
        env = {**os.environ, "CODEX_HOME": str(cfg.codex_home),
               "A2_CODEX_REASONING_EFFORT": cfg.reasoning_effort}
        proc = subprocess.run(argv, input=json.dumps(payload) + "\n", text=True, stdout=handle,
                              stderr=handle, cwd=REPO_ROOT, env=env,
                              timeout=cfg.timeout * attempts + 900)
    attempts_rows = e1run.read_rows(emit)
    return proc.returncode, attempts_rows


def run_a2_full(cfg: Config, opaque: str, arm: str, run_dir: Path) -> dict:
    manifest = tasks.load_manifest(opaque)
    workspace = fresh_workspace(opaque, arm)
    log = run_dir / f"{opaque}.{arm}.log"
    offset = e1run.log_offset(log)
    emit = run_dir / f"{opaque}.{arm}.attempts.jsonl"
    emit.unlink(missing_ok=True)
    payload = task_payload(opaque, manifest["problem_statement"])
    started = time.time()
    cfg.codex_home = codex_homes.ensure_role_home("confirm")
    lineage = "none" if arm == "raw" else fresh_lineage(run_dir, opaque, arm)
    returncode, attempts = run_a2ctl(cfg, workspace, payload, arm, emit, log, lineage)
    passed = [row for row in attempts if e1run.verifier_passed(row)]
    final = passed[0] if passed else (attempts[-1] if attempts else None)
    diff = (final or {}).get("patch_diff") or ""
    infra = e1run.classify_infra(attempts, [returncode], logs=[e1run.log_since(log, offset)])
    if not attempts and not infra:
        infra = "no_attempt_row"
    grade = hardened.grade_hardened(opaque, diff) if final is not None else None
    if final is not None and grade and grade.get("error"):
        infra = "grader_error"
    shutil.rmtree(workspace.parent, ignore_errors=True)
    return {"grade": grade, "tokens": sum(int(r.get("tokens_in") or 0) + int(r.get("tokens_out") or 0)
                                            for r in attempts),
            "wall_secs": round(time.time() - started, 1), "infra_failure": infra,
            "attempts_used": len(attempts), "a2ctl_returncode": returncode}


def run_episode_arm(cfg: Config, opaque: str, arm: str, run_dir: Path) -> dict:
    genome = cfg.champion_genome if arm == "champion" else cfg.g0_genome
    cfg.codex_home = codex_homes.ensure_role_home("confirm")
    cfg.genome = genome
    cfg.deny_read = deny_read_paths()
    cfg.endpoint = list(ENDPOINTS)
    cfg.provider, cfg.max_tokens, cfg.timeout = "codex/gpt-6-luna", 2000000, 1500
    cfg.reasoning_effort = "high"
    cfg.problem_statement = tasks.load_manifest(opaque)["problem_statement"]
    return episode.run_episode(cfg, opaque, genome, run_dir, uuid.uuid4().hex)


def run_arm(cfg: Config, opaque: str, arm: str, run_dir: Path) -> dict:
    if arm in ("a2-full", "raw"):
        result = run_a2_full(cfg, opaque, arm, run_dir)
        return result
    if arm in ("champion", "g0"):
        result = run_episode_arm(cfg, opaque, arm, run_dir)
        return {"grade": result.get("grade"),
                "tokens": int(result.get("tokens_in") or 0) + int(result.get("tokens_out") or 0),
                "wall_secs": result.get("wall_secs"), "infra_failure": result.get("infra_failure"),
                "agent_calls": result.get("agent_calls")}
    raise ValueError(f"unknown confirmation arm {arm!r}")


def _safe_run_arm(cfg: Config, opaque: str, arm: str, run_dir: Path) -> dict:
    try:
        return run_arm(cfg, opaque, arm, run_dir)
    except Exception as exc:  # infrastructure/evaluator failures are recorded and retried once
        return {"grade": None, "tokens": 0, "wall_secs": None,
                "infra_failure": "evaluator_error", "error": f"{type(exc).__name__}: {exc}"[:500]}


def _prepare_with_infra_retry(opaque: str) -> tuple[bool, str | None]:
    try:
        tasks.prepare(opaque)
        return True, None
    except Exception:
        try:
            tasks.prepare(opaque)
            return True, "setup_error"
        except Exception:
            return False, "setup_error"


def _read_sample(path: Path) -> dict:
    value = json.loads(path.read_text())
    return {"sample": value["sample"], "calibration": value.get("calibration", [])}


def run_confirmation(cfg: Config) -> int:
    home = assert_sealed_home()
    if not cfg.calibration:  # the raw calibration uses no genome; only the confirmation needs champion != G0
        validate_genomes(cfg.champion_genome, cfg.g0_genome)
    ok, mismatches = lock.verify()
    if not ok:
        print("evaluator lock mismatch: " + ", ".join(mismatches), file=sys.stderr)
        raise SystemExit(2)
    split = _read_sample(cfg.sample_json)
    sample = split["calibration"] if cfg.calibration else split["sample"]
    arms = ["raw"] if cfg.calibration else cfg.arms.split(",")
    allowed_arms = set(ARMS) | ({"raw"} if cfg.calibration else set())
    if not arms or any(arm not in allowed_arms for arm in arms):
        raise SystemExit("--arms must be a comma-separated subset of champion,a2-full,g0")
    shard_index, shard_count = e1run.shard_spec(cfg.shard)
    selected = [task for index, task in enumerate(sample) if index % shard_count == shard_index]
    run_dir = tasks.private_dir() / "confirm-runs" / cfg.run_id
    run_dir.mkdir(parents=True, exist_ok=True)
    rows_path = run_dir / f"rows.shard{shard_index}of{shard_count}.jsonl"
    done = read_done(run_dir)
    cfg.endpoint, cfg.provider = list(ENDPOINTS), "codex/gpt-6-luna"
    cfg.max_tokens, cfg.timeout, cfg.instruction = 2000000, 1500, "issue-only"
    cfg.reasoning_effort = "high"
    cfg.deny_read = deny_read_paths()
    cfg.codex_home = codex_homes.ensure_role_home("confirm")
    proxy = e1run.start_proxy(cfg, run_dir, f"shard{shard_index}of{shard_count}-{cfg.run_id}")
    ordered = {opaque: ["raw"] for opaque in sample} if cfg.calibration else arm_order(sample)
    try:
        for opaque in selected:
            pending = [arm for arm in ordered[opaque] if arm in arms and (opaque, arm) not in done]
            if not pending:
                continue
            try:
                prepared, prepare_retry = _prepare_with_infra_retry(opaque)
                for arm in pending:
                    if prepared:
                        row = run_with_infra_retry(_safe_run_arm, cfg, opaque, arm, run_dir)
                        if prepare_retry:
                            row["prepare_retry_of"] = prepare_retry
                    else:
                        row = {"grade": None, "tokens": 0, "wall_secs": None,
                               "infra_failure": "setup_error", "infra_retry_of": "setup_error"}
                    champion_sha = _genome_sha(cfg.champion_genome)
                    output = {"opaque_id": opaque, "arm": arm, "grade": row.get("grade"),
                              "tokens": int(row.get("tokens") or 0), "wall_secs": row.get("wall_secs"),
                              "infra_failure": row.get("infra_failure"),
                              "champion_genome_sha256": champion_sha,
                              "infra_retry_of": row.get("infra_retry_of")}
                    with rows_path.open("a") as stream:
                        stream.write(json.dumps(output, sort_keys=True) + "\n")
                    done.add((opaque, arm))
                    print(json.dumps({"opaque_id": opaque, "arm": arm,
                                      "infra_failure": output["infra_failure"],
                                      "resolved": bool((output["grade"] or {}).get("resolved"))}), flush=True)
            finally:
                tasks.evict(opaque)
    finally:
        if proxy is not None:
            proxy.terminate()
    return 0


def _resolved(row: dict | None) -> bool:
    return bool((row or {}).get("grade", {}).get("resolved"))


def _row_status(row: dict | None) -> bool:
    return bool(row is not None and not row.get("infra_failure") and row.get("grade") is not None)


def analyse(rows: list[dict], sample: list[str], arms: list[str], sample_size: int | None = None) -> dict:
    latest = {(row["opaque_id"], row["arm"]): row for row in rows}
    primary_tasks = [task for task in sample if all(_row_status(latest.get((task, arm)))
                                                    for arm in ("champion", "a2-full"))]
    dropped = [task for task in sample if any(
        (latest.get((task, arm)) is not None and not _row_status(latest.get((task, arm))))
        for arm in ("champion", "a2-full"))]
    incomplete = [task for task in sample if task not in primary_tasks and task not in dropped]
    champ = sum(_resolved(latest[(task, "champion")]) for task in primary_tasks)
    a2 = sum(_resolved(latest[(task, "a2-full")]) for task in primary_tasks)
    b = sum(_resolved(latest[(task, "champion")]) and not _resolved(latest[(task, "a2-full")])
            for task in primary_tasks)
    c = sum(_resolved(latest[(task, "a2-full")]) and not _resolved(latest[(task, "champion")])
            for task in primary_tasks)
    p = e1decide.mcnemar_one_sided_p(b, c)
    denom = len(sample) if sample_size is None else sample_size
    infra_fraction = len(dropped) / denom if denom else 0.0

    def pair(x: str, y: str) -> dict:
        paired = [task for task in sample if _row_status(latest.get((task, x)))
                  and _row_status(latest.get((task, y)))]
        wins = sum(_resolved(latest[(task, x)]) and not _resolved(latest[(task, y)]) for task in paired)
        losses = sum(_resolved(latest[(task, y)]) and not _resolved(latest[(task, x)]) for task in paired)
        return {"n_paired": len(paired), "resolved": {x: sum(_resolved(latest[(t, x)]) for t in paired),
                                                         y: sum(_resolved(latest[(t, y)]) for t in paired)},
                "wins": wins, "losses": losses,
                "one_sided_p": e1decide.mcnemar_one_sided_p(wins, losses)}

    return {
        "n_sample": denom, "primary": {"arms": ["champion", "a2-full"], "n_paired": len(primary_tasks),
                    "paired_tasks": primary_tasks, "resolved": {"champion": champ, "a2-full": a2},
                    "discordant_b_champion_wins": b, "discordant_c_a2_full_wins": c,
                    "b": b, "c": c, "one_sided_p": p},
        "secondary_champion_vs_g0": pair("champion", "g0"),
        "dropped_infra": dropped, "incomplete": incomplete,
        "infra_fraction": infra_fraction,
        "promote": p <= 0.05 and infra_fraction <= 0.20,
        "power_table": POWER_TABLE,
    }


def load_rows(run_dir: Path) -> list[dict]:
    rows = []
    for path in sorted(run_dir.glob("rows.shard*.jsonl")):
        rows.extend(json.loads(line) for line in path.read_text().splitlines() if line.strip())
    return rows


def decide(run_dir: Path, sample_json: Path, out: Path) -> dict:
    sample = _read_sample(sample_json)["sample"]
    result = analyse(load_rows(run_dir), sample, list(ARMS), len(sample))
    result["rows_sha256"] = hashlib.sha256(json.dumps(load_rows(run_dir), sort_keys=True).encode()).hexdigest()
    out.write_text(json.dumps(result, indent=1, sort_keys=True) + "\n")
    return result


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    run = sub.add_parser("run")
    run.add_argument("--arms", default="champion,a2-full,g0")
    run.add_argument("--calibration", action="store_true")
    run.add_argument("--champion-genome", type=Path, required=True)
    run.add_argument("--g0-genome", type=Path, required=True)
    run.add_argument("--sample-json", type=Path, default=DEFAULT_SAMPLE)
    run.add_argument("--run-id", default=None)
    run.add_argument("--shard", default="0/1")
    run.add_argument("--a2ctl", type=Path, default=e1run.REPO_ROOT / "target" / "debug" / "a2ctl")
    analysis = sub.add_parser("decide")
    analysis.add_argument("--run-dir", type=Path, required=True)
    analysis.add_argument("--sample-json", type=Path, default=DEFAULT_SAMPLE)
    analysis.add_argument("--out", type=Path, required=True)
    args = parser.parse_args(argv)
    if args.command == "decide":
        result = decide(args.run_dir, args.sample_json, args.out)
        print(json.dumps({"primary": result["primary"], "infra_fraction": result["infra_fraction"],
                          "promote": result["promote"]}, sort_keys=True))
        return 0
    cfg = args
    cfg.run_id = cfg.run_id or uuid.uuid4().hex
    cfg.champion_genome = cfg.champion_genome.resolve()
    cfg.g0_genome = cfg.g0_genome.resolve()
    return run_confirmation(cfg)  # run_confirmation enforces champion != G0 for confirmation runs


if __name__ == "__main__":
    raise SystemExit(main())
