# Sudoku Test-Prompt Hardening — 2026-07-04

## Purpose

Tighten the seed coder prompt around the public `has_tests` fitness gate. Prior repeated Sudoku evidence had one bounded seed replica fail `has_tests` despite compiling and exposing the required public functions (`runs/20260630-sudoku-repeat-evidence/r1/sudoku-solver-cycle-0-fitness-evidence.json`). That made the reliability gap partly a prompt-compliance failure rather than a solver-only failure.

## Change

The Rust-source coder contract now names the exact required test module header and the consequence of omission:

- include `#[cfg(test)] mod tests` with at least three test cases;
- omitting it fails the mechanical `has_tests` fitness gate;
- include normal-path, edge/invalid-input, and end-to-end behavior tests.

Unified-diff/Senior-SWE-Bench artifact behavior is unchanged.

## Validation

Focused prompt coverage:

```bash
cargo fmt --check
cargo test -p a2d seed_germline_coder_consumes_design_plan_and_requirements -- --nocapture
```

Full suite:

```bash
cargo test
```

Result: 295 passed, 2 ignored.

## Fresh fitness evidence

Run directory: `runs/20260704-sudoku-test-prompt-evidence/`.

### Live generated seed challenge

Command:

```bash
A2D_GERMLINE=seed \
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260704-sudoku-test-prompt-evidence/challenge-seed-r1 \
A2D_PROVIDER_TIMEOUT_SECS=90 \
A2D_MAX_CYCLE_SECS=120 \
cargo run -q -p a2d -- challenge sudoku 1
```

Evidence: `runs/20260704-sudoku-test-prompt-evidence/challenge-seed-r1/sudoku-solver-cycle-0-fitness-evidence.json`

Inspected fields:

- `schema_version: a2d.fitness-evidence.v1`
- `actual_tests_evaluated: true`
- `non_regressing: true`
- `fitness: 0.8333333333333334` (5/6)
- `failed_cases: ["all_tests_pass"]`
- `has_tests: true`
- `source_diff_hash: d44224bb8edd9d79608bb2b2646b00867f4cf4f1`

This proves the generated solution satisfied the public test-module gate under the hardened prompt, but it did not solve all Sudoku hidden/acceptance behavior. Do not claim Sudoku mastery from this run.

### Source-patch replay gate

Command:

```bash
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260704-sudoku-test-prompt-evidence/actual-test-score-artifact \
cargo run -q -p a2d -- score-artifact sudoku runs/20260701-score-artifact-fitness-evidence/good-sudoku-artifact.rs
```

Evidence: `runs/20260704-sudoku-test-prompt-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`

Inspected fields:

- `schema_version: a2d.fitness-evidence.v1`
- `actual_tests_evaluated: true`
- `non_regressing: true`
- `fitness: 1.0` (6/6)
- `failed_cases: []`
- result labels include `has_tests` and `all_tests_pass`
- `source_diff_hash: d44224bb8edd9d79608bb2b2646b00867f4cf4f1`

This gates the prompt source patch with full-passing hidden-holdout replay evidence and a live generated-solution check of the specific `has_tests` compliance gap.
