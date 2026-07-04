# Senior SWE-Bench Retry-Attempt Step Evidence Inspection

**Date:** 2026-07-04
**Scope:** deterministic evidence-inspection execution after retry-attempt step
**Status:** source patch gated by fresh `a2d.fitness-evidence.v1` actual-test evidence

## What changed

Added CLI-only `a2d senior-swe-bench-retry-attempt-step-evidence <retry-attempt-step-execution.json|->`.

The command consumes `a2d.senior-swe-bench-retry-attempt-step-execution.v1`, revalidates no-provider/no-evaluator/no-public-GitHub-search flags, rechecks selected artifact and candidate patch byte/hash provenance, requires the embedded retry-step decision to be `inspect_fitness_evidence`, runs exactly one planned `fitness-evidence-inspect <path> --require-all-tests-pass` command, then re-reads the inspected evidence and verifies `candidate_patch_hash` plus reviewed `evaluator_kind` provenance.

It emits `a2d.senior-swe-bench-retry-attempt-step-evidence-execution.v1` with `fitness_evidence_inspection_started: true` and starts no providers or evaluators.

## Validation

Captured under `runs/20260704-senior-swe-bench-retry-attempt-step-evidence-inspect-evidence/`:

- `validation/cargo-fmt-check.out|err`
- `validation/focused-tests.out|err`
- `step-evidence-smoke/step-execution.json`
- `step-evidence-smoke/step-evidence-execution.json`
- `local-evaluator/fitness/senior-swe-bench-firezone-fix-connlib-align-device-hard-cycle-0-fitness-evidence.json`
- `local-evaluator/fitness-evidence-inspect.txt`
- `actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`
- `actual-test-score-artifact/fitness-evidence-inspect.txt`

Commands passed:

```bash
cargo fmt --check
cargo test -p a2d --test senior_swe_bench_retry_attempt_step_evidence --test senior_swe_bench_retry_attempt_step --test senior_swe_bench_retry_step -- --nocapture
cargo test

target/debug/a2d senior-swe-bench-retry-attempt-step-evidence \
  runs/20260704-senior-swe-bench-retry-attempt-step-evidence-inspect-evidence/step-evidence-smoke/step-execution.json
```

Fresh source-patch evidence:

- `runs/20260704-senior-swe-bench-retry-attempt-step-evidence-inspect-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`
- schema: `a2d.fitness-evidence.v1`
- `actual_tests_evaluated: true`
- `non_regressing: true`
- `fitness: 1.0`
- `failed_cases: []`
- `source_diff_scope: crates`
- `source_diff_hash: 51733d777b8c4356279f7b970281bbd330ad0709`

Local-wrapper evidence inspected by the new command:

- `runs/20260704-senior-swe-bench-retry-attempt-step-evidence-inspect-evidence/local-evaluator/fitness/senior-swe-bench-firezone-fix-connlib-align-device-hard-cycle-0-fitness-evidence.json`
- `actual_tests_evaluated: true`
- `non_regressing: true`
- `all_tests_pass: true`
- `hidden_acceptance: true`
- `candidate_patch_hash: 7ab3f194a20701a3bece6215b95a1fd7912eff48`
- `candidate_patch_artifact_hash: 65a0f6cc634a9d8789b2e0f3c852a0a8dd1c3919`
- `evaluator_kind: provided_local_command`

## Claim boundary

This is deterministic retry-attempt evidence-inspection plumbing. It proves the local-wrapper evidence gate can be executed and bound mechanically; it is not an autonomous retry loop and not official Senior SWE-Bench mastery.
