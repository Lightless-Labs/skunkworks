"""python3 -I nano/test_look_if_promoted.py"""
import datetime
import json
import pathlib
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import look_if_promoted as lp  # noqa: E402

UTC = datetime.timezone.utc
BEFORE = datetime.datetime(2026, 10, 22, 12, 0, tzinfo=UTC)
HEADER = "id\tdate\tproposer\tstage\thypothesis\tdelta_frac\tse\tdelta_allpass\tse_allpass\tdecision\n"


def rows(*lines):
    return HEADER + "".join("\t".join([i, "2026-10-20", "sol", stage, "h", "0", "0", "0", "0", dec]) + "\n"
                            for i, stage, dec in lines)


def decision(cmd):
    return json.loads(cmd.split("printf '%s\\n' ", 1)[1].strip("'"))["decision"]


class Plan(unittest.TestCase):
    def plan(self, text, now=BEFORE, attempted=False, same=True, reg="abc123"):
        return lp.plan(["E005", "E007"], text, now, attempted, same, reg)

    def test_no_promotion_keeps_e002(self):
        cmd = self.plan(rows(("E007", "full", "provisional keep"), ("E007", "confirm", "discard")))
        self.assertIn("no promoted keep", decision(cmd))

    def test_promotion_runs_exactly_one_look_before_t(self):
        text = rows(("E007", "full", "provisional keep"), ("E007", "confirm", "keep"))
        self.assertEqual(self.plan(text), "python3 -I nano/eval.py look --n 2 --registered-at abc123 --seed 6262 --jobs 8")
        self.assertIn("already attempted", decision(self.plan(text, attempted=True)))
        late = datetime.datetime(2026, 10, 24, 0, 0, tzinfo=UTC)
        self.assertIn("past T", decision(self.plan(text, now=late)))

    def test_refuses_when_the_incumbent_is_not_the_promoted_keep(self):
        text = rows(("E007", "confirm", "keep"))
        self.assertIn("incumbent", decision(self.plan(text, same=False)))

    def test_refuses_without_registration(self):
        self.assertIn("registration", decision(self.plan(rows(("E007", "confirm", "keep")), reg=None)))

    def test_ignores_keeps_outside_the_batch(self):
        self.assertIn("no promoted keep", decision(self.plan(rows(("E002", "confirm", "keep")))))


if __name__ == "__main__":
    unittest.main()
