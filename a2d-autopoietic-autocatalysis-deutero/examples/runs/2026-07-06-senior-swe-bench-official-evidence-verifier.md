# Senior SWE-Bench Official Evidence Verifier Hardening

**Date:** 2026-07-06
**Status:** source-patch evidence passed; not official Senior SWE-Bench mastery

## Lineage

The prior official-manifest sidecar slice made inspected sidecars mandatory before official evaluator execution and required sidecar fields in official `a2d.fitness-evidence.v1` provenance. A remaining verifier gap was that serialized provenance could still be accepted without re-reading the referenced manifest/inspection files, and relative path semantics needed to be CWD-stable and project-root contained.

## Change

- Official Senior SWE-Bench evidence validation now re-reads the referenced official evaluator manifest and inspection sidecar.
- The verifier recomputes their git object hashes and rejects stale or tampered files.
- Relative official evidence paths resolve only under the A²D project root, not the process CWD.
- Lexical `..` traversal, missing project-relative files, symlink/canonical escapes, and absolute paths outside the A²D project root fail closed.
- Official evidence export now serializes in-project manifest/inspection paths through the retry artifact path normalizer.
- Integration coverage runs `fitness-evidence-inspect` from a non-root CWD and proves repo-relative official paths still resolve while an outside absolute manifest path is rejected.
- A sandbox timeout unit test now asserts the recorded test-binary elapsed time rather than compile+test wall time, avoiding unrelated compile-load flakiness without changing production timeout behavior.

## Validation

```bash
cargo fmt --check
CARGO_BUILD_JOBS=2 cargo test -p a2d --test senior_swe_bench_official_evaluator_manifest fitness_evidence_inspect_resolves_official_repo_relative_paths_from_non_root_cwd -- --nocapture --test-threads=1
CARGO_BUILD_JOBS=2 cargo test -p a2d official_evidence_validation_resolves_repo_relative_manifest_files -- --nocapture
CARGO_BUILD_JOBS=2 cargo test -p a2d senior_swe_bench_official_manifest_is_serialized_into_fitness_evidence -- --nocapture
CARGO_BUILD_JOBS=2 cargo test -p a2d senior_swe_bench_exported_fitness_evidence_binds_candidate_patch_hash -- --nocapture
CARGO_BUILD_JOBS=2 cargo test -p a2d --test senior_swe_bench_retry_execute -- --nocapture
CARGO_BUILD_JOBS=2 cargo test

A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260706-official-evidence-verifier-evidence/actual-test-score-artifact \
  cargo run -q -p a2d -- score-artifact sudoku runs/20260701-score-artifact-fitness-evidence/good-sudoku-artifact.rs

cargo run -q -p a2d -- fitness-evidence-inspect \
  runs/20260706-official-evidence-verifier-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json \
  --require-all-tests-pass
```

Reviewer re-review found no blockers after the absolute-path containment fix.

## Evidence

Fresh source-patch gate: `runs/20260706-official-evidence-verifier-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`.

Summary:

- `schema_version: a2d.fitness-evidence.v1`
- `actual_tests_evaluated: true`
- `non_regressing: true`
- `fitness: 1.0`
- `failed_cases: []`
- `source_diff_scope: crates`
- `source_diff_hash: ae8a6b2c6618b331c9b3d711c0560394d668548b`
- `hidden_acceptance: not_present` for this Sudoku score-artifact source-patch gate

## Interpretation

This is official evidence verifier hardening for Senior SWE-Bench orchestration. It is not proof of official Senior SWE-Bench task success, not a public-solution-search network-isolation proof, and not top-level A²D completion.
