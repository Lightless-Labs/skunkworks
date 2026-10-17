"""Joint power of the cycle-2 Confirmation-2 rule (X vs D and X vs B; 36 families x 4 tasks, r replicates).

Completion needs C(H*) SUPPORTED against each of C(H0), D and B
(intersection-union), each by the implemented classifier: whole-family
percentile bootstrap of the paired lift (crates/a2d-lift/src/stats.rs);
SUPPORTED if ci_low > 0 and lift >= 0.10.

Model (as c2_power.py, calibrated to P2/P3 data): task latent z ~ N(mu_f, 1),
mu_f ~ N(0, SF); arm j passes a replicate with p = Phi(A z + e_jt + B0 + s_j),
where e_jt ~ N(0, SE) is a task-level arm-specific deviation (fixed across
replicates) and s_j is the arm's shift chosen so its marginal rate is
base + delta_j. D has e = 0 (reference).

usage: python3 -I c2_joint_power2.py <sims> <r> <dX> <unused> <dB> [SE]
  deltas are true marginal lifts over D (e.g. 0.15 0.00 -0.02)
"""
import math, random, sys
from statistics import NormalDist

PHI = NormalDist().cdf
SIMS, R = int(sys.argv[1]), int(sys.argv[2])
D_STAR, D_H0, D_B = (float(x) for x in sys.argv[3:6])
SE = float(sys.argv[6]) if len(sys.argv) > 6 else 3.2
BOOT, FAMILIES, PER = 2000, 36, 4
A, B0, SF = 2.4, 1.4, 0.6


def marginal(shift, extra):
    sd = math.sqrt(A * A * (1 + SF * SF) + extra * extra)
    xs = [NormalDist(0, sd).inv_cdf((i + 0.5) / 2000) for i in range(2000)]
    return sum(PHI(x + B0 + shift) for x in xs) / len(xs)


def shift_for(delta, extra):
    base, lo, hi = marginal(0, 0), -4.0, 4.0
    for _ in range(50):
        mid = (lo + hi) / 2
        lo, hi = (mid, hi) if marginal(mid, extra) - base < delta else (lo, mid)
    return (lo + hi) / 2


def classify(clusters, rng):
    n = sum(c[0] for c in clusters)
    lift = (sum(c[1] for c in clusters) - sum(c[2] for c in clusters)) / n
    samples = []
    for _ in range(BOOT):
        s = sx = sy = 0
        for _ in range(FAMILIES):
            c = clusters[rng.randrange(FAMILIES)]
            s += c[0]; sx += c[1]; sy += c[2]
        samples.append((sx - sy) / s)
    samples.sort()
    last = BOOT - 1
    lo, hi = samples[math.floor(0.025 * last)], samples[math.ceil(0.975 * last)]
    return ("SUPPORTED" if lo > 0 and lift >= 0.10 else "RULED OUT" if hi < 0.10 else "INCONCLUSIVE")


def one(rng, shifts):
    # arms: 0 = H*, 1 = H0, 2 = B, 3 = D
    fam_counts = []
    for _ in range(FAMILIES):
        mu = rng.gauss(0, SF)
        counts = [0, 0, 0, 0]
        n = 0
        for _ in range(PER):
            z = rng.gauss(mu, 1)
            ps = [PHI(A * z + (rng.gauss(0, SE) if j < 3 else 0) + B0 + shifts[j]) for j in range(4)]
            for _ in range(R):
                n += 1
                for j in range(4):
                    counts[j] += rng.random() < ps[j]
        fam_counts.append((n, counts))
    # Overall class (PROTOCOL): SUPPORTED needs X > D and X > B both
    # SUPPORTED; RULED OUT is decided by the primary comparison (X vs D).
    vs_d = classify([(n, c[0], c[3]) for n, c in fam_counts], rng)
    vs_b = classify([(n, c[0], c[2]) for n, c in fam_counts], rng)
    if vs_d == "SUPPORTED" and vs_b == "SUPPORTED":
        return "SUPPORTED"
    return "RULED OUT" if vs_d == "RULED OUT" else "INCONCLUSIVE"


rng = random.Random(20261016)
shifts = [shift_for(D_STAR, SE), shift_for(D_H0, SE), shift_for(D_B, SE), 0.0]
from collections import Counter
classes = Counter(one(rng, shifts) for _ in range(SIMS))
print(f"r={R} SE={SE} lifts over D: X={D_STAR:+.3f} B={D_B:+.3f} -> " + ", ".join(
    f"P({k})={classes[k] / SIMS:.2f}" for k in ("SUPPORTED", "INCONCLUSIVE", "RULED OUT")) + f" ({SIMS} sims)")
