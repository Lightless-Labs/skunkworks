import random
import unittest

from bench.evolve import stats


class StatsTests(unittest.TestCase):
    def test_null_samples_center_near_zero_and_p_value_monotone(self):
        rates = {"a": (1, 2), "b": (1, 2), "c": (1, 2)}
        samples = stats.null_delta_samples(rates, list(rates), runs=1, draws=4000, seed=3)
        self.assertLess(abs(sum(samples) / len(samples)), 0.12)
        self.assertGreaterEqual(stats.p_value(-1, samples), stats.p_value(0, samples))
        self.assertGreaterEqual(stats.p_value(0, samples), stats.p_value(1, samples))

    def test_select_screen_set_categories(self):
        rates = {"x": (2, 3), "s1": (3, 3), "s2": (3, 3), "s3": (3, 3), "s4": (3, 3),
                 "h1": (0, 3), "h2": (0, 3), "h3": (0, 3), "h4": (0, 3), "h5": (0, 3)}
        result = stats.select_screen_set(rates, sentinels=3, hard=4, seed=7)
        self.assertEqual(result["contested"], ["x"])
        self.assertEqual(len(result["sentinels"]), 3)
        self.assertEqual(len(result["hard"]), 4)
        self.assertEqual(result["S"], sorted(result["contested"] + result["sentinels"] + result["hard"]))

    def test_simulation_false_keep_and_power(self):
        rng = random.Random(31)
        ps = [0.02] * 23 + [0.97] * 4 + [rng.uniform(0.15, 0.85) for _ in range(17)]
        rates = {"t%02d" % i: (sum(rng.random() < p for _ in range(3)), 3) for i, p in enumerate(ps)}
        selected = stats.select_screen_set(rates, seed=5)["S"]
        false_keep = stats.simulate_rule(rates, selected, effect_logit=0, trials=800, seed=19)
        power = stats.simulate_rule(rates, selected, effect_logit=1.5, trials=800, seed=19)
        self.assertLessEqual(false_keep, 0.08)
        self.assertGreater(power, false_keep)


if __name__ == "__main__":
    unittest.main()
