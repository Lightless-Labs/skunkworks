# 2026-07-05 — No-search policy evidence label scope

## Scope

Rename Senior SWE-Bench local-wrapper fitness result label from ambiguous `has_no_solution_search` to `has_no_solution_search_policy_declared`.

The new label records that A²D accepted no-public-solution-search task/manifest policy and propagated policy metadata. It does **not** claim OS/network isolation or prove no egress.

## Validation

- `cargo fmt --check`
- `cargo test -p a2d senior_swe_bench_local_fitness_evidence_contains_holdout_and_policy_status -- --nocapture`
- `cargo test -p a2d failed_senior_swe_bench_local_evaluator_is_not_non_regressing_evidence -- --nocapture`
- `cargo test -p a2d senior_swe_bench -- --nocapture`
- `cargo test`
- `target/debug/a2d fitness-evidence-inspect runs/20260705-no-search-policy-label-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json --require-all-tests-pass`

## Evidence

- Source-patch gate evidence: `runs/20260705-no-search-policy-label-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`
- Validation summary: `runs/20260705-no-search-policy-label-evidence/validation-summary.json`
- Staged crates diff hash / evidence `source_diff_hash`: `fe6b1a58ad238ec217bf7169c28964d337c2d1d7`

The focused tests prove successful and failed Senior SWE-Bench local evaluator reports emit the scoped policy label and that failed evaluator output remains regressing/non-acceptable. The fresh `a2d.fitness-evidence.v1` artifact is source-patch gating evidence for the crates diff.
