#!/usr/bin/env python3
"""Monte Carlo keep/discard analysis for harness evolution."""

from __future__ import annotations

import argparse
import json
import math
import random
import sys
from pathlib import Path


def load_rows(run_dirs, label=None) -> list[dict]:
    if isinstance(run_dirs, (str, Path)):
        run_dirs = [run_dirs]
    rows = []
    for run_dir in run_dirs:
        for path in sorted(Path(run_dir).glob("rows.shard*of*.jsonl")):
            for line in path.read_text().splitlines():
                if line.strip():
                    row = json.loads(line)
                    if label is None or row.get("label") == label:
                        rows.append(row)
    return rows


def per_task_rates(rows, label) -> dict[str, tuple[int, int]]:
    counts = {}
    for row in rows:
        if row.get("label") != label or row.get("infra_failure"):
            continue
        task = row["opaque_id"]
        successes, total = counts.get(task, (0, 0))
        counts[task] = (successes + int(bool((row.get("grade") or {}).get("resolved"))), total + 1)
    return counts


def scored_tasks(champion_rates, tasks) -> list[str]:
    """Tasks with at least one clean champion run; a task the champion never ran cleanly has no
    baseline and is left out of both the null and the observed delta."""
    return [task for task in tasks if champion_rates.get(task, (0, 0))[1] > 0]


def null_delta_samples(champion_rates, tasks, runs, draws=20000, seed=0) -> list[float]:
    rng = random.Random(seed)
    tasks = scored_tasks(champion_rates, tasks)
    samples = []
    for _ in range(draws):
        delta_total = 0.0
        for task in tasks:
            successes, n = champion_rates[task]
            point = successes / n
            p = rng.betavariate(successes + 0.5, n - successes + 0.5)
            outcomes = sum(rng.random() < p for _ in range(runs)) / float(runs)
            delta_total += outcomes - point
        samples.append(delta_total)
    return sorted(samples)


def delta(candidate_rows, champion_rates, tasks) -> float:
    """Mean over the candidate's runs of sum_t (r_t - p_t). A task missing from a run (infra failure)
    is imputed at the champion's p_t, i.e. neutral, rather than scored as a failure."""
    tasks = scored_tasks(champion_rates, tasks)
    by_rep = {}
    for row in candidate_rows:
        if row.get("infra_failure") or row.get("opaque_id") not in tasks:
            continue
        rep = row.get("rep", 0)
        by_rep.setdefault(rep, {})[row["opaque_id"]] = int(bool((row.get("grade") or {}).get("resolved")))
    if not by_rep:
        return float("nan")
    point = {t: champion_rates[t][0] / champion_rates[t][1] for t in tasks}
    values = [sum(outcomes.get(t, point[t]) - point[t] for t in tasks) for outcomes in by_rep.values()]
    return sum(values) / len(values)


def p_value(observed, samples) -> float:
    if not samples:
        return 1.0
    return sum(sample >= observed for sample in samples) / float(len(samples))


def quantile(samples, q) -> float:
    if not samples:
        raise ValueError("quantile requires at least one sample")
    if not 0 <= q <= 1:
        raise ValueError("q must be between 0 and 1")
    values = sorted(samples)
    index = (len(values) - 1) * q
    lower = int(math.floor(index))
    upper = int(math.ceil(index))
    if lower == upper:
        return values[lower]
    return values[lower] + (values[upper] - values[lower]) * (index - lower)


def screen_decision(candidate_rows, champion_rates, tasks, draws=20000, seed=0) -> dict:
    samples = null_delta_samples(champion_rates, tasks, runs=1, draws=draws, seed=seed)
    observed = delta(candidate_rows, champion_rates, tasks)
    threshold = quantile(samples, 0.70)
    return {"pass": observed >= threshold, "delta": observed, "threshold": threshold}


def stage_b_decision(candidate_rows, full_rows, champion_rates, screen_tasks, full_tasks,
                     draws=20000, seed=0) -> dict:
    samples_b = null_delta_samples(champion_rates, screen_tasks, runs=2, draws=draws, seed=seed)
    observed_b = delta(candidate_rows, champion_rates, screen_tasks)
    p = p_value(observed_b, samples_b)
    samples_full = null_delta_samples(champion_rates, full_tasks, runs=1, draws=draws, seed=seed + 1)
    observed_full = delta(full_rows, champion_rates, full_tasks)
    no_regression_floor = quantile(samples_full, 0.10)
    no_regression = observed_full >= no_regression_floor
    return {"keep": p <= 0.05 and no_regression, "delta": observed_b, "p_value": p,
            "full_delta": observed_full, "full_no_regression_floor": no_regression_floor,
            "full_no_regression": no_regression}


def select_screen_set(champion_rates, sentinels=3, hard=4, seed=0) -> dict:
    contested = sorted(task for task, (s, n) in champion_rates.items() if 0 < s < n)
    easy = [task for task, (s, n) in champion_rates.items() if n and s == n]
    difficult = [task for task, (s, n) in champion_rates.items() if n and s == 0]
    rng = random.Random(seed)
    rng.shuffle(easy)
    rng.shuffle(difficult)
    chosen_sentinels = sorted(easy[:sentinels])
    chosen_hard = sorted(difficult[:hard])
    screen = sorted(set(contested + chosen_sentinels + chosen_hard))
    return {"contested": contested, "sentinels": chosen_sentinels, "hard": chosen_hard, "S": screen}


def _logit(p):
    p = min(max(p, 1e-12), 1 - 1e-12)
    return math.log(p / (1 - p))


def _logistic(value):
    if value >= 0:
        z = math.exp(-value)
        return 1.0 / (1.0 + z)
    z = math.exp(value)
    return z / (1.0 + z)


def simulate_rule(champion_rates, S, effect_logit, trials, seed) -> float:
    """Estimate keep probability using independent stage A, stage B, and full-pool draws."""
    rng = random.Random(seed)
    all_tasks = sorted(champion_rates)
    null_a = null_delta_samples(champion_rates, S, 1, draws=6000, seed=seed + 10)
    null_b = null_delta_samples(champion_rates, S, 2, draws=6000, seed=seed + 20)
    null_full = null_delta_samples(champion_rates, all_tasks, 1, draws=6000, seed=seed + 30)
    a_bar = quantile(null_a, 0.70)
    full_floor = quantile(null_full, 0.10)
    kept = 0
    for _ in range(trials):
        true_p = {}
        for task in all_tasks:
            s, n = champion_rates[task]
            point = s / n if n else 0.0
            p = rng.betavariate(s + 0.5, n - s + 0.5)
            true_p[task] = _logistic(_logit(p) + effect_logit)
        a_delta = sum(int(rng.random() < true_p[t]) - champion_rates[t][0] / float(champion_rates[t][1])
                      for t in S)
        if a_delta < a_bar:
            continue
        b_rows = []
        for rep in range(2):
            b_rows.extend({"opaque_id": t, "rep": rep, "grade": {"resolved": rng.random() < true_p[t]}}
                           for t in S)
        b_delta = delta(b_rows, champion_rates, S)
        if p_value(b_delta, null_b) > 0.05:
            continue
        full_delta = sum(int(rng.random() < true_p[t]) - champion_rates[t][0] / float(champion_rates[t][1])
                         for t in all_tasks)
        if full_delta >= full_floor:
            kept += 1
    return kept / float(trials) if trials else 0.0


def _noise_floor(args) -> int:
    rows = load_rows(args.run_dir, args.label)
    rates = per_task_rates(rows, args.label)
    selected = select_screen_set(rates, sentinels=args.sentinels, hard=args.hard, seed=args.seed)
    screen = selected["S"]
    null_a = null_delta_samples(rates, screen, 1, seed=args.seed)
    null_b = null_delta_samples(rates, screen, 2, seed=args.seed)
    full = null_delta_samples(rates, sorted(rates), 1, seed=args.seed + 1)
    result = {"per_task_rates": {task: {"successes": s, "n": n} for task, (s, n) in sorted(rates.items())},
              "screen_set": selected,
              "thresholds": {"stage_a_q70": quantile(null_a, 0.70), "stage_b_p05": quantile(null_b, 0.95),
                             "full44_q10": quantile(full, 0.10)},
              "simulate_rule": {str(effect): simulate_rule(rates, screen, effect, args.trials, args.seed)
                                for effect in (0, 0.5, 1.0, 1.5)}}
    args.out.write_text(json.dumps(result, indent=1, sort_keys=True) + "\n")
    print(json.dumps(result, sort_keys=True))
    return 0


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    noise = sub.add_parser("noise-floor")
    noise.add_argument("--run-dir", type=Path, required=True)
    noise.add_argument("--label", required=True)
    noise.add_argument("--out", type=Path, required=True)
    noise.add_argument("--seed", type=int, default=0)
    noise.add_argument("--trials", type=int, default=2000)
    noise.add_argument("--sentinels", type=int, default=3)
    noise.add_argument("--hard", type=int, default=4)
    args = parser.parse_args(argv)
    return _noise_floor(args)


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
