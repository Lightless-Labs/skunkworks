"""Cluster-corrected power for gym v3 (plan docs/plans/2026-10-21-gym-v3.md, "Targets").

Paired all-pass difference between two arms on n tasks, r replicates each:
  SE = sqrt(d / (n * r)) * inflation
where d is the discordance (share of task pairs whose outcomes differ) and inflation is the
family-cluster correction observed in cycle 2 (1.25). Power is one-sided at alpha = 0.05 under
the normal approximation: power = Phi(effect / SE - 1.645).

Reproduces the plan's table (n = 240, d = 0.3): r = 1 -> SE 4.4 pp, power 0.30 / 0.52 / 0.73 for
+5 / +7.5 / +10 pp; r = 2 -> SE 3.1 pp, 0.48 / 0.77 / 0.94. Dev-holdout (100 tasks, r = 2):
SE 4.8 pp. Stdlib only.

usage: python3 -I tools/gym/power.py [--n 240] [--r 1 2] [--d 0.3] [--effects 0.05 0.075 0.10]
                                     [--inflation 1.25]
"""
import argparse
import json
import math

INFLATION = 1.25        # cycle 2's observed family-cluster inflation
Z_ALPHA = 1.645         # one-sided alpha = 0.05


def phi(x):
    return 0.5 * (1.0 + math.erf(x / math.sqrt(2.0)))


def se(n, r, d, inflation=INFLATION):
    if n <= 0 or r <= 0 or not 0 < d <= 1:
        raise ValueError("n, r > 0 and 0 < d <= 1")
    return math.sqrt(d / (n * r)) * inflation


def power(effect, n, r, d, inflation=INFLATION, z_alpha=Z_ALPHA):
    return phi(effect / se(n, r, d, inflation) - z_alpha)


def pairs_for_power(effect, d, target=0.8, inflation=INFLATION, z_alpha=Z_ALPHA):
    """Independent pairs (n * r) needed for `target` power at `effect`."""
    z_beta = _inv_phi(target)
    return math.ceil(d * (inflation * (z_alpha + z_beta) / effect) ** 2)


def _inv_phi(p):
    lo, hi = -10.0, 10.0
    for _ in range(200):
        mid = (lo + hi) / 2
        if phi(mid) < p:
            lo = mid
        else:
            hi = mid
    return (lo + hi) / 2


def table(n, rs, d, effects, inflation=INFLATION):
    return [{"n": n, "r": r, "d": d, "se_pp": round(100 * se(n, r, d, inflation), 1),
             "power": [round(power(e, n, r, d, inflation), 2) for e in effects],
             "effects_pp": [round(100 * e, 1) for e in effects]} for r in rs]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--n", type=int, default=240)
    ap.add_argument("--r", type=int, nargs="+", default=[1, 2])
    ap.add_argument("--d", type=float, default=0.3)
    ap.add_argument("--effects", type=float, nargs="+", default=[0.05, 0.075, 0.10])
    ap.add_argument("--inflation", type=float, default=INFLATION)
    a = ap.parse_args()
    for row in table(a.n, a.r, a.d, a.effects, a.inflation):
        print(json.dumps(row))


if __name__ == "__main__":
    main()
