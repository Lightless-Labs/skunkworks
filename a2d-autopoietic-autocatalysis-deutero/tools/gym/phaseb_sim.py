"""Phase B on gym v3: sizing by simulation (plan docs/plans/2026-10-21-gym-v3.md, "Phase B on gym v3").

The method is fixed here, before gym v3 exists. The inputs are measured on gym v3 later (the
acceptance replicate and the first X:inc arm give the A/A) and frozen in a LOG entry before
round 1. Defaults are cycle-2 placeholders from docs/experiments/analysis/c2_noise_calibration.py.

Model
- Tasks come in families of 4. Each task has an incumbent pass probability p drawn from a mixture:
  always-pass, always-fail, or mixed with p ~ Beta(s, s). The shares are solved so that the mean is
  `pi` and the A/A discordance E[2p(1-p)] is `d`. Cycle 2 measured d at 0.41-0.57 of the binomial
  2pi(1-pi): outcomes concentrate per task.
- A proposal with mean effect delta moves each task's p: gains are proportional to (1 - p), losses
  to p, normalised so the mean shift is delta. An optional family-level spread `tau` adds
  heterogeneity; cycle 2 measured no family clustering (cluster/iid SE about 0.96), so it defaults to 0.
- Per round, each arm draws once per task (all-pass is binary per task at r = 1).
- Proposal quality: delta ~ N(mu_arm, sd_delta). The informed arm has mu + gap. Cycle 2's
  method-of-moments SD of true effects is about 3.5 pp.

Screens (each feeds the same continuation; a keep needs the screen, then the full stage over the
round's tasks plus fresh draws on the remaining families, Δ > max(thr, 1.645·SE), then a fresh
confirmation replicate on all tasks, Δ > thr/2; thr = the A/A 95th percentile of the full-set mean):
- `fixed`: round net all-pass ≥ k tasks (protocol v2: k = 2);
- `aa`: round mean Δ ≥ the A/A 95th percentile for the round's size, 1.645·sqrt(d/n);
- `half`: round mean Δ ≥ thr/2 (eval v2.3's screen rule applied to the round).

Arm comparison over R rounds (the self-improvement endpoint), one-sided at alpha = 0.05:
- `cluster`: protocol v2's pooled informed-minus-uninformed difference with a (round, family)
  cluster SE. Tasks in a round share one pair of proposals, so the clusters are not independent
  when sd_delta > 0. The simulation shows how anti-conservative that is.
- `round`: a t-test on the R round-level differences. The proposal pair is the unit.
- `keeps`: a one-sided Fisher exact test on confirmed keeps per arm.

Stdlib only. usage: python3 -I -B tools/gym/phaseb_sim.py [--quick] [--out FILE]
"""
import argparse
import json
import math
import random
import statistics

Z95 = 1.6448536269514722
# One-sided 95% t quantiles by degrees of freedom (1..40); larger df uses the normal.
T95 = [None, 6.3138, 2.9200, 2.3534, 2.1318, 2.0150, 1.9432, 1.8946, 1.8595, 1.8331, 1.8125,
       1.7959, 1.7823, 1.7709, 1.7613, 1.7531, 1.7459, 1.7396, 1.7341, 1.7291, 1.7247,
       1.7207, 1.7171, 1.7139, 1.7109, 1.7081, 1.7056, 1.7033, 1.7011, 1.6991, 1.6973,
       1.6955, 1.6939, 1.6924, 1.6909, 1.6896, 1.6883, 1.6871, 1.6860, 1.6849, 1.6839]

# Cycle-2 placeholders (c2_noise_calibration.py); replaced by gym-v3 measurements before round 1.
DEFAULTS = {"pi": 0.5, "d": 0.25, "shape": 1.5, "tau": 0.0, "sd_delta": 0.035, "mu": 0.0,
            "n_dev": 240, "calls_per_task": 2.25}


def t95(df):
    return T95[df] if 1 <= df < len(T95) else Z95


def mixture(pi, d, shape=1.5):
    """(always_pass, always_fail, mixed) shares with mean pi and E[2p(1-p)] = d."""
    per_mixed = shape / (2 * shape + 1)          # E[2p(1-p)] under Beta(shape, shape)
    c = d / per_mixed
    a = pi - c / 2
    b = 1 - a - c
    if not (0 < pi < 1) or c > 1 or a < -1e-12 or b < -1e-12:
        raise ValueError(f"infeasible: d = {d} at pi = {pi} (shape {shape})")
    return max(a, 0.0), max(b, 0.0), c


def draw_gym(n_families, pi, d, shape, rng):
    a, b, _ = mixture(pi, d, shape)
    ps, fams = [], []
    for f in range(n_families):
        for _ in range(4):
            u = rng.random()
            ps.append(1.0 if u < a else 0.0 if u < a + b else rng.betavariate(shape, shape))
            fams.append(f)
    return ps, fams


def shifted(ps, fams, delta, tau, pi, rng):
    """Candidate pass probabilities for a proposal with mean effect delta."""
    effect = {f: delta + tau * rng.gauss(0, 1) for f in set(fams)} if tau else None
    out = []
    for p, f in zip(ps, fams):
        e = effect[f] if effect else delta
        q = p + e * (1 - p) / (1 - pi) if e >= 0 else p + e * p / pi
        out.append(min(1.0, max(0.0, q)))
    return out


def bern(ps, rng):
    return [1 if rng.random() < p else 0 for p in ps]


def aa_threshold(d, n):
    """The A/A 95th percentile of a mean all-pass difference on n tasks (normal approximation)."""
    return Z95 * math.sqrt(d / n)


def iid_se(diffs):
    n = len(diffs)
    mu = sum(diffs) / n
    return math.sqrt(sum((x - mu) ** 2 for x in diffs) / max(n - 1, 1) / n)


def screen_pass(design, net, n, d, n_dev, k=2):
    if design == "fixed":
        return net >= k
    if design == "aa":
        return net / n >= aa_threshold(d, n) - 1e-12
    if design == "half":
        return net / n >= aa_threshold(d, n_dev) / 2 - 1e-12
    raise ValueError(design)


def proposal_trial(gym, round_fams, delta, p, design, k, rng):
    """One proposal: (screened, kept, luna_calls). The incumbent and candidate draw fresh each stage."""
    ps, fams = gym
    cand = shifted(ps, fams, delta, p["tau"], p["pi"], rng)
    in_round = [i for i, f in enumerate(fams) if f in round_fams]
    rest = [i for i, f in enumerate(fams) if f not in round_fams]
    cost_task = p["calls_per_task"]
    xi, xc = bern([ps[i] for i in in_round], rng), bern([cand[i] for i in in_round], rng)
    net = sum(xc) - sum(xi)
    calls = 1.5 * len(in_round) * cost_task        # its own arm plus half the shared incumbent arm
    n_dev = len(ps)
    if not screen_pass(design, net, len(in_round), p["d"], n_dev, k):
        return False, False, calls
    thr = aa_threshold(p["d"], n_dev)
    ri, rc = bern([ps[i] for i in rest], rng), bern([cand[i] for i in rest], rng)
    full = [c - i for c, i in zip(xc + rc, xi + ri)]
    calls += 2 * len(rest) * cost_task
    if not sum(full) / n_dev > max(thr, Z95 * iid_se(full)):
        return True, False, calls
    conf = [c - i for c, i in zip(bern(cand, rng), bern(ps, rng))]
    calls += 2 * n_dev * cost_task
    return True, sum(conf) / n_dev > thr / 2, calls


def screen_table(p, designs, deltas, n_round_fams, sims, seed, k=2):
    """Per (design, round size, delta): P(screen), P(keep), mean Luna calls per proposal."""
    rng = random.Random(seed)
    n_fams = p["n_dev"] // 4
    out = []
    for nf in n_round_fams:
        for delta in deltas:
            stats = {dsg: [0, 0, 0.0] for dsg in designs}
            for _ in range(sims):
                gym = draw_gym(n_fams, p["pi"], p["d"], p["shape"], rng)
                round_fams = set(rng.sample(range(n_fams), nf))
                for dsg in designs:
                    s, kept, calls = proposal_trial(gym, round_fams, delta, p, dsg, k, rng)
                    stats[dsg][0] += s
                    stats[dsg][1] += kept
                    stats[dsg][2] += calls
            for dsg, (s, kept, calls) in stats.items():
                out.append({"design": dsg, "round_tasks": 4 * nf, "delta": delta, "p_screen": s / sims,
                            "p_keep": kept / sims, "luna_per_proposal": calls / sims})
    return out


def fisher_one_sided(a, n1, b, n2):
    """P(X >= a) for X ~ Hypergeometric: keeps a of n1 in arm 1 given a + b total keeps."""
    k = a + b
    total = math.comb(n1 + n2, k)
    return sum(math.comb(n1, x) * math.comb(n2, k - x) for x in range(a, min(n1, k) + 1)) / total


def cluster_test(rounds):
    """Protocol v2: pooled task differences with a (round, family)-cluster SE (CR0), one-sided."""
    diffs = [x for r in rounds for (_, x) in r]
    n = len(diffs)
    mu = sum(diffs) / n
    by = {}
    for ri, r in enumerate(rounds):
        for fam, x in r:
            by.setdefault((ri, fam), []).append(x - mu)
    se = math.sqrt(sum(sum(v) ** 2 for v in by.values())) / n
    return se > 0 and mu - Z95 * se > 0


def round_test(rounds):
    means = [sum(x for _, x in r) / len(r) for r in rounds]
    R = len(means)
    mu = sum(means) / R
    sd = statistics.stdev(means) if R > 1 else 0.0
    return sd > 0 and mu / (sd / math.sqrt(R)) > t95(R - 1)


def keep_probability(table, design, round_tasks):
    """Linear interpolation of P(keep | delta) from a screen table."""
    pts = sorted((r["delta"], r["p_keep"]) for r in table if r["design"] == design and r["round_tasks"] == round_tasks)

    def f(delta):
        if delta <= pts[0][0]:
            return pts[0][1]
        for (x0, y0), (x1, y1) in zip(pts, pts[1:]):
            if delta <= x1:
                return y0 + (y1 - y0) * (delta - x0) / (x1 - x0)
        return pts[-1][1]
    return f


def comparison_table(p, gaps, n_round_fams, rounds_list, sims, seed, keep_curves=None):
    """Per (round size, R, gap): rejection rates of the cluster, round and keep-count tests."""
    rng = random.Random(seed)
    n_fams = p["n_dev"] // 4
    out = []
    for nf in n_round_fams:
        for R in rounds_list:
            for gap in gaps:
                rej = {"cluster": 0, "round": 0, "keeps": 0}
                for _ in range(sims):
                    gym = draw_gym(n_fams, p["pi"], p["d"], p["shape"], rng)
                    ps, fams = gym
                    rounds, keeps = [], [0, 0]
                    for _ in range(R):
                        round_fams = set(rng.sample(range(n_fams), nf))
                        idx = [i for i, f in enumerate(fams) if f in round_fams]
                        sub_p, sub_f = [ps[i] for i in idx], [fams[i] for i in idx]
                        d_inf = rng.gauss(p["mu"] + gap, p["sd_delta"])
                        d_un = rng.gauss(p["mu"], p["sd_delta"])
                        x_inf = bern(shifted(sub_p, sub_f, d_inf, p["tau"], p["pi"], rng), rng)
                        x_un = bern(shifted(sub_p, sub_f, d_un, p["tau"], p["pi"], rng), rng)
                        rounds.append(list(zip(sub_f, [a - b for a, b in zip(x_inf, x_un)])))
                        if keep_curves:
                            f = keep_curves[4 * nf]
                            keeps[0] += rng.random() < f(d_inf)
                            keeps[1] += rng.random() < f(d_un)
                    rej["cluster"] += cluster_test(rounds)
                    rej["round"] += round_test(rounds)
                    if keep_curves:
                        rej["keeps"] += fisher_one_sided(keeps[0], R, keeps[1], R) <= 0.05
                row = {"round_tasks": 4 * nf, "R": R, "gap": gap,
                       "reject_cluster": rej["cluster"] / sims, "reject_round": rej["round"] / sims}
                if keep_curves:
                    row["reject_keeps"] = rej["keeps"] / sims
                out.append(row)
    return out


def main():
    ap = argparse.ArgumentParser()
    for key, val in DEFAULTS.items():
        ap.add_argument("--" + key.replace("_", "-"), type=type(val), default=val)
    ap.add_argument("--k", type=int, default=2, help="the fixed screen's net-task bar (protocol v2: 2)")
    ap.add_argument("--quick", action="store_true", help="few simulations, for a smoke run")
    ap.add_argument("--seed", type=int, default=20261022)
    ap.add_argument("--out")
    args = ap.parse_args()
    p = {key: getattr(args, key) for key in DEFAULTS}
    sims_screen, sims_cmp = (200, 200) if args.quick else (3000, 1500)
    round_fams = [9, 15, 30]
    designs = ["fixed", "aa", "half"]
    deltas = [0.0, 0.025, 0.05, 0.075, 0.10, 0.15]
    screens = screen_table(p, designs, deltas, round_fams, sims_screen, args.seed, args.k)
    curves = {4 * nf: keep_probability(screens, "half", 4 * nf) for nf in round_fams}
    comps = comparison_table(p, [0.0, 0.025, 0.05, 0.075], round_fams, [4, 6, 10, 15, 20], sims_cmp,
                             args.seed + 1, curves)
    result = {"method": "tools/gym/phaseb_sim.py (module docstring)", "params": p, "k": args.k,
              "sims": {"screen": sims_screen, "comparison": sims_cmp}, "mixture": mixture(p["pi"], p["d"], p["shape"]),
              "screens": screens, "comparisons": comps}
    text = json.dumps(result, indent=1)
    if args.out:
        with open(args.out, "w") as fh:
            fh.write(text + "\n")
    print(text)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
