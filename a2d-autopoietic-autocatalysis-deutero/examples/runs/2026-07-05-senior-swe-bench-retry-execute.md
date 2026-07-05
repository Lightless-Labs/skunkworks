# Senior SWE-Bench retry execute — 2026-07-05

## Purpose

Add the first bounded executor wrapper over the existing Senior SWE-Bench retry gates.

This is not a provider/cycle runner and not official Senior SWE-Bench mastery. It consumes precomputed cycle-output manifests and composes the existing machine-verifiable gates: attempt planning, patch extraction, evaluator execution, retry-step decision, fitness-evidence inspection, and terminal run-result summary.

## Command

```bash
a2d senior-swe-bench-retry-execute \
  --retry-plan <retry-plan.json> \
  --task-cycle-input <task-cycle-input.json> \
  --checkout <benchmark-checkout> \
  --work-dir <dir> \
  --attempt-output-manifest <manifest.json> [...] \
  [--apply-candidate-patch] \
  [--official-evaluator-manifest <json>] \
  -- <evaluator> [args...]
```

Output schema: `a2d.senior-swe-bench-retry-execution.v1`.

## Behavior

- Validates the existing retry plan and task-cycle-input binding.
- Bounds execution by retry-plan `max_attempts` and supplied precomputed manifests.
- Starts no provider or cycle invocations itself.
- Runs the existing deterministic gates for each supplied manifest.
- On passed evaluation, runs the planned `fitness-evidence-inspect --require-all-tests-pass` gate and emits the existing run-result summary.
- On failed evaluation, either writes the next feedback-enriched cycle input or stops with an explicit non-success reason.
- Keeps local-wrapper evidence non-official unless a real official evaluator manifest is present.

## Validation

Run directory: `runs/20260705-senior-swe-bench-retry-execute-evidence/`.

Commands passed:

```bash
cargo fmt --check
cargo test -p a2d --test senior_swe_bench_retry_execute --test senior_swe_bench_retry_run_result --test senior_swe_bench_retry_attempt_step_evidence -- --nocapture
cargo test
target/debug/a2d fitness-evidence-inspect \
  runs/20260705-senior-swe-bench-retry-execute-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json \
  --require-all-tests-pass
```

Independent review found no critical issues. Reviewer warnings about top-level evaluator invocation accounting and precomputed-manifest exhaustion wording were fixed before final tests.

## Source-patch evidence

Fresh source-patch gate:

- `runs/20260705-senior-swe-bench-retry-execute-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`
- `schema_version: a2d.fitness-evidence.v1`
- `actual_tests_evaluated: true`
- `non_regressing: true`
- `fitness: 1.0`
- `failed_cases: []`
- `source_diff_scope: crates`
- `source_diff_hash: fee83715a54f10f0e73e8c8bbf73591e6cb921e8`

After commit `ada37b2`, clean-HEAD evidence was regenerated at `runs/20260705-postcommit-fitness-evidence-ada37b2/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`; it passes `fitness-evidence-inspect --require-all-tests-pass`, records `source_revision: a9652a3` for `HEAD:a2d-autopoietic-autocatalysis-deutero/crates`, `source_tree_dirty: false`, and clean `source_diff_hash: e69de29bb2d1d6434b8b29ae775ad8c2e48c5391`.

This is bounded executor plumbing and source-patch evidence only. It is not an official Senior SWE-Bench success claim and does not prove top-level A²D goal completion.

## Artifact persistence addendum

Follow-up slice: the executor now persists the typed intermediate artifacts it composes into the supplied work directory instead of leaving them only in memory/stdout. A successful attempt writes:

- `attempt-0/retry-attempt-plan.json`
- `attempt-0/retry-attempt-extraction.json`
- `attempt-0/retry-attempt-evaluation.json`
- `attempt-0/retry-attempt-step-execution.json`
- `attempt-0/retry-attempt-step-evidence-execution.json`
- `attempt-0/retry-run-result.json`
- `retry-execution.json`

Failure paths persist the artifacts reached before the stop decision plus the terminal `retry-execution.json`; success-only evidence/run-result artifacts are not invented for failed evaluations. JSON artifact writes fail closed if the destination already exists.

Validation/evidence for this addendum lives under `runs/20260705-senior-swe-bench-retry-execute-artifacts-evidence/`:

```bash
cargo fmt --check
cargo test -p a2d --test senior_swe_bench_retry_execute -- --nocapture
cargo test
target/debug/a2d fitness-evidence-inspect \
  runs/20260705-senior-swe-bench-retry-execute-artifacts-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json \
  --require-all-tests-pass
```

Source-patch gate: `runs/20260705-senior-swe-bench-retry-execute-artifacts-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, `source_diff_hash: 0a6c6625acac24d056b8c3dc38331c47f8c5eb5b`. A transient local retry-execute smoke confirmed the composed path, but host-local evaluator/temp paths were not committed. This remains non-official benchmark plumbing evidence.
