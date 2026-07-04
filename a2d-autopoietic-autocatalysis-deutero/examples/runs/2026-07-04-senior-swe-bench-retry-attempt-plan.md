# Senior SWE-Bench retry-attempt plan

## Purpose

Compose the existing deterministic gates for one bounded retry attempt without starting providers or evaluators: select the exact coder artifact, verify it is extractable as a candidate patch, and emit the exact extraction/evaluation/retry-step command arguments.

## Command

```bash
a2d senior-swe-bench-retry-attempt-plan \
  --retry-plan <retry-plan.json|-> \
  --attempt-index <n> \
  --task-cycle-input <task-cycle-input.json|-> \
  --cycle-output-manifest <manifest.json|-> \
  --checkout <dir> \
  --attempt-dir <dir> \
  [--apply-candidate-patch] \
  [--official-evaluator-manifest <json>] \
  -- <evaluator> [args...]
```

The output schema is `a2d.senior-swe-bench-retry-attempt-plan.v1`.

## Behavior

- Validates retry-plan bounds/stop criteria via the retry-step validator.
- Validates task-cycle-input task/repo/no-solution-search binding.
- Reuses candidate-artifact selection to require exactly one coder/code artifact, byte/hash match, and no public GitHub solution references.
- For extractable unified diffs, emits `extract_patch_args`, `evaluate_args`, `retry_step_args`, planned output paths, and candidate patch hash.
- For prose/non-diff artifacts, stops with `candidate_patch_extraction_failed` and emits no evaluator args.
- Starts no providers/evaluators, writes no attempt files, and makes no fitness claim.

## Validation

Run directory: `runs/20260704-senior-swe-bench-retry-attempt-plan-evidence/`.

Focused/full validation:

```bash
cargo fmt --check
cargo test -p a2d --test senior_swe_bench_retry_attempt_plan -- --nocapture
cargo test -p a2d --test senior_swe_bench_select_candidate_artifact -- --nocapture
cargo test -p a2d senior_swe_bench -- --nocapture
cargo test -p a2d cycle_output_artifact -- --nocapture
cargo test
```

Smokes:

- `retry-attempt-plan-smoke/retry-attempt-plan.json` — valid diff emits selection/extraction/evaluation/retry-step args and preserves no-provider/no-evaluator booleans.
- `retry-attempt-plan-smoke/prose-stop-plan.json` — prose artifact stops fail-closed with no evaluator args.
- `negative-smoke/missing-command.err` — `--` with no evaluator command is rejected.
- `negative-smoke/multi-stdin.err` — multiple `-` JSON inputs are rejected before any stdin read.
- `negative-smoke/missing-official.err` — missing official evaluator manifest path fails at planning time.

Fresh source-patch evidence:

- `runs/20260704-senior-swe-bench-retry-attempt-plan-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`
- `source_diff_hash: 95e5f526834ef5bce322ab1f474e9f9ef5fcba0b`
- Matches `git diff --binary HEAD -- crates | git hash-object --stdin`.

This is deterministic retry-attempt planning/source-patch evidence. It is not an autonomous retry executor and not official Senior SWE-Bench mastery.
