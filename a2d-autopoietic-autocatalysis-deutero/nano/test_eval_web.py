"""python3 -I nano/test_eval_web.py"""
import pathlib
import sys
import tempfile
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
sys.argv = sys.argv[:1]
import eval as ev  # noqa: E402


class WebSearchEvents(unittest.TestCase):
    def test_counts_announced_searches_in_every_transcript_only(self):
        with tempfile.TemporaryDirectory() as d:
            run = pathlib.Path(d)
            a = run / "invocations/X-cand/t1/r0"
            b = run / "invocations/X-inc/t2/r0"
            a.mkdir(parents=True)
            b.mkdir(parents=True)
            # The deprecation notice mentions web search but is not a search.
            (a / "inv00-lift-coder.transcript.txt").write_text(
                "deprecated: `[features].web_search_request` is deprecated because web search is enabled by default.\n"
                "web search: \nweb search: site:example.org rust\ncodex\n")
            (b / "inv01-lane_judge.transcript.txt").write_text("web search: q\n")
            (b / "inv01-lane_judge.response.txt").write_text("web search: not a transcript\n")
            self.assertEqual(ev.web_search_events(run), 3)
            self.assertEqual(ev.web_search_events(run / "invocations/X-inc"), 1)

    def test_audit_counts_per_arm_failed_calls_and_other_codex_versions(self):
        with tempfile.TemporaryDirectory() as d:
            run = pathlib.Path(d)
            d_dir = run / "invocations/D/t1/r0"
            x_dir = run / "invocations/X-frozen/t1/r0"
            d_dir.mkdir(parents=True)
            x_dir.mkdir(parents=True)
            (d_dir / "inv00-lift-coder.prompt.txt").write_text("p")
            (d_dir / "inv00-lift-coder.transcript.txt").write_text(
                "Reading additional input from stdin...\n2026 ERROR rmcp: noise\nOpenAI Codex v0.162.0\n--------\n")
            (d_dir / "inv01-lift-coder.prompt.txt").write_text("p")  # failed call: no transcript
            (x_dir / "inv00-lane_judge.prompt.txt").write_text("p")
            (x_dir / "inv00-lane_judge.transcript.txt").write_text("OpenAI Codex v0.163.0\nweb search: q\n")
            self.assertEqual(ev.web_audit(run), {
                "D": {"web_search_events": 0, "transcripts": 1, "invocations_without_transcript": 1,
                      "transcripts_not_codex_0_162_0": 0},
                "X-frozen": {"web_search_events": 1, "transcripts": 1, "invocations_without_transcript": 0,
                             "transcripts_not_codex_0_162_0": 1}})



class LogResult(unittest.TestCase):
    def test_logs_each_id_stage_decision_once_and_tolerates_short_rows(self):
        with tempfile.TemporaryDirectory() as d:
            nano, saved = pathlib.Path(d), ev.NANO
            ev.NANO = nano
            try:
                (nano / "results.tsv").write_text("\t".join(ev.FIELDS) + "\nE1\tshort row\n\n")
                rec = {"id": "E5", "stage": "full", "decision": "provisional keep"}
                ev.log_result(rec)
                ev.log_result(rec)
                ev.log_result({**rec, "decision": "discard (full)"})
                lines = (nano / "results.tsv").read_text().splitlines()
                self.assertEqual([line.split("\t")[12] for line in lines if line.startswith("E5")],
                                 ["provisional keep", "discard (full)"])
            finally:
                ev.NANO = saved


if __name__ == "__main__":
    unittest.main()
