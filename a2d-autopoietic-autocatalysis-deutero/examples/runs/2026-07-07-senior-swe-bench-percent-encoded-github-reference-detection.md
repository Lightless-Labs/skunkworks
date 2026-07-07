# Senior SWE-Bench Percent-Encoded GitHub Reference Detection

**Date:** 2026-07-07
**Scope:** artifact no-public-solution-reference hardening
**Evidence:** `runs/20260707-senior-swe-bench-percent-encoded-github-reference-detection-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`

## Lineage

After URL/raw/obfuscated-host and GitHub CLI command hardening, the shared artifact detector still matched only literal text. A provider artifact could cite public GitHub material through percent-encoded forms such as `github%2ecom/...` or `refs%2fpull%2f123%2fhead` and avoid the literal host/ref checks.

## Change

`contains_public_github_solution_reference` now percent-decodes ASCII `%XX` sequences once and applies the same GitHub host/ref/CLI checks to both original and decoded text. Diagnosis redacts encoded public references, and candidate-artifact selection rejects valid-looking diffs containing them before extraction/evaluation.

## Validation

- TDD baseline failures recorded in `runs/20260707-senior-swe-bench-percent-encoded-github-reference-detection-evidence/tdd-baseline-failures.txt`.
- `cargo fmt --check`
- `CARGO_BUILD_JOBS=2 cargo test -p a2d --test senior_swe_bench_diagnose_artifact -- --nocapture`
- `CARGO_BUILD_JOBS=2 cargo test -p a2d --test senior_swe_bench_select_candidate_artifact -- --nocapture`
- `CARGO_BUILD_JOBS=2 cargo test`
- `cargo run -q -p a2d -- fitness-evidence-inspect runs/20260707-senior-swe-bench-percent-encoded-github-reference-detection-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json --require-all-tests-pass`

The evidence is full-passing `a2d.fitness-evidence.v1` actual-test evidence with `source_diff_hash: 2eb9a3e509b8442569b4fab43c56603bb7b363dd`, matching the scoped crates diff. `hidden_acceptance: not_present` is expected for this local score-artifact source-patch gate.

## Non-claims

This is not official Senior SWE-Bench mastery, not hidden-holdout Senior SWE-Bench evidence, not a network no-egress proof, and not live provider-loop success evidence.
