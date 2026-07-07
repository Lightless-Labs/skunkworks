# Senior SWE-Bench retry fitness-evidence path hardening

**Date:** 2026-07-07

## Purpose

Keep retry/local-evaluation `fitness_evidence_path` handoffs portable and CWD-stable without leaking host-local project paths for in-project artifacts.

Prior retry path normalization made status/next-cycle handoffs project-relative, but retry-attempt evaluation still serialized exported in-project fitness evidence with `Path::to_string_lossy()`, producing host-local absolute project paths in persisted local-evaluation JSON. Review also found the opposite edge: if `A2D_FITNESS_EVIDENCE_EXPORT_DIR` is a relative directory outside the repo, serializing the original relative path would later resolve against the wrong CWD.

## Change

- `senior-swe-bench-evaluate` now serializes exported `fitness_evidence_path` through retry artifact path semantics:
  - in-project evidence paths become repo-relative strings;
  - outside-project evidence paths become absolute, CWD-stable strings.
- Retry attempt step-evidence and retry run-result consumers resolve `fitness_evidence_path` through the same retry artifact resolver before spawning `fitness-evidence-inspect` or reading evidence bytes.
- Regression coverage proves:
  - in-project retry-attempt evaluation evidence under `target/...` is persisted repo-relative;
  - a relative export dir from an external/non-repo CWD persists an absolute path under the external retry work dir;
  - retry status tamper tests resolve final evidence paths from the project root instead of assuming caller CWD.

## Validation

```bash
cargo fmt --check
CARGO_BUILD_JOBS=2 cargo test -p a2d --test senior_swe_bench_retry_attempt_evaluate -- --nocapture
CARGO_BUILD_JOBS=2 cargo test -p a2d --test senior_swe_bench_retry_execute -- --nocapture
CARGO_BUILD_JOBS=2 cargo test
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260707-retry-fitness-evidence-path-hardening-evidence/actual-test-score-artifact \
  cargo run -q -p a2d -- score-artifact sudoku runs/20260701-score-artifact-fitness-evidence/good-sudoku-artifact.rs
cargo run -q -p a2d -- fitness-evidence-inspect \
  runs/20260707-retry-fitness-evidence-path-hardening-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json \
  --require-all-tests-pass
```

Fresh source-patch gate evidence:

- `runs/20260707-retry-fitness-evidence-path-hardening-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`
- `source_diff_hash: 6eac6834ac85a53a12842a232c0fb35b7f00bb89`, matching `git diff --binary HEAD -- crates | git hash-object --stdin`
- `actual_tests_evaluated: true`, `non_regressing: true`, `all_tests_pass: true`, `failed_cases: []`
- `hidden_acceptance: not_present` for this Sudoku score-artifact source-patch gate

This is retry path portability/source-patch hardening. It is not official Senior SWE-Bench mastery, not hidden-holdout Senior SWE-Bench evidence, and not OS/network no-egress enforcement.
