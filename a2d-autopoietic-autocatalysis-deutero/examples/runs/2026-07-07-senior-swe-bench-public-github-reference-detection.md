# Senior SWE-Bench Public GitHub Reference Detection Hardening

**Date:** 2026-07-07
**Scope:** Artifact diagnosis/selection no-public-solution-reference hardening; not official Senior SWE-Bench mastery.

## Lineage

Senior SWE-Bench coding agents must not search or copy public GitHub issues, pull requests, commits, forks, refs, or solution writeups. Existing extraction, diagnosis, candidate selection, feedback, and retry gates rejected obvious `https://github.com/.../pull/...` or `/commit/...` references, and the retry/controller work preserved no-search metadata. A remaining artifact-safety gap was narrower: provider artifacts can cite public GitHub by SSH remote form (`git@github.com:org/repo.git`) or raw pull refs (`refs/pull/123/head`) without containing `github.com/`, `/pull/`, or `/commit/`.

This slice stays outside `a2d-core` and changes only CLI/evaluation-layer Senior SWE-Bench artifact filtering.

## Change

`contains_public_github_solution_reference` now treats these as public solution-reference indicators:

- any `github.com` occurrence, including `git@github.com:...` remotes;
- `/pull/`;
- `/commit/`;
- `/issues/`;
- `refs/pull`.

Regression coverage was extended in:

- `crates/a2d-cli/tests/senior_swe_bench_diagnose_artifact.rs`
- `crates/a2d-cli/tests/senior_swe_bench_select_candidate_artifact.rs`

The tests prove diagnostic previews redact these references and candidate-artifact selection rejects valid-looking diffs that append these public GitHub references before extraction/evaluation can proceed.

## TDD Baseline

Before restoring the production detector widening, the new fixtures were run against the old implementation. Both failed as expected:

- `diagnose_artifact_redacts_mixed_case_public_github_references` observed `contains_public_github_solution_reference == false` for newly added `git@github.com` / `refs/pull` fixtures.
- `senior_swe_bench_select_candidate_artifact_fails_closed_on_unsafe_manifests` observed exit `0` instead of fail-closed exit `1` for newly added public-reference artifact variants.

Sanitized baseline notes: `runs/20260707-senior-swe-bench-public-github-reference-detection-evidence/tdd-baseline-failures.txt`.

## Validation

```bash
cargo fmt --check
CARGO_BUILD_JOBS=2 cargo test -p a2d --test senior_swe_bench_diagnose_artifact diagnose_artifact_redacts_mixed_case_public_github_references -- --nocapture
CARGO_BUILD_JOBS=2 cargo test -p a2d --test senior_swe_bench_select_candidate_artifact senior_swe_bench_select_candidate_artifact_fails_closed_on_unsafe_manifests -- --nocapture
CARGO_BUILD_JOBS=2 cargo test -p a2d --test senior_swe_bench_diagnose_artifact -- --nocapture
CARGO_BUILD_JOBS=2 cargo test -p a2d --test senior_swe_bench_select_candidate_artifact -- --nocapture
CARGO_BUILD_JOBS=2 cargo test
```

Fresh source-patch evidence:

```bash
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260707-senior-swe-bench-public-github-reference-detection-evidence/actual-test-score-artifact \
  cargo run -q -p a2d -- score-artifact sudoku runs/20260701-score-artifact-fitness-evidence/good-sudoku-artifact.rs

cargo run -q -p a2d -- fitness-evidence-inspect \
  runs/20260707-senior-swe-bench-public-github-reference-detection-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json \
  --require-all-tests-pass
```

Evidence summary: `a2d.fitness-evidence.v1`, `actual_tests_evaluated: true`, `non_regressing: true`, `fitness: 1.0`, `passed: 6`, `total: 6`, `failed_cases: []`, aggregate `all_tests_pass: true`, `hidden_acceptance: not_present`, `source_diff_scope: crates`, `source_diff_hash: 195c9ca41f615e64a8a8fedbe486183b8a48ddca`, matching the scoped crates diff.

`hidden_acceptance: not_present` is expected here because this is a local source-patch gate using the Sudoku `score-artifact` acceptance tests to prove the A²D source change is non-regressing. It is not official Senior SWE-Bench hidden-holdout evidence.

## Interpretation

This is artifact safety hardening for Senior SWE-Bench no-public-solution-search policy. It is not an OS/network no-egress proof, not official Senior SWE-Bench mastery, and not live provider-loop success evidence.
