# Senior SWE-Bench retry-attempt extract patch

## Purpose

Execute only the first planned command from `a2d.senior-swe-bench-retry-attempt-plan.v1`: materialize the candidate patch bytes from the selected coder artifact, then hand off evaluator/retry-step args without running providers or evaluators.

## Command

```bash
a2d senior-swe-bench-retry-attempt-extract-patch <retry-attempt-plan.json|->
```

Output schema: `a2d.senior-swe-bench-retry-attempt-extraction.v1`.

## Behavior

- Requires retry-attempt plan schema `a2d.senior-swe-bench-retry-attempt-plan.v1`.
- Requires `decision: extract_and_evaluate_candidate_patch`; stop/prose plans are rejected before writing files.
- Re-reads the selected artifact path, verifies byte count and `git hash-object` hash, and re-runs unified-diff extraction.
- Rejects public GitHub solution references via the shared extractor.
- Requires extracted patch hash to match the plan `candidate_patch_hash`.
- Writes `planned_outputs.candidate_patch` idempotently: exact existing bytes are accepted; mismatched existing files fail closed.
- Emits copied `evaluate_args` and `retry_step_args` for the next gates.
- Starts no providers/evaluators and makes no fitness claim.

## Validation

Run directory: `runs/20260704-senior-swe-bench-retry-attempt-extract-patch-evidence/`.

Focused/full validation:

```bash
cargo fmt --check
cargo test -p a2d --test senior_swe_bench_retry_attempt_extract_patch -- --nocapture
cargo test -p a2d --test senior_swe_bench_retry_attempt_plan -- --nocapture
cargo test -p a2d senior_swe_bench -- --nocapture
cargo test
```

Smokes:

- `extract-smoke/extraction.json` — materializes `attempt-0/candidate.patch`, emits next args, and records no provider/evaluator invocation.
- `negative-smoke/stop-plan.err` — stop/prose plans are rejected before patch write.
- `negative-smoke/tampered.err` — same-length selected artifact tamper fails by hash mismatch.
- `negative-smoke/stale-patch.err` — existing mismatched patch file fails closed.

Fresh source-patch evidence:

- Pre-commit dirty-tree source gate: `runs/20260704-senior-swe-bench-retry-attempt-extract-patch-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`
- `source_diff_hash: aa8a4df265620b7ef92923a8e18ea693fd54b7b0`
- Matched the then-current `git diff --binary HEAD -- crates | git hash-object --stdin` before commit.

Post-commit clean-HEAD evidence was regenerated after `b0abcf6` because the pre-commit evidence correctly stopped validating once the source tree became clean:

- `runs/20260704-postcommit-fitness-evidence-b0abcf6/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`
- `source_revision: c7cd22e` for `HEAD:a2d-autopoietic-autocatalysis-deutero/crates`
- `source_tree_dirty: false`
- `source_diff_hash: e69de29bb2d1d6434b8b29ae775ad8c2e48c5391`
- `fitness-evidence-inspect --require-all-tests-pass` passed.

This is deterministic extraction plumbing/source-patch evidence. It is not evaluator execution, an autonomous retry executor, or official Senior SWE-Bench mastery.
