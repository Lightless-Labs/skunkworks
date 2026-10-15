"""Power of the implemented decision rule on Confirmation-2 (36 families x 4).

Mirrors crates/a2d-lift/src/stats.rs: whole-family percentile bootstrap of the
paired lift; SUPPORTED if ci_low > 0 and lift >= 0.10; RULED OUT if
ci_high < 0.10; INCONCLUSIVE otherwise (min-families guard is met at 36).

Generative model (calibrated to P2/P3 data): task latent z ~ N(mu_f, 1) with
family mean mu_f ~ N(0, s_f); D passes a replicate with p = Phi(a z + b);
the treatment with p = Phi(a z + e_t + b + g_f), g_f ~ N(g, tau) chosen so
the marginal lift is delta, where e_t ~ N(0, SE) is a task-level arm-specific
deviation calibrated so that paired discordance at zero lift is ~0.26
(E08r C vs D 18/68, E07 C vs B 40/148). a and b give D's rate ~0.68 and test-retest
agreement ~0.87 (E05/E06 replicate agreement 0.875).

usage: python3 -I c2_power.py [sims] [SE]
"""
import math, random, sys
from statistics import NormalDist

PHI = NormalDist().cdf
SIMS = int(sys.argv[1]) if len(sys.argv) > 1 else 400
BOOT = 2000
FAMILIES, PER = 36, 4
A, B, SF = 2.4, 1.4, 0.6
SE = float(sys.argv[2]) if len(sys.argv) > 2 else 3.2


def marginal(shift, extra=0.0):
    """Marginal pass rate when the linear predictor is A x + B + shift, with
    x ~ N(0, 1 + SF^2) and an extra independent N(0, extra) term."""
    sd = math.sqrt(A * A * (1 + SF * SF) + extra * extra)
    xs = [NormalDist(0, sd).inv_cdf((i + 0.5) / 2000) for i in range(2000)]
    return sum(PHI(x + B + shift) for x in xs) / len(xs)


def shift_for(delta):
    base, lo, hi = marginal(0), -3.0, 3.0
    for _ in range(50):
        mid = (lo + hi) / 2
        lo, hi = (mid, hi) if marginal(mid, SE) - base < delta else (lo, mid)
    return (lo + hi) / 2


def discordance(rng, g, n=20000):
    disc = 0
    for _ in range(n):
        z = rng.gauss(rng.gauss(0, SF), 1)
        a = rng.random() < PHI(A * z + rng.gauss(0, SE) + B + g)
        b = rng.random() < PHI(A * z + B)
        disc += a != b
    return disc / n


def one_run(rng, g, tau, reps):
    clusters = []
    for _ in range(FAMILIES):
        mu, gf = rng.gauss(0, SF), rng.gauss(g, tau)
        n = x = y = 0
        for _ in range(PER):
            z = rng.gauss(mu, 1)
            pd, pt = PHI(A * z + B), PHI(A * z + rng.gauss(0, SE) + B + gf)
            for _ in range(reps):
                n += 1
                x += rng.random() < pt
                y += rng.random() < pd
        clusters.append((n, x, y))
    N = sum(c[0] for c in clusters)
    lift = (sum(c[1] for c in clusters) - sum(c[2] for c in clusters)) / N
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
    if lo > 0 and lift >= 0.10:
        return "S"
    if hi < 0.10:
        return "R"
    return "I"


def agreement(rng, n=20000):
    agree = 0
    for _ in range(n):
        p = PHI(A * rng.gauss(rng.gauss(0, SF), 1) + B)
        a, b = rng.random() < p, rng.random() < p
        agree += a == b
    return agree / n


rng = random.Random(20261015)
print(f"D rate {marginal(0):.3f}; test-retest agreement {agreement(rng):.3f}; "
      f"paired discordance at zero lift {discordance(rng, shift_for(0.0)):.3f} (SE={SE})")
for reps in (1, 2):
    for tau in (0.0, 0.25):
        for delta in (0.0, 0.10, 0.15):
            g = shift_for(delta)
            out = [one_run(rng, g, tau, reps) for _ in range(SIMS)]
            print(f"r={reps} tau={tau:.2f} true lift {delta:.2f}: "
                  f"P(SUPPORTED)={out.count('S')/SIMS:.2f} P(RULED OUT)={out.count('R')/SIMS:.2f} "
                  f"P(INCONCLUSIVE)={out.count('I')/SIMS:.2f}")
