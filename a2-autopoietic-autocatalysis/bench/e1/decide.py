#!/usr/bin/env python3
"""E1 analysis: paired resolution by arm, exact one-sided McNemar, and the pre-registered decision.

Rules come from bench/external-value/protocol-v1.toml:
  primary  a2-full vs stateless-retry resolved rate, paired by task
  GO       difference >= +15 pp AND one-sided exact McNemar p <= 0.10
  INCONCLUSIVE  difference >= +15 pp with p > 0.10, or infrastructure exclusions > 20% of sample (= NO GO)
  NO GO    otherwise
A task with an infrastructure failure in any arm (after the one retry) is dropped for all arms and
replaced by the next reserve task in order.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import sys
from pathlib import Path

ARMS = ("raw", "stateless-retry", "a2-full")
GO_EFFECT_PP = 15.0
ALPHA = 0.10
MAX_INFRA_FRACTION = 0.20


def mcnemar_one_sided_p(b: int, c: int) -> float:
    """P(X >= b) for X ~ Binomial(b + c, 1/2): evidence that a2-full wins more discordant pairs."""
    n = b + c
    if n == 0:
        return 1.0
    return sum(math.comb(n, k) for k in range(b, n + 1)) / 2**n


def decision(diff_pp: float, p: float, infra_fraction: float) -> str:
    if infra_fraction > MAX_INFRA_FRACTION:
        return "INCONCLUSIVE"
    if diff_pp >= GO_EFFECT_PP:
        return "GO" if p <= ALPHA else "INCONCLUSIVE"
    return "NO GO"


def latest_rows(rows: list[dict]) -> dict[tuple[str, str], dict]:
    latest: dict[tuple[str, str], dict] = {}
    for r in rows:
        latest[(r["opaque_id"], r["arm"])] = r
    return latest


def _resolved(r: dict, key: str = "resolved") -> bool:
    return bool((r.get("grade") or {}).get(key))


def analyse(rows: list[dict], sample: list[str], reserve: list[str]) -> dict:
    latest = latest_rows(rows)

    def status(task: str) -> str:
        arms = [latest.get((task, a)) for a in ARMS]
        if any(r is None for r in arms):
            return "incomplete"
        if any(r.get("infra_failure") or r.get("grade") is None for r in arms):
            return "infra"
        return "ok"

    paired, dropped, incomplete, reserves_used = [], [], [], []
    reserve_queue = list(reserve)
    for task in sample:
        state = status(task)
        if state == "ok":
            paired.append(task)
        elif state == "incomplete":
            incomplete.append(task)
        else:
            dropped.append(task)
            while reserve_queue:
                candidate = reserve_queue.pop(0)
                if status(candidate) == "ok":
                    paired.append(candidate)
                    reserves_used.append(candidate)
                    break
    n = len(paired)
    resolved = {a: sum(_resolved(latest[(t, a)]) for t in paired) for a in ARMS}
    hidden = {a: sum(_resolved(latest[(t, a)], "hidden_resolved") for t in paired) for a in ARMS}
    b = sum(_resolved(latest[(t, "a2-full")]) and not _resolved(latest[(t, "stateless-retry")]) for t in paired)
    c = sum(_resolved(latest[(t, "stateless-retry")]) and not _resolved(latest[(t, "a2-full")]) for t in paired)
    diff_pp = (b - c) / n * 100 if n else 0.0
    p = mcnemar_one_sided_p(b, c)
    infra_fraction = len(dropped) / len(sample) if sample else 0.0

    def pair(x: str, y: str) -> dict:
        bx = sum(_resolved(latest[(t, x)]) and not _resolved(latest[(t, y)]) for t in paired)
        cx = sum(_resolved(latest[(t, y)]) and not _resolved(latest[(t, x)]) for t in paired)
        return {"wins": bx, "losses": cx, "diff_pp": (bx - cx) / n * 100 if n else 0.0,
                "one_sided_p": mcnemar_one_sided_p(bx, cx)}

    tokens = {a: sum(int(latest[(t, a)].get("tokens_in") or 0) + int(latest[(t, a)].get("tokens_out") or 0)
                     for t in paired) for a in ARMS}
    wall = {a: round(sum(float(latest[(t, a)].get("wall_secs") or 0) for t in paired), 1) for a in ARMS}
    return {
        "n_sample": len(sample),
        "n_paired": n,
        "paired_tasks": paired,
        "dropped_infra": dropped,
        "reserves_used": reserves_used,
        "incomplete": incomplete,
        "resolved": resolved,
        "resolved_rate": {a: (resolved[a] / n if n else 0.0) for a in ARMS},
        "hidden_resolved": hidden,
        "b": b,
        "c": c,
        "diff_pp": diff_pp,
        "one_sided_p": p,
        "infra_fraction": infra_fraction,
        "decision": decision(diff_pp, p, infra_fraction) if not incomplete else "PENDING",
        "secondary": {
            "stateless-retry_vs_raw": pair("stateless-retry", "raw"),
            "a2-full_vs_raw": pair("a2-full", "raw"),
        },
        "tokens": tokens,
        "wall_secs": wall,
    }


def load_rows(run_dir: Path) -> list[dict]:
    rows = []
    for path in sorted(run_dir.glob("results.shard*.jsonl")):
        rows += [json.loads(line) for line in path.read_text().splitlines() if line.strip()]
    return rows


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--run-dir", type=Path, required=True)
    parser.add_argument("--sample-json", type=Path, required=True, help="docs/.../e1/sample.json")
    parser.add_argument("--out", type=Path, required=True, help="analysis JSON to write")
    args = parser.parse_args(argv)
    split = json.loads(args.sample_json.read_text())
    rows = load_rows(args.run_dir)
    result = analyse(rows, split["sample"], split["reserve"])
    result["rows_sha256"] = hashlib.sha256(json.dumps(rows, sort_keys=True).encode()).hexdigest()
    args.out.write_text(json.dumps(result, indent=1, sort_keys=True) + "\n")
    print(json.dumps({k: result[k] for k in ("n_paired", "resolved", "b", "c", "diff_pp", "one_sided_p",
                                              "dropped_infra", "incomplete", "decision")}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
