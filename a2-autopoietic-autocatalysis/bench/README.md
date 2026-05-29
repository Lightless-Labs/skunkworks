# Benchmark Harness

## Self-Correction Benchmark

`bench/self_correction.py` is the first loop-shaped A² benchmark. It creates an isolated git worktree, injects a deterministic fixture regression, commits that bug only in the isolated branch, and runs repeated `a2ctl run --apply` attempts with the same JSONL `task_id`.

Smoke the fixture without a model:

```bash
bench/self_correction.py --smoke-only --results /tmp/a2-self-correction-smoke.jsonl
```

Run real A² attempts with a cheap provider:

```bash
bench/self_correction.py --provider gemini --attempts 3 \
  --results bench/self-correction-results.jsonl
```

Run the harder compound fixture that usually exercises the loop instead of pass@1:

```bash
bench/self_correction.py --fixture compound-hidden \
  --provider opencode/minimax-coding-plan/MiniMax-M2.7 \
  --attempts 3 \
  --results bench/self-correction-compound-results.jsonl
```

Current loop-shaped fixtures:

- `fibonacci` — visible one-crate `a2_core` regression; usually too easy.
- `compound-hidden` — visible `a2_core`, hidden `a2ctl` TODO scanner regression.
- `compound-membrane-hidden` — visible `a2_core`, hidden `a2_membrane` deny-overrides-allow regression.
- `compound-archive-hidden` — visible `a2_core`, hidden `a2_archive` lineage ordering regression.
- `compound-sensorium-same-crate-hidden` — visible and hidden regressions in `a2_sensorium`.
- `compound-raf-same-crate-hidden` — visible and hidden regressions in `a2_raf` graph edge-case behavior.

Smoke a specific fixture without a model:

```bash
bench/self_correction.py --fixture compound-raf-same-crate-hidden \
  --smoke-only \
  --results /tmp/a2-raf-fixture-smoke.jsonl
```

Each JSONL result includes:

- `task_id`, `run_id`, `attempt`, `category`
- `provider` / `model`
- `resolved`
- `prior_lineage_present`
- `lineage_records_before` / `lineage_records_after`
- `lineage_reconciled_by_core`
- `anti_repeat_retry_enabled` / `ablation`
- `touched_files`, `touched_file_count`, `diff_added_lines`, `diff_removed_lines`
- verification command, return code, duration, stdout, stderr

Score self-correction specifically:

```bash
bench/self_correction_score.py bench/self-correction-results.jsonl
```

This scorer reports `pass@1` separately from `self-corrected`. A first-attempt pass is useful model capability data, but it does not exercise prior-lineage self-correction.

Run the anti-repeat retry ablation by keeping the same fixture/provider/attempt budget and writing enabled/disabled cohorts to one log or to paired logs:

```bash
: > /tmp/a2-anti-repeat-ablation.jsonl
bench/self_correction.py --fixture compound-hidden \
  --provider opencode/minimax-coding-plan/MiniMax-M2.7 \
  --attempts 3 \
  --results /tmp/a2-anti-repeat-ablation.jsonl
bench/self_correction.py --fixture compound-hidden \
  --provider opencode/minimax-coding-plan/MiniMax-M2.7 \
  --attempts 3 \
  --disable-anti-repeat \
  --results /tmp/a2-anti-repeat-ablation.jsonl
bench/self_correction_score.py /tmp/a2-anti-repeat-ablation.jsonl
```

`--disable-anti-repeat` passes `--disable-anti-repeat-retry` to `a2ctl run`. Candidate-worktree verification commands, prior lineage, verifier-derived relevant files, and retry acceptance criteria remain enabled.

After each attempt, `a2ctl run --apply` reconciles the newest lineage row with the core post-apply rebuild result. The harness records whether that core reconciliation path ran, but does not patch lineage itself.

The benchmark removes its isolated worktree by default. Use `--keep-workspace` to inspect a run.

## BigCodeBench / Legacy Harness

The old `bench/tasks/*.toml` suite is still in-tree, but the BigCodeBench evaluation path lives in three small Python CLIs:

- `bench/bigcodebench_runner.py` emits BigCodeBench Hard tasks as JSONL.
- `bench/eval.py` sets up a task workspace, runs the verification command, and emits one JSON result object.
- `bench/score.py` aggregates a JSONL result log into pass-rate metrics.

## Task Format

`bench/eval.py` expects a JSON object on stdin with these required fields:

```json
{
  "problem_statement": "Prompt shown to A²",
  "setup_script": "shell script to prepare the task workspace",
  "test_command": "command that decides pass/fail",
  "repo_path": "/absolute/or/relative/workspace/path"
}
```

Extra fields like `task_id`, `category`, `run_id`, or `attempt` are preserved in the evaluator output so they can be scored later.

## Generate BigCodeBench Tasks

The generator defaults to:

- dataset: `bigcode/bigcodebench-hard`
- split: `v0.1.4`
- first `20` tasks

It uses the Hugging Face `datasets` package unless you pass `--dataset-path` with a local JSON or JSONL export.

```bash
python3 -m pip install datasets
python3 bench/bigcodebench_runner.py > bench/bigcodebench-hard-20.jsonl
```

Each line includes:

- `problem_statement` for `a2ctl run`
- `setup_script` for the evaluator
- `test_command`
- `repo_path`
- metadata such as `task_id`, `category`, and `difficulty`

## Run A² Against Generated Tasks

`a2ctl run` now accepts JSONL task input and will use `problem_statement` when present.

```bash
python3 bench/bigcodebench_runner.py \
  | cargo run -p a2ctl -- run --provider codex --apply
```

The generated prompts target per-task workspaces under `bench/workspaces/`.
The generated setup scripts only seed `solution.py` when it does not already exist, so you can run evaluation after `a2ctl run --apply` without clobbering the agent's output.

## Evaluate Results

The evaluator runs one task at a time. It enforces:

- 60 second timeout per command
- CPU, memory, file-size, and open-file resource limits on Unix

Example for a single task:

```bash
head -n 1 bench/bigcodebench-hard-20.jsonl | python3 bench/eval.py
```

To run the full loop and build a JSONL log:

```bash
: > bench/results.jsonl
while IFS= read -r task; do
  printf '%s\n' "$task" | cargo run -p a2ctl -- run --provider codex --apply
  printf '%s\n' "$task" | python3 bench/eval.py >> bench/results.jsonl
done < bench/bigcodebench-hard-20.jsonl
```

This keeps generation, solving, and evaluation loosely coupled:

- `bigcodebench_runner.py` decides what the task is
- `a2ctl run --apply` writes the candidate code into the repository workspace
- `eval.py` prepares any missing files and runs the benchmark test command

## Score a Log

`bench/score.py` reads the JSONL log and prints:

- overall pass rate
- `pass@1`
- `pass@3`
- category breakdown
- trend over time when multiple runs are present in the log

```bash
python3 bench/score.py bench/results.jsonl
```

If you want trend reporting across repeated runs, include `run_id` in the JSON lines before evaluation.
