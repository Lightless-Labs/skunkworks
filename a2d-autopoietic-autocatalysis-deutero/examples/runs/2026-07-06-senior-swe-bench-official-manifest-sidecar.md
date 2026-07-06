# Senior SWE-Bench Official Manifest Sidecar Binding

**Date:** 2026-07-06
**Status:** source-patch evidence passed; not official Senior SWE-Bench mastery

## Lineage

Prior official-manifest slices added a read-only `senior-swe-bench-official-evaluator-manifest-inspect` preflight and then made `senior-swe-bench-retry-execute` require a canonical inspection sidecar before official evaluator execution. The remaining gap was uneven propagation: direct `senior-swe-bench-evaluate`, retry-attempt planning, resume planning, and next-gate planning could carry official manifest metadata without making the inspected sidecar provenance mandatory in the exported evidence path.

This continues the existing Stage 2 evidence-gating line: official benchmark claims must be based on inspected manifest provenance and still require `a2d.fitness-evidence.v1`; local-wrapper or Sudoku score-artifact evidence must not be described as official Senior SWE-Bench mastery.

## Change

- `senior-swe-bench-evaluate --official-evaluator-manifest <json>` now fails closed unless `--official-evaluator-manifest-inspection <json>` is also supplied.
- Supplying an inspection sidecar without an official manifest is rejected.
- The inspection sidecar is parsed and revalidated against the current task, manifest path/hash, exact evaluator argv, and canonical no-side-effect fields before evaluator execution.
- Retry-attempt planning, resume-attempt planning, retry execution, and next-gate resume planning propagate `--official-evaluator-manifest-inspection` through `evaluate_args`.
- `a2d.senior-swe-bench-local-evaluation.v1` and exported `a2d.fitness-evidence.v1` official provenance now include:
  - `official_evaluator_manifest_inspection_path`
  - `official_evaluator_manifest_inspection_hash`
  - `official_evaluator_manifest_inspection_validated: true`
- The exported evidence validator rejects `evaluator_kind: official_senior_swe_bench` unless those inspection fields are present and valid.
- Retry-attempt planning now validates the sidecar before emitting evaluator args, while tolerating equivalent manifest paths rather than exact path strings after explicit path-equivalence validation.

## Validation

```bash
cargo fmt --check
cargo test -p a2d --test senior_swe_bench_retry_attempt_plan -- --nocapture
cargo test -p a2d --test senior_swe_bench_retry_execute -- --nocapture
cargo test -p a2d senior_swe_bench_evaluate -- --nocapture
cargo test

A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260706-official-manifest-sidecar-evidence/actual-test-score-artifact \
  cargo run -q -p a2d -- score-artifact sudoku runs/20260701-score-artifact-fitness-evidence/good-sudoku-artifact.rs

cargo run -q -p a2d -- fitness-evidence-inspect \
  runs/20260706-official-manifest-sidecar-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json \
  --require-all-tests-pass
```

Reviewer found no critical blockers. Follow-up review warnings about exact sidecar path comparison and late plan validation were addressed by removing the exact path field from canonical value comparison after `paths_equivalent` succeeds and by validating retry-attempt sidecars before emitting `evaluate_args`.

## Evidence

Fresh source-patch gate: `runs/20260706-official-manifest-sidecar-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`.

Summary:

- `schema_version: a2d.fitness-evidence.v1`
- `actual_tests_evaluated: true`
- `non_regressing: true`
- `fitness: 1.0`
- `failed_cases: []`
- `hidden_acceptance: not_present` for this Sudoku score-artifact source-patch gate
- `source_diff_scope: crates`
- `source_diff_hash: f2edde1c687892a0cb1fa50c41e114944eb29b70`, matching the scoped crates diff before commit

## Interpretation

This is official-manifest sidecar/provenance hardening for Senior SWE-Bench orchestration. It is not proof of official Senior SWE-Bench task success, not a public-solution-search network-isolation proof, and not top-level A²D completion.
