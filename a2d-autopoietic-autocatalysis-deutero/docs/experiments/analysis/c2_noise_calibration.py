#!/usr/bin/env python3
"""Cycle-2 noise calibration for sizing phase B on gym v3 (plan 2026-10-21-gym-v3, "Phase B on gym v3").

Reads only dev-set runs (tasks/train-all) that cycle 2 already scored. No model calls, no holdout.
Output is aggregates only: no task id is ever printed with an outcome.

Sources, each run de-duplicated by its ledger's sha256 (re-registered experiments re-used runs):
- runs/c2-E0*/{screen,rest,confirm}: X:inc = the experiment's incumbent, X:cand = its candidate;
- runs/c2-aa-*/run: D, and X:a and X:b (both the A/A'd harness);
- runs/c2-pc/round<r>/scores.json: X:inc (E007), X:inf and X:uninf (phase-B proposals).

Reports, per harness with replicates:
- `repeat`: the same harness's outcome variation. `discordance` is the A/A discordance, the
  unbiased per-task 2·s(k−s)/(k(k−1)) averaged over tasks with k ≥ 2 draws. `binomial_ratio`
  is discordance / (2π(1−π)): below 1 means outcomes concentrate per task (always-pass or
  always-fail), which gym v3's sizing needs.
- `aa_pairs`: for every pair of full replicates of one harness, the cluster-bootstrap SE over the
  iid SE of the paired all-pass difference (the A/A cluster inflation).

Per proposal comparison (candidate vs incumbent, same run, same tasks):
- delta all-pass, discordance, iid and cluster SE, and their ratio (the effect cluster inflation);
- the between-proposal SD of true effects, by method of moments (var(delta) − mean(SE²)).

usage: python3 -I -B docs/experiments/analysis/c2_noise_calibration.py [--out runs/c2-noise-calibration.json]
"""
import argparse
import hashlib
import importlib.util
import itertools
import json
import math
import pathlib
import statistics
import sys

ROOT = pathlib.Path(__file__).resolve().parents[3]


def load_eval():
    spec = importlib.util.spec_from_file_location("calib_eval", ROOT / "nano/eval.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def file_sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def iid_se(diffs):
    n = len(diffs)
    mu = sum(diffs) / n
    return math.sqrt(sum((d - mu) ** 2 for d in diffs) / (n - 1) / n) if n > 1 else 0.0


def repeat_stats(draws):
    """draws: {task: [0/1, ...]} for one harness. Aggregates only."""
    multi = {t: v for t, v in draws.items() if len(v) >= 2}
    if not multi:
        return None
    pairs = [2 * sum(v) * (len(v) - sum(v)) / (len(v) * (len(v) - 1)) for v in multi.values()]
    pi = sum(sum(v) / len(v) for v in multi.values()) / len(multi)
    d = sum(pairs) / len(pairs)
    deep = [v for v in multi.values() if len(v) >= 4]
    shares = None
    if deep:
        always = sum(1 for v in deep if all(v))
        never = sum(1 for v in deep if not any(v))
        mixed = [sum(v) / len(v) for v in deep if 0 < sum(v) < len(v)]
        bins = [0, 0, 0]
        for p in mixed:
            bins[0 if p < 1 / 3 else 1 if p <= 2 / 3 else 2] += 1
        shares = {"tasks_with_4plus": len(deep), "always_pass": always / len(deep),
                  "always_fail": never / len(deep), "mixed": len(mixed) / len(deep),
                  "mixed_p_low_mid_high": [b / len(mixed) if mixed else None for b in bins]}
    return {"tasks": len(multi), "draws_per_task_median": statistics.median(len(v) for v in multi.values()),
            "pass_rate": pi, "discordance": d,
            "binomial_ratio": d / (2 * pi * (1 - pi)) if 0 < pi < 1 else None, "deep": shares}


def comparison(ev, rows):
    """rows: [(family, delta_frac, delta_allpass)] paired on one run's tasks."""
    diffs = [r[2] for r in rows]
    iid = iid_se(diffs)
    clu = ev.cluster_se(rows, 2)
    return {"n": len(rows), "families": len({r[0] for r in rows}), "delta_allpass": sum(diffs) / len(diffs),
            "discordance": sum(1 for d in diffs if d) / len(diffs), "se_iid": iid, "se_cluster": clu,
            "inflation": clu / iid if iid else None}


def collect(ev):
    """Per harness {task: [outcomes]}, full-replicate tables, and proposal comparisons.

    Each ledger is counted once (re-registered experiments re-used runs). A full replicate
    (screen+rest, or confirm, or an A/A arm) carries its (ledger, arm) identities, so that two
    replicates built on the same physical run are never paired."""
    draws, family_of, seen = {}, {}, set()
    full_reps = {}
    comparisons = []

    def add_draws(harness, table):
        for task, (ok, fam) in table.items():
            draws.setdefault(harness, {}).setdefault(task, []).append(ok)
            family_of[task] = fam

    def add_rep(harness, table, ids):
        full_reps.setdefault(harness, []).append((frozenset(ids), {t: ok for t, (ok, _) in table.items()}))

    def arm_table(s, arm):
        return {t: (ok, fam) for (a, t), (_, ok, fam) in s.items() if a == arm}

    for exp in sorted(ROOT.glob("runs/c2-E0*")):
        manifest = exp / "manifest.json"
        if not manifest.is_file():
            continue
        m = json.loads(manifest.read_text())
        names = {"X:inc": m.get("incumbent_sha", "")[:12], "X:cand": m.get("candidate_sha", "")[:12]}
        stages = {}
        for stage, suite in (("screen", exp / "suites/S"), ("rest", exp / "suites/R"), ("confirm", ev.DEV)):
            run = exp / stage
            if not (run / "ledger.jsonl").is_file() or not suite.is_dir():
                continue
            key = file_sha(run / "ledger.jsonl")
            s = ev.scores(run, suite)
            stages[stage] = (key, s)
            if key in seen:
                continue
            seen.add(key)
            for arm, harness in names.items():
                add_draws(harness, arm_table(s, arm))
            comparisons.append({"source": f"{exp.name}/{stage}",
                                **comparison(ev, ev.paired([(run, suite)], "X:cand", "X:inc"))})
        groups = []
        if "screen" in stages and "rest" in stages:
            groups.append(({stages["screen"][0], stages["rest"][0]}, {**stages["screen"][1], **stages["rest"][1]}))
        if "confirm" in stages:
            groups.append(({stages["confirm"][0]}, stages["confirm"][1]))
        for keys, s in groups:
            for arm, harness in names.items():
                add_rep(harness, arm_table(s, arm), {(k, arm) for k in keys})

    for aa in sorted(ROOT.glob("runs/c2-aa-*")):
        run = aa / "run"
        if not (run / "ledger.jsonl").is_file():
            continue
        key = file_sha(run / "ledger.jsonl")
        if key in seen:
            continue
        seen.add(key)
        s = ev.scores(run, ev.DEV)
        harness = aa.name.split("-")[-1][:12]
        for arm, name in (("D", "D"), ("X:a", harness), ("X:b", harness)):
            add_draws(name, arm_table(s, arm))
            add_rep(name, arm_table(s, arm), {(key, arm)})
        comparisons.append({"source": f"{aa.name} X:b vs X:a (A/A)", "aa": True,
                            **comparison(ev, ev.paired([(run, ev.DEV)], "X:b", "X:a"))})

    frozen = ROOT / "runs/c2-pc/frozen.json"
    inc = (json.loads(frozen.read_text()).get("incumbent_sha") or "E007")[:12] if frozen.is_file() else "E007"
    for scores_path in sorted(ROOT.glob("runs/c2-pc/round*/scores.json")):
        rnd = scores_path.parent
        data = json.loads(scores_path.read_text())
        table = {(s["arm"], s["task"]): (s["fraction"], s["all_passed"], s["family"]) for s in data["scores"]}
        names = {"X:inc": inc}
        for arm, label in (("X:inf", "informed"), ("X:uninf", "uninformed")):
            cj = rnd / label / "candidate.json"
            names[arm] = json.loads(cj.read_text())["candidate_sha"][:12] if cj.is_file() else f"{rnd.name}-{label}"
        for arm, harness in names.items():
            add_draws(harness, {t: (ok, fam) for (a, t), (_, ok, fam) in table.items() if a == arm})

        def pair(x, y):
            return [(fam, fx - table[y, t][0], ax - table[y, t][1])
                    for (a, t), (fx, ax, fam) in sorted(table.items()) if a == x]
        for x, y in (("X:inf", "X:inc"), ("X:uninf", "X:inc"), ("X:inf", "X:uninf")):
            comparisons.append({"source": f"c2-pc/{rnd.name} {x} vs {y}", "phase_b": True,
                                **comparison(ev, pair(x, y))})
    return draws, family_of, full_reps, comparisons


def aa_pairs(ev, family_of, full_reps):
    out = []
    for harness, reps in full_reps.items():
        for (ia, a), (ib, b) in itertools.combinations(reps, 2):
            common = sorted(set(a) & set(b))
            if len(common) < 100 or ia & ib:      # partial, or the same (ledger, arm): identical draws
                continue
            rows = [(family_of[t], 0.0, b[t] - a[t]) for t in common]
            c = comparison(ev, rows)
            out.append({"harness": harness, "n": c["n"], "discordance": c["discordance"],
                        "delta_allpass": c["delta_allpass"], "inflation": c["inflation"]})
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default="runs/c2-noise-calibration.json")
    args = ap.parse_args()
    ev = load_eval()
    draws, family_of, full_reps, comparisons = collect(ev)
    repeat = {h: repeat_stats(d) for h, d in sorted(draws.items())}
    pairs = aa_pairs(ev, family_of, full_reps)
    search = [c for c in comparisons if not c.get("aa") and not c.get("phase_b")
              and c["n"] >= 60 and "vs X:uninf" not in c["source"]]
    proposals = search + [c for c in comparisons if c.get("phase_b") and "vs X:inc" in c["source"]]
    deltas = [c["delta_allpass"] for c in proposals]
    noise = sum(c["se_cluster"] ** 2 for c in proposals) / len(proposals)
    var = statistics.variance(deltas) if len(deltas) > 1 else 0.0
    by_harness_inflation = [p["inflation"] for p in pairs if p["inflation"]]
    result = {
        "method": "cycle-2 dev runs only; aggregates; see the module docstring",
        "repeat": repeat,
        "aa_pairs": {"count": len(pairs), "inflation_median": statistics.median(by_harness_inflation) if by_harness_inflation else None,
                     "by_harness": {h: {"pairs": sum(1 for p in pairs if p["harness"] == h),
                                        "discordance_mean": statistics.mean(p["discordance"] for p in pairs if p["harness"] == h)}
                                    for h in sorted({p["harness"] for p in pairs})}},
        "proposals": {"count": len(proposals),
                      "inflation_median": statistics.median(c["inflation"] for c in proposals if c["inflation"]),
                      "discordance_median": statistics.median(c["discordance"] for c in proposals),
                      "delta_allpass": sorted(round(d, 4) for d in deltas),
                      "var_observed": var, "mean_noise_var": noise,
                      "true_effect_sd_mom": math.sqrt(max(0.0, var - noise))},
        "phase_b_inf_vs_uninf": [{k: c[k] for k in ("source", "n", "discordance", "delta_allpass", "inflation")}
                                 for c in comparisons if "vs X:uninf" in c["source"]],
        "comparisons": [{k: (round(v, 4) if isinstance(v, float) else v) for k, v in c.items()} for c in comparisons],
    }
    out = ROOT / args.out
    out.write_text(json.dumps(result, indent=1) + "\n")
    print(json.dumps({k: result[k] for k in ("repeat", "aa_pairs", "proposals", "phase_b_inf_vs_uninf")}, indent=1))
    return 0


if __name__ == "__main__":
    sys.exit(main())
