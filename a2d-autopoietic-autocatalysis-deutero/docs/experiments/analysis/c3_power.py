"""Blind power statement for confirmation-3 (plan 2026-10-18-confirmation-3.md, §0.6 step 1).

Reads ONLY dev summaries under runs/c2-*/ (paired comparisons between harness
variants on tasks/train-all). It never reads holdout, look or confirmation data,
and prints only discordance rates, the calibrated task-level deviation SE, and
P(SUPPORTED / INCONCLUSIVE / RULED OUT) at fixed hypothetical lifts.

Model: docs/experiments/analysis/c2_joint_power2.py (the confirmation-2 power
model; 36 families x 4 tasks; whole-family percentile bootstrap; SUPPORTED iff
lift >= 0.10 and CI low > 0 vs both D and B; RULED OUT on X vs D).
Calibration: choose SE so the model's paired discordance between two arms with
the observed dev lift matches the pooled observed dev discordance.

usage: python3 -I docs/experiments/analysis/c3_power.py [sims] [r]
"""
import glob, json, math, random, subprocess, sys
from statistics import NormalDist

HERE = "docs/experiments/analysis"
SIMS = int(sys.argv[1]) if len(sys.argv) > 1 else 400
R = int(sys.argv[2]) if len(sys.argv) > 2 else 2
PHI = NormalDist().cdf
A, B0, SF = 2.4, 1.4, 0.6          # c2_joint_power2.py constants
LIFTS = [0.0, 0.071, 0.089, 0.10, 0.15]
B_MINUS_D = -0.036                  # confirmation-2 registration's B assumption (fresh confirm stage)


def dev_discordance():
    """Pooled discordance over every complete dev comparison in runs/c2-*/**/summary.json."""
    rows, n_tot, disc_tot, lift_w = [], 0, 0, 0.0
    for path in sorted(glob.glob("runs/c2-*/*/summary.json")):
        if any(w in path for w in ("look", "confirm2", "holdout")):
            continue  # never read look or confirmation summaries, even if one were copied into the repo
        try:
            s = json.load(open(path))
        except (OSError, ValueError):
            continue
        if not s.get("complete") or s.get("aborted"):
            continue
        for c in s.get("comparisons", []):
            n = c.get("n") or 0
            d = (c.get("discordant_treatment_only") or 0) + (c.get("discordant_comparator_only") or 0)
            if n >= 40:
                rows.append((path, n, d / n, c.get("lift", 0.0)))
                n_tot += n; disc_tot += d; lift_w += abs(c.get("lift", 0.0)) * n
    return rows, (disc_tot / n_tot if n_tot else float("nan")), (lift_w / n_tot if n_tot else 0.0)


def model_discordance(se, lift, sims=60000, seed=7):
    """Paired discordance in the c2 model between D (e = 0) and an arm with deviation SE and the given lift."""
    rng = random.Random(seed)
    # shift for the treated arm (same bisection as c2_joint_power2.shift_for, Monte Carlo marginal)
    def marg(shift, extra):
        sd = math.sqrt(A * A * (1 + SF * SF) + extra * extra)
        xs = [NormalDist(0, sd).inv_cdf((i + 0.5) / 2000) for i in range(2000)]
        return sum(PHI(x + B0 + shift) for x in xs) / len(xs)
    base, lo, hi = marg(0, 0), -4.0, 4.0
    for _ in range(40):
        mid = (lo + hi) / 2
        lo, hi = (mid, hi) if marg(mid, se) - base < lift else (lo, mid)
    shift = (lo + hi) / 2
    disc = 0
    for _ in range(sims):
        z = rng.gauss(rng.gauss(0, SF), 1)
        pd = PHI(A * z + B0)
        pt = PHI(A * z + rng.gauss(0, se) + B0 + shift)
        disc += (rng.random() < pd) != (rng.random() < pt)
    return disc / sims


def calibrate(target, lift):
    best = None
    for se in [x / 10 for x in range(5, 61, 5)]:
        m = model_discordance(se, lift)
        if best is None or abs(m - target) < abs(best[1] - target):
            best = (se, m)
    return best


rows, pooled, mean_abs_lift = dev_discordance()
print(f"dev comparisons used: {len(rows)}")
for path, n, d, _ in rows:
    print(f"  {path}: n={n} discordance={d:.3f}")
print(f"pooled dev discordance: {pooled:.3f} (mean |lift| {mean_abs_lift:.3f})")
se, m = calibrate(pooled, mean_abs_lift)
print(f"calibrated task-level deviation SE = {se} (model discordance {m:.3f}); confirmation-2 used SE = 3.2")
for lift in LIFTS:
    db = 0.0 if lift == 0.0 else B_MINUS_D
    out = subprocess.run([sys.executable, "-I", f"{HERE}/c2_joint_power2.py", str(SIMS), str(R),
                          str(lift), "0", str(db), str(se)], capture_output=True, text=True)
    print(out.stdout.strip() or out.stderr.strip())
