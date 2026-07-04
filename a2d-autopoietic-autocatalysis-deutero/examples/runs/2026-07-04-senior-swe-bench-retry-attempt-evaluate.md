# Senior SWE-Bench retry-attempt evaluate

**Date:** 2026-07-04

## Purpose

Execute exactly one planned evaluator command from `a2d.senior-swe-bench-retry-attempt-extraction.v1`, then emit a bounded evaluation handoff for the retry-step gate without running retry-step or treating evaluator success as a fitness claim.

## Command

```bash
a2d senior-swe-bench-retry-attempt-evaluate <retry-attempt-extraction.json|->
```

Output schema: `a2d.senior-swe-bench-retry-attempt-evaluation.v1`.

## Invariants

- Consumes only a completed retry-attempt extraction artifact.
- Re-validates no provider/evaluator invocation, no pre-evidence fitness claim, and no public GitHub solution search flags from extraction.
- Re-reads selected artifact bytes/hash and re-extracts the unified diff before any evaluator invocation.
- Parses the exact planned `senior-swe-bench-evaluate` argv, rejects duplicate/ambiguous wrapper flags before the evaluator separator, allows evaluator commands to receive their own `--` arguments, and requires `--output` to be a file.
- Validates emitted `retry_step_args` point to the same attempt index, task-cycle-input, and local-evaluation path.
- Runs exactly one planned evaluator wrapper command through the current `a2d` executable.
- Validates the produced local evaluation JSON independently of wrapper exit code: schema, task/repo, no-search, evaluator kind, patch path/hash, patch-application/preflight provenance, checkout provenance, evaluator command, source provenance, and status/exit coherence.
- Does not run retry-step or `fitness-evidence-inspect`; passed local evaluation only records a next evidence path for later inspection.

## Evidence

Run directory: `runs/20260704-senior-swe-bench-retry-attempt-evaluate-evidence/`.

Positive smoke:

- `evaluate-smoke/extraction.json`
- `evaluate-smoke/evaluation.json`
- `evaluate-smoke/attempt-0/local-evaluation.json`

Negative smokes:

- `negative-smoke/tampered.err` — candidate patch hash mismatch rejected before trusting evaluator output.
- `negative-smoke/duplicate-output.err` — duplicate planned evaluator `--output` rejected before invocation.
- `negative-smoke/retry-step-mismatch.err` — mismatched retry-step `--local-evaluation` rejected before invocation.

Validation:

- `cargo fmt --check`
- `cargo test -p a2d --test senior_swe_bench_retry_attempt_evaluate -- --nocapture` (6 passed, including evaluator commands with their own `--` arguments)
- `cargo test` (workspace passed; 126 a2d bin tests, integration tests including retry-attempt evaluate, 160 a2d-core tests with 2 ignored, bootstrap/provider/doc tests)
- `target/debug/a2d fitness-evidence-inspect runs/20260704-senior-swe-bench-retry-attempt-evaluate-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json --require-all-tests-pass`

Fresh source-patch gate evidence:

- `runs/20260704-senior-swe-bench-retry-attempt-evaluate-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`
- `source_diff_hash: 24663b72742e27d26a149b2d7c7330bf195471ad`, matching the current dirty `git diff --binary HEAD -- crates | git hash-object --stdin`

This is deterministic local/official evaluator orchestration plumbing and source-patch evidence. It is not an autonomous retry executor, not retry-step/evidence-inspection completion, and not official Senior SWE-Bench task mastery.
