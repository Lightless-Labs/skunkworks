# Senior SWE-Bench Retry Next-Cycle Pre-Provider Classifier Coverage

**Date:** 2026-07-07
**Scope:** Retry next-cycle classifier unit coverage; not official Senior SWE-Bench mastery.

## Lineage

The previous real-spawn slice proved that an invalid checkout can fail the current-exe `cycle-input` child before provider invocation, and that provider stderr after the `A²D Catalytic Cycle` marker is not masked as pre-provider validation. Scout/reviewer follow-up identified a smaller residual gap: the classifier had one integration path and one provider-phase negative path, but most known pre-provider validation prefixes were not pinned by direct unit tests.

## Change

Added `retry_next_cycle_pre_provider_classifier_covers_all_validation_prefixes` in `crates/a2d-cli/src/main.rs`.

The test covers every currently reviewed pre-provider validation prefix used by `cycle_input_failure_happened_before_provider`:

- empty checkout/context;
- checkout symlink;
- checkout not a directory;
- failed checkout read;
- failed checkout canonicalization;
- reserved runtime artifact input;
- non-object artifact bundle input.

It also asserts negative cases for unrelated stderr, delimiter-sensitive checkout-like text, and checkout-like stderr after stdout has reached the `A²D Catalytic Cycle` phase marker. This keeps provider-start metadata conservative: only known validation failures before the catalytic-cycle phase may report provider invocations as not started.

## Validation

```bash
cargo fmt --check
CARGO_BUILD_JOBS=2 cargo test -p a2d retry_next_cycle_pre_provider_classifier_covers_all_validation_prefixes -- --nocapture
CARGO_BUILD_JOBS=2 cargo test -p a2d retry_run_next_cycle -- --nocapture
CARGO_BUILD_JOBS=2 cargo test -p a2d --test senior_swe_bench_retry_execute -- --nocapture
CARGO_BUILD_JOBS=2 cargo test
```

Fresh source-patch evidence:

```bash
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260707-retry-next-cycle-classifier-evidence/actual-test-score-artifact \
  cargo run -q -p a2d -- score-artifact sudoku runs/20260701-score-artifact-fitness-evidence/good-sudoku-artifact.rs

cargo run -q -p a2d -- fitness-evidence-inspect \
  runs/20260707-retry-next-cycle-classifier-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json \
  --require-all-tests-pass
```

Evidence summary: `a2d.fitness-evidence.v1`, `actual_tests_evaluated: true`, `non_regressing: true`, `fitness: 1.0`, `passed: 6`, `total: 6`, `source_diff_scope: crates`, `source_diff_hash: fae0002cf0d5380e22115f48f3fd7ad020ff6ac4`, matching the scoped crates diff.

## Interpretation

This is classifier test hardening for retry-controller safety metadata. It is not official Senior SWE-Bench mastery, not OS/network no-search enforcement, and not a provider/evaluator run.
