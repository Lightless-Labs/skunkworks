import json
import tempfile
import unittest
from argparse import Namespace
from pathlib import Path
from unittest.mock import patch

from bench.evolve import evaluate


class EvaluateTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.private = self.root / "private"
        self.private.mkdir()
        self.genome = self.root / "genome"
        self.genome.mkdir()
        (self.genome / "policy.py").write_text("# fixture\n")
        self.cfg = Namespace(run_id="run-x", label="G0", timeout=1500, a2ctl=Path("a2ctl"),
                             shard="0/1", repeat=1, genome=self.genome, endpoint=[],
                             provider="codex/gpt-6-luna", max_tokens=2000000,
                             reasoning_effort="high")
        self.calls = []
        self.patches = [
            patch.object(evaluate.tasks, "private_dir", return_value=self.private),
            patch.object(evaluate.tasks, "prepare", side_effect=lambda task: self.calls.append(("prepare", task))),
            patch.object(evaluate.tasks, "evict", side_effect=lambda task: self.calls.append(("evict", task))),
            patch.object(evaluate.e1run, "start_proxy", return_value=None),
            patch.object(evaluate.codex_homes, "ensure_role_home", return_value=self.root / "codex"),
            patch.object(evaluate.lock, "verify", return_value=(True, [])),
        ]
        for p in self.patches:
            p.start()
            self.addCleanup(p.stop)
        self.episode = patch.object(evaluate.episode, "run_episode", side_effect=self.fake_episode)
        self.episode.start()
        self.addCleanup(self.episode.stop)

    def tearDown(self):
        self.temp.cleanup()

    def fake_episode(self, cfg, task, genome, run_dir, nonce):
        self.calls.append(("episode", task, nonce))
        return {"opaque_id": task, "nonce": nonce, "genome_sha256": evaluate.episode._genome_sha(Path(genome)),
                "agent_calls": 1, "tokens_in": 2, "tokens_out": 3, "wall_secs": 1,
                "policy_error": None, "infra_failure": None,
                "grade": {"resolved": True, "hidden_resolved": True, "required": 2,
                          "required_passed": 2, "rejected": False}}

    def rows(self, run_id="run-x", shard="0of1"):
        path = self.private / "evolve-runs" / run_id / ("rows.shard" + shard + ".jsonl")
        return [json.loads(line) for line in path.read_text().splitlines() if line]

    def test_shards_partition_tasks(self):
        self.cfg.shard = "1/2"
        evaluate.evaluate_tasks(self.cfg, ["a", "b", "c", "d"])
        self.assertEqual([r["opaque_id"] for r in self.rows(shard="1of2")], ["b", "d"])

    def test_resume_skips_done_and_repeat_records_rep(self):
        self.cfg.repeat = 2
        evaluate.evaluate_tasks(self.cfg, ["task"])
        evaluate.evaluate_tasks(self.cfg, ["task"])
        self.assertEqual([r["rep"] for r in self.rows()], [0, 1])
        self.assertEqual(sum(call[0] == "episode" for call in self.calls), 2)

    def test_infra_retries_once_and_keeps_retry_row(self):
        values = iter(["temporary", None])
        def row(cfg, task, genome, run_dir, nonce):
            return {"opaque_id": task, "nonce": nonce, "genome_sha256": evaluate.episode._genome_sha(Path(genome)),
                    "infra_failure": next(values), "grade": None}
        with patch.object(evaluate.episode, "run_episode", side_effect=row) as episode_mock:
            evaluate.evaluate_tasks(self.cfg, ["task"])
        saved = self.rows()[0]
        self.assertIsNone(saved["infra_failure"])
        self.assertEqual(saved["infra_retry_of"], "temporary")
        self.assertEqual(episode_mock.call_count, 2)

    def test_lock_mismatch_refuses_to_start(self):
        with patch.object(evaluate.lock, "verify", return_value=(False, ["bench/evolve/episode.py"])):
            with self.assertRaises(SystemExit):
                evaluate.evaluate_tasks(self.cfg, ["task"])
        self.assertFalse(any(call[0] == "prepare" for call in self.calls))


if __name__ == "__main__":
    unittest.main()
