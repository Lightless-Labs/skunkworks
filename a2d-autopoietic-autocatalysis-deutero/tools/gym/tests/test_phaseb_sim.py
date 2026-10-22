"""Tests for tools/gym/phaseb_sim.py (pure, stdlib, seeded: deterministic)."""
import random
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
import phaseb_sim as sim  # noqa: E402


class MixtureTest(unittest.TestCase):
    def test_shares_hit_mean_and_discordance(self):
        rng = random.Random(1)
        ps, fams = sim.draw_gym(5000, 0.5, 0.25, 1.5, rng)
        self.assertEqual(len(ps), 20000)
        self.assertEqual(fams[:8], [0, 0, 0, 0, 1, 1, 1, 1])
        self.assertAlmostEqual(sum(ps) / len(ps), 0.5, delta=0.01)
        a, b = sim.bern(ps, rng), sim.bern(ps, rng)
        self.assertAlmostEqual(sum(x != y for x, y in zip(a, b)) / len(ps), 0.25, delta=0.01)

    def test_cycle2_like_point_is_feasible_and_infeasible_points_raise(self):
        a, b, c = sim.mixture(0.84, 0.11)
        self.assertAlmostEqual(a + b + c, 1.0)
        self.assertGreater(a, 0.6)
        with self.assertRaises(ValueError):
            sim.mixture(0.9, 0.3)          # more discordance than a 90% pass rate allows


class EffectTest(unittest.TestCase):
    def test_mean_shift_equals_delta_both_signs(self):
        rng = random.Random(2)
        ps, fams = sim.draw_gym(5000, 0.5, 0.25, 1.5, rng)
        base = sum(ps) / len(ps)
        for delta in (0.05, -0.05, 0.10):
            q = sim.shifted(ps, fams, delta, 0.0, 0.5, rng)
            self.assertAlmostEqual(sum(q) / len(q) - base, delta, delta=0.006)
            self.assertTrue(all(0.0 <= x <= 1.0 for x in q))

    def test_zero_effect_is_identity(self):
        rng = random.Random(3)
        ps, fams = sim.draw_gym(50, 0.5, 0.25, 1.5, rng)
        self.assertEqual(sim.shifted(ps, fams, 0.0, 0.0, 0.5, rng), ps)


class ScreenTest(unittest.TestCase):
    def test_aa_screen_passes_a_null_about_five_percent(self):
        p = dict(sim.DEFAULTS)
        rows = sim.screen_table(p, ["aa", "fixed"], [0.0], [15], 2000, 7)
        aa = next(r for r in rows if r["design"] == "aa")
        fixed = next(r for r in rows if r["design"] == "fixed")
        self.assertLess(aa["p_screen"], 0.09)
        self.assertGreater(fixed["p_screen"], 0.25)     # protocol v2's +2 bar at 60 tasks, d = 0.25
        self.assertLess(aa["p_keep"], 0.02)

    def test_large_effect_is_kept_and_cost_counts_continuations(self):
        p = dict(sim.DEFAULTS)
        rows = sim.screen_table(p, ["aa", "half"], [0.0, 0.15], [15], 400, 8)
        null, big = (next(r for r in rows if r["delta"] == d and r["design"] == "aa") for d in (0.0, 0.15))
        # The A/A bar on a 60-task round is about 10.6 pp, against an SD of about 7 pp at +15 pp.
        self.assertGreater(big["p_keep"], 0.6)
        half = next(r for r in rows if r["delta"] == 0.15 and r["design"] == "half")
        self.assertGreater(half["p_keep"], 0.9)
        c = p["calls_per_task"]
        round_only, full, confirm = 1.5 * 60 * c, 2 * (240 - 60) * c, 2 * 240 * c
        # A screened proposal pays the full stage, and the confirmation if the full stage passes.
        self.assertGreaterEqual(null["luna_per_proposal"], round_only + null["p_screen"] * full - 1e-9)
        self.assertLessEqual(null["luna_per_proposal"], round_only + null["p_screen"] * (full + confirm) + 1e-9)
        self.assertGreater(big["luna_per_proposal"], round_only + big["p_keep"] * (full + confirm) - 1e-9)

    def test_keep_probability_interpolates(self):
        table = [{"design": "aa", "round_tasks": 60, "delta": 0.0, "p_keep": 0.0},
                 {"design": "aa", "round_tasks": 60, "delta": 0.1, "p_keep": 0.8}]
        f = sim.keep_probability(table, "aa", 60)
        self.assertAlmostEqual(f(0.05), 0.4)
        self.assertEqual(f(-1), 0.0)
        self.assertEqual(f(1), 0.8)


class ComparisonTest(unittest.TestCase):
    def test_fisher_one_sided(self):
        self.assertAlmostEqual(sim.fisher_one_sided(3, 4, 0, 4), 4 / 56)
        self.assertAlmostEqual(sim.fisher_one_sided(0, 6, 0, 6), 1.0)

    def test_round_test_holds_size_and_cluster_test_does_not_under_proposal_spread(self):
        p = dict(sim.DEFAULTS, sd_delta=0.05)
        row = sim.comparison_table(p, [0.0], [15], [6], 1500, 9)[0]
        self.assertLess(row["reject_round"], 0.08)
        # Analytic: the cluster SE omits the 2·sd² proposal spread, so its size is about 0.13, not 0.05.
        self.assertGreater(row["reject_cluster"], 0.10)

    def test_t95_table(self):
        self.assertAlmostEqual(sim.t95(5), 2.0150)
        self.assertAlmostEqual(sim.t95(500), sim.Z95)


class ScenarioTest(unittest.TestCase):
    def test_skew_effect_matches_normal_mean_and_sd(self):
        rng = random.Random(10)
        xs = [sim.effect_draw("skew", 0.01, 0.035, rng.gauss(0, 1)) for _ in range(60000)]
        self.assertAlmostEqual(sum(xs) / len(xs), 0.01, delta=0.0006)
        self.assertAlmostEqual(sim.statistics.pstdev(xs), 0.035, delta=0.001)
        self.assertAlmostEqual(sim.effect_draw("normal", 0.01, 0.035, 1.0), 0.045)

    def test_quarters_are_disjoint_and_cover_each_cycle(self):
        rounds = sim.schedule(60, 15, 8, "quarters", random.Random(11))
        self.assertEqual(len(rounds), 8)
        for cycle in (rounds[:4], rounds[4:]):
            self.assertEqual(set().union(*cycle), set(range(60)))
            self.assertEqual(sum(len(r) for r in cycle), 60)

    def test_task_spread_keeps_the_mean_and_mixes_signs(self):
        rng = random.Random(12)
        ps, fams = sim.draw_gym(5000, 0.5, 0.25, 1.5, rng)
        q = sim.shifted(ps, fams, 0.05, 0.0, 0.5, rng, task_sd=0.10)
        self.assertAlmostEqual(sum(q) / len(q) - sum(ps) / len(ps), 0.05, delta=0.008)
        self.assertTrue(any(b < a for a, b in zip(ps, q)))       # some tasks regress

    def test_registered_bootstrap_direction(self):
        pos = [[(f, 1) for f in range(6)] + [(f, 0) for f in range(6)] for _ in range(4)]
        zero = [[(f, 1) for f in range(3)] + [(f, -1) for f in range(3, 6)] for _ in range(4)]
        self.assertTrue(sim.registered_bootstrap_test(pos, reps=200))
        self.assertFalse(sim.registered_bootstrap_test(zero, reps=200))

    def test_round_test_size_under_the_conservative_scenario(self):
        rows = sim.scenario_rows(dict(sim.DEFAULTS), sim.SCENARIOS["conservative"], [(9, 20)], [0.0], 500, 13)
        self.assertLess(rows[0]["reject_round"], 0.09)


if __name__ == "__main__":
    unittest.main()
