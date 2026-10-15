#!/usr/bin/env python3
"""Evaluate one frozen genome on an opaque task list using the E1 shard/resume protocol."""

from __future__ import annotations

import argparse
import json
import shutil
import sys
import uuid
from pathlib import Path

E1_DIR = Path(__file__).resolve().parents[1] / "e1"
if str(E1_DIR) not in sys.path:
    sys.path.insert(0, str(E1_DIR))

from bench.e1 import run as e1run, tasks
from bench.evolve import codex_homes, episode, lock

REPO_ROOT = Path(__file__).resolve().parents[2]


def shard_spec(value: str) -> tuple[int, int]:
    index, separator, count = value.partition("/")
    if not separator:
        raise ValueError("shard must be i/n")
    i, n = int(index), int(count)
    if n < 1 or not 0 <= i < n:
        raise ValueError("invalid shard {!r}".format(value))
    return i, n


def _task_list(value: str) -> list[str]:
    path = Path(value)
    try:
        is_file = path.is_file()
    except OSError:
        is_file = False
    raw = path.read_text() if is_file else value
    parsed = json.loads(raw)
    if not isinstance(parsed, list) or not all(isinstance(item, str) for item in parsed):
        raise ValueError("tasks must be a JSON list of opaque id strings")
    return parsed


def _read_done(path: Path) -> set[tuple[str, int, str]]:
    done = set()
    if path.exists():
        for line in path.read_text().splitlines():
            if line.strip():
                row = json.loads(line)
                done.add((row["opaque_id"], int(row["rep"]), row["label"]))
    return done


def _preserve_genome(source: Path, run_dir: Path, digest: str) -> Path:
    dest = run_dir / ("genome-" + digest[:12])
    if not dest.exists():
        shutil.copytree(source, dest)
    copied_digest = episode._genome_sha(dest)
    if copied_digest != digest:
        raise RuntimeError("preserved genome does not match the genome used by the episode")
    return dest


def evaluate_tasks(cfg: argparse.Namespace, full_tasks: list[str]) -> int:
    ok, mismatches = lock.verify()
    if not ok:
        print("evaluator lock mismatch: " + ", ".join(mismatches), file=sys.stderr)
        raise SystemExit(2)
    evaluator_sha256 = lock.compute()["evaluator_sha256"]

    shard_index, shard_count = shard_spec(cfg.shard)
    selected = [task for index, task in enumerate(full_tasks) if index % shard_count == shard_index]
    private_root = tasks.private_dir()
    run_dir = private_root / "evolve-runs" / cfg.run_id
    run_dir.mkdir(parents=True, exist_ok=True)
    rows_path = run_dir / "rows.shard{}of{}.jsonl".format(shard_index, shard_count)
    done = _read_done(rows_path)
    cfg.endpoint = ["https://chatgpt.com", "https://auth.openai.com"]
    cfg.provider = getattr(cfg, "provider", "codex/gpt-6-luna")
    cfg.max_tokens = getattr(cfg, "max_tokens", 2000000)
    cfg.reasoning_effort = "high"
    cfg.codex_home = codex_homes.ensure_role_home("agent")
    cfg.genome = Path(cfg.genome).resolve()
    proxy = e1run.start_proxy(cfg, run_dir, "shard{}of{}-{}".format(shard_index, shard_count, cfg.run_id))
    try:
        for opaque in selected:
            pending_reps = [rep for rep in range(cfg.repeat)
                            if (opaque, rep, cfg.label) not in done]
            if not pending_reps:
                continue
            tasks.prepare(opaque)
            try:
                for rep in pending_reps:
                    row = episode.run_episode(cfg, opaque, cfg.genome, run_dir, uuid.uuid4().hex)
                    if row.get("infra_failure"):
                        first = row["infra_failure"]
                        row = episode.run_episode(cfg, opaque, cfg.genome, run_dir, uuid.uuid4().hex)
                        row["infra_retry_of"] = first
                    digest = row.get("genome_sha256")
                    if not digest:
                        raise RuntimeError("episode row is missing genome_sha256")
                    preserved = _preserve_genome(cfg.genome, run_dir, digest)
                    row.update({"label": cfg.label, "rep": rep, "run_id": cfg.run_id,
                                "evaluator_sha256": evaluator_sha256,
                                "genome_dir": str(preserved)})
                    with rows_path.open("a") as output:
                        output.write(json.dumps(row, sort_keys=True) + "\n")
                    done.add((opaque, rep, cfg.label))
                    print(json.dumps({"opaque_id": opaque, "label": cfg.label, "rep": rep,
                                      "infra_failure": row.get("infra_failure"),
                                      "resolved": (row.get("grade") or {}).get("resolved")}), flush=True)
            finally:
                tasks.evict(opaque)
    finally:
        if proxy is not None:
            proxy.terminate()
    return 0


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--genome", type=Path, required=True)
    parser.add_argument("--tasks", required=True, help="JSON list of opaque ids, or a path containing it")
    parser.add_argument("--run-id", required=True)
    parser.add_argument("--label", required=True)
    parser.add_argument("--shard", default="0/1")
    parser.add_argument("--timeout", type=int, default=1500)
    parser.add_argument("--a2ctl", type=Path, default=REPO_ROOT / "target" / "debug" / "a2ctl")
    parser.add_argument("--repeat", type=int, default=1)
    args = parser.parse_args(argv)
    if args.repeat < 1:
        parser.error("--repeat must be at least 1")
    args.a2ctl = args.a2ctl.resolve()
    return evaluate_tasks(args, _task_list(args.tasks))


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
