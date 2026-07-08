# Fitness Evidence Full Source Revision Provenance — 2026-07-08

## Context

Prior source-bound `a2d.fitness-evidence.v1` artifacts recorded full `source_diff_hash` values but short `source_revision` values from `git rev-parse --short HEAD:<crates>`. That was sufficient for display, but weaker than the evidence gate's source-binding intent.

## Change

- `source_revision` is now emitted as the full scoped git object ID for `HEAD:<crates>`.
- Exported fitness evidence, autopilot source-fitness evidence, and Senior SWE-Bench local-evaluation provenance reject malformed/short revisions before comparing against the current scoped revision.
- Fixture helpers now synthesize full revisions, and score-artifact provenance asserts a 40-hex revision.

## Validation

Commands run:

```bash
cargo fmt --check
CARGO_BUILD_JOBS=2 cargo test -p a2d exported_fitness_evidence_validation_requires_source_provenance -- --nocapture
CARGO_BUILD_JOBS=2 cargo test -p a2d --test score_artifact -- --nocapture
CARGO_BUILD_JOBS=2 cargo test
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260708-source-revision-full-hash-evidence/actual-test-score-artifact \
  cargo run -q -p a2d -- score-artifact sudoku runs/20260701-score-artifact-fitness-evidence/good-sudoku-artifact.rs
cargo run -q -p a2d -- fitness-evidence-inspect \
  runs/20260708-source-revision-full-hash-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json \
  --require-all-tests-pass
```

Fresh evidence:

- `runs/20260708-source-revision-full-hash-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`
- `source_revision: 46f2eda9d75fd9736035b16d1ff8a6dc31c2edb4` (40 hex chars)
- `source_diff_hash: 8bd947344234a680de3d3032c28ab90f40e632d0`, matching the final scoped `crates` diff
- `fitness: 1.0`, `passed: 6`, `failed: 0`, `all_tests_pass: true`

This is provenance hardening for future evidence-gated self-improvement; it is not official Senior SWE-Bench mastery, hidden official holdout proof, or OS/network no-egress proof.
