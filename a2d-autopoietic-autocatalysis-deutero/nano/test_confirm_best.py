"""python3 -I -m unittest discover -s nano -p 'test_confirm_best.py'"""
import json
import pathlib
import sys
import tempfile
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import confirm_best  # noqa: E402

HEAD = "id\tdate\tproposer\tstage\thypothesis\tdelta_frac\tse\tdelta_allpass\tse_allpass\tthreshold\tn_tasks\tn_families\tdecision"


def row(exp, stage, decision, delta="+0.0000", se="0.0300", proposer="claude"):
    return "\t".join([exp, "2026-10-17", proposer, stage, "h", "+0", "0", delta, se, "t", "140", "35", decision])


class ConfirmBest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.runs = pathlib.Path(self.tmp.name)

    def tearDown(self):
        self.tmp.cleanup()

    def register(self, exp, incumbent="inc"):
        d = self.runs / f"c2-{exp}"
        d.mkdir()
        (d / "manifest.json").write_text(json.dumps({"incumbent_sha": incumbent}))
        (d / "hypothesis.txt").write_text(f"{exp}'s hypothesis\n")

    def plan(self, batch, rows):
        return confirm_best.plan(batch, "\n".join([HEAD] + rows) + "\n", "inc", self.runs)

    def test_waits_while_any_batch_experiment_is_undecided(self):
        self.register("E5")
        self.register("E7")
        out = self.plan(["E5", "E6", "E7"], [row("E5", "full", "provisional keep", "+0.0500")])
        self.assertEqual(out, confirm_best.say("waiting for E6 E7"))
        self.assertEqual(out, """echo '{"decision": "confirm-best: waiting for E6 E7"}'""")

    def test_picks_the_largest_full_stage_delta_then_the_smaller_se(self):
        for e in ("E5", "E6", "E7", "E8"):
            self.register(e)
        rows = [row("E5", "full", "provisional keep", "+0.0500", "0.0281"),
                row("E6", "screen", "discard (screen)", "+0.0100"),
                row("E7", "full", "provisional keep", "+0.0643", "0.0300", proposer="sol"),
                row("E8", "full", "provisional keep", "+0.0643", "0.0290", proposer="claude-c1")]
        out = self.plan(["E5", "E6", "E7", "E8"], rows)
        self.assertTrue(out.startswith("python3 -I nano/eval.py run --id E8 "), out)
        self.assertIn("--proposer claude-c1", out)
        self.assertNotIn("--stop-after", out)
        self.assertIn("E8'\"'\"'s hypothesis", out)

    def test_a_discarded_confirmation_moves_to_the_next_and_a_keep_stops(self):
        for e in ("E5", "E7"):
            self.register(e)
        rows = [row("E5", "full", "provisional keep", "+0.0500"),
                row("E7", "full", "provisional keep", "+0.0700"),
                row("E7", "confirm", "discard (confirm)", "+0.0100")]
        self.assertIn("--id E5 ", self.plan(["E5", "E7"], rows))
        rows.append(row("E5", "confirm", "keep", "+0.0400"))
        self.assertEqual(self.plan(["E5", "E7"], rows), confirm_best.say("E5 was already promoted"))

    def test_refuses_an_experiment_registered_against_another_incumbent(self):
        self.register("E5", incumbent="old")
        out = self.plan(["E5"], [row("E5", "full", "provisional keep", "+0.0500")])
        self.assertIn("registered against another incumbent", out)

    def test_an_integrity_review_is_a_final_decision_not_pending(self):
        for e in ("E5", "E9"):
            self.register(e)
        rows = [row("E5", "full", "provisional keep", "+0.0500"),
                row("E9", "screen", "integrity review (1 web-search events)", "+0.0300")]
        self.assertIn("--id E5 ", self.plan(["E5", "E9"], rows))

    def test_nothing_left(self):
        self.register("E5")
        self.assertEqual(self.plan(["E5"], [row("E5", "full", "discard (full)")]),
                         confirm_best.say("no provisional keep left"))


if __name__ == "__main__":
    unittest.main()
