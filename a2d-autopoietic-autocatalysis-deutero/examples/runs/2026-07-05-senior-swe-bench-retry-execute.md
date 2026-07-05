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

Post-commit clean-HEAD evidence: `runs/20260705-postcommit-fitness-evidence-7167562/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, `source_revision: 7b528d1`, `source_tree_dirty: false`, `source_diff_hash: e69de29bb2d1d6434b8b29ae775ad8c2e48c5391`.

## Next-cycle input no-overwrite addendum

Follow-up hardening: failed non-final attempts now write `attempt-<n>/next-cycle-input.json` via the same fail-closed JSON artifact helper as the retry attempt/run artifacts. This prevents a stale feedback input from being silently overwritten before a later retry/cycle orchestration step consumes it.

Validation/evidence:

```bash
cargo fmt --check
cargo test -p a2d --test senior_swe_bench_retry_execute -- --nocapture
cargo test
target/debug/a2d fitness-evidence-inspect \
  runs/20260705-retry-execute-next-cycle-input-no-overwrite-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json \
  --require-all-tests-pass
```

Source-patch gate: `runs/20260705-retry-execute-next-cycle-input-no-overwrite-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, `source_diff_hash: 875e1a0f511cb98ad1736e21f964e8f5d0f0efab`.

Post-commit clean-HEAD evidence: `runs/20260705-postcommit-fitness-evidence-e88c627/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, `source_revision: 748e18f`, `source_tree_dirty: false`, `source_diff_hash: e69de29bb2d1d6434b8b29ae775ad8c2e48c5391`.

## Next-cycle command persistence addendum

Follow-up hardening: when a failed non-final attempt produces a feedback-enriched `next-cycle-input.json`, the retry execution attempt record and terminal summary now include `next_cycle_command`. This records the exact next live cycle boundary without starting it:

```text
a2d cycle-input <attempt-N/next-cycle-input.json> 1 \
  --checkout <benchmark-checkout> \
  --output-artifacts <attempt-N+1/cycle-output-artifacts>
```

The record also includes the expected next `manifest.json` path plus `provider_invocations_started: false` and `fitness_claim_allowed_before_evidence: false`. The executor still consumes only precomputed manifests; this command data is for the next resume/live-provider orchestration step.

Validation/evidence:

```bash
cargo fmt --check
cargo test -p a2d --test senior_swe_bench_retry_execute -- --nocapture
cargo test
target/debug/a2d fitness-evidence-inspect \
  runs/20260705-retry-execute-next-cycle-command-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json \
  --require-all-tests-pass
```

Source-patch gate: `runs/20260705-retry-execute-next-cycle-command-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, `source_diff_hash: 781675eec3e4e261c7b5ab08f114a06f78d9e603`.

Post-commit clean-HEAD evidence: `runs/20260705-postcommit-fitness-evidence-d4557a1/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, `source_revision: d2a2c87`, `source_tree_dirty: false`, `source_diff_hash: e69de29bb2d1d6434b8b29ae775ad8c2e48c5391`.

## Retry resume attempt-plan addendum

Follow-up command: `a2d senior-swe-bench-retry-resume-attempt-plan --retry-execution <retry-execution.json> --retry-plan <retry-plan.json> --cycle-output-manifest <manifest.json> [--apply-candidate-patch] [--official-evaluator-manifest <json>] -- <evaluator> [args...]`.

This consumes a persisted failed non-final retry execution after the recorded `next_cycle_command` has produced the next manifest. It validates the saved command boundary, expected manifest path, no-provider/no-pre-evidence-fitness flags, task input, checkout, attempt count/index consistency, last-attempt metadata, and expected attempt directory before emitting the existing retry-attempt plan for the new manifest. It does not run providers, evaluators, retry-step, or evidence inspection.

Validation/evidence:

```bash
cargo fmt --check
cargo test -p a2d --test senior_swe_bench_retry_execute -- --nocapture
cargo test
target/debug/a2d fitness-evidence-inspect \
  runs/20260705-retry-resume-attempt-plan-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json \
  --require-all-tests-pass
```

Source-patch gate: `runs/20260705-retry-resume-attempt-plan-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, `source_diff_hash: f927a382b84f1280994776f4285b1adf4b8a723d`, `actual_tests_evaluated: true`, `non_regressing: true`, `fitness: 1.0`, `failed_cases: []`.

Post-commit clean-HEAD evidence: `runs/20260705-postcommit-fitness-evidence-58150c5/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, `source_revision: 074fd8c`, `source_tree_dirty: false`, `source_diff_hash: e69de29bb2d1d6434b8b29ae775ad8c2e48c5391`.

This is resume orchestration planning over persisted retry artifacts. It is not an official Senior SWE-Bench success claim and does not prove top-level A²D goal completion.

## Retry next-cycle execution addendum

Follow-up command: `a2d senior-swe-bench-retry-run-next-cycle --retry-execution <retry-execution.json>`.

This consumes a failed retry execution whose saved `next_cycle_command` points at the next `cycle-input` run. It validates the failed/precomputed-manifest-exhausted boundary, exact argv order, expected manifest path, task/repo identity against the next cycle input, checkout/input existence, attempt metadata, no-pre-evidence-fitness flags, and absent pre-existing manifest before running the current `a2d` executable once. Child execution is bounded by `A2D_SENIOR_SWE_BENCH_RETRY_NEXT_CYCLE_TIMEOUT_SECS` (default 1800s). It persists `attempt-N/retry-next-cycle-execution.json` on success, nonzero exit, spawn failure, timeout, or invalid manifest; no evaluator, retry-step, or `fitness-evidence-inspect` command is started.

Success requires a valid `a2d.cycle-output-artifacts.v1` manifest whose artifacts have readable paths, byte counts, and matching `git hash-object` hashes. Failed/spawn/timeout summaries keep `fitness_claim_allowed_after_cycle: false`.

Validation/evidence:

```bash
cargo fmt --check
cargo test -p a2d retry_run_next_cycle -- --list
cargo test -p a2d retry_run_next_cycle -- --nocapture
cargo test -p a2d --test senior_swe_bench_retry_execute -- --nocapture
cargo test
target/debug/a2d fitness-evidence-inspect \
  runs/20260705-retry-run-next-cycle-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json \
  --require-all-tests-pass
```

Source-patch gate: `runs/20260705-retry-run-next-cycle-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, `source_diff_hash: 254682e7891a408eeee2a43d78c76225ecaca7ed`, `actual_tests_evaluated: true`, `non_regressing: true`, `fitness: 1.0`, `failed_cases: []`.

This is bounded next-cycle orchestration over persisted retry artifacts. It is not evaluator execution, not an official Senior SWE-Bench success claim, and does not prove top-level A²D goal completion.

## Retry next-cycle summary resume addendum

Follow-up hardening: `a2d senior-swe-bench-retry-resume-attempt-plan` can now consume the successful `attempt-N/retry-next-cycle-execution.json` summary directly via `--next-cycle-execution <retry-next-cycle-execution.json>`. `senior-swe-bench-retry-run-next-cycle` persists `cycle_output_manifest_git_object_hash` for successful summaries; the resume consumer recomputes the current manifest hash and rejects stale/overwritten manifests, failed summaries, summaries at the wrong attempt path, mismatched prior-boundary metadata/`next_cycle_command`, and pre-evidence fitness claim fields before it emits the next retry-attempt plan.

Validation/evidence:

```bash
cargo fmt --check
cargo test -p a2d --test senior_swe_bench_retry_execute -- --nocapture
cargo test
target/debug/a2d fitness-evidence-inspect \
  runs/20260705-retry-next-cycle-summary-resume-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json \
  --require-all-tests-pass
```

Source-patch gate: `runs/20260705-retry-next-cycle-summary-resume-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, `source_diff_hash: 4bd31c0927ac95d9bc48bca4a8c028626f36f6a1`, `actual_tests_evaluated: true`, `non_regressing: true`, `fitness: 1.0`, `failed_cases: []`.

This is retry summary/resume plumbing. It is not evaluator execution, not an official Senior SWE-Bench success claim, and does not prove top-level A²D goal completion.
