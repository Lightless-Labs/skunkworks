# Autopilot Source Fitness Evidence Gate — 2026-07-03

## Purpose

Close an outer-loop safety gap: `a2d autopilot` source self-modifications under `crates/...` were previously gated by path checks, temp-worktree validation, and `cargo test`, but not by fresh source-bound `a2d.fitness-evidence.v1` actual-test evidence.

## Change

Autopilot now accepts a source evidence path via either:

```bash
A2D_AUTOPILOT_SOURCE_FITNESS_EVIDENCE=<path>
a2d autopilot --source-fitness-evidence <path>
```

When a `ProjectPatchset` touches eligible source and therefore requires the cargo-test gate, real-tree apply/commit now also requires fresh source-bound fitness evidence. The validator requires:

- `schema_version: a2d.fitness-evidence.v1`
- `actual_tests_evaluated: true`
- `non_regressing: true`
- hidden/aggregate status in `results` (`all_tests_pass` or `hidden_acceptance`)
- `source_revision` matching the current `crates` tree revision
- `source_tree_dirty` matching the current scoped dirty state
- `source_diff_scope: crates`
- `source_diff_hash` matching `git diff -- crates | git hash-object --stdin`
- non-empty `evidence_command`

Without evidence, source autopilot apply fails closed with:

```text
source self-modification requires fresh source-bound a2d.fitness-evidence.v1 actual-test evidence
```

Docs-only autopilot patchsets are unchanged.

## Validation

```bash
cargo fmt --check
cargo test -p a2d autopilot_source -- --nocapture
cargo test
```

Focused coverage proves source patchsets are rejected without evidence, matching source-bound evidence is accepted, stale/mismatched source provenance is rejected, non-actual/regressing evidence is rejected, and apply-report JSON exposes `fitness_evidence_required` / `fitness_evidence_path`.

Full suite after the slice: 288 passed, 2 ignored.

## Fresh actual-test evidence

```bash
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260703-autopilot-source-fitness-evidence/actual-test-score-artifact \
  target/debug/a2d score-artifact sudoku \
  runs/20260701-score-artifact-fitness-evidence/good-sudoku-artifact.rs
```

Artifact:

- `runs/20260703-autopilot-source-fitness-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`

Inspection:

- `schema_version: a2d.fitness-evidence.v1`
- `actual_tests_evaluated: true`
- `non_regressing: true`
- `fitness: 1.0`
- `failed_cases: []`
- result labels include `all_tests_pass`
- `source_diff_scope: crates`
- `source_diff_hash: 5d105fe70f380e1635100c4c663642da7fe614df`, matching `git diff -- crates | git hash-object --stdin`

This gates the autopilot source-evidence commit path. It is not a new benchmark-mastery claim.
