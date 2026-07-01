# Fitness Evidence Source Provenance — 2026-07-01

## Purpose

Bind exported `a2d.fitness-evidence.v1` records to the source snapshot that produced them. Previous exports proved actual-test execution, non-regression, and hidden-holdout aggregate status, but not the dirty source diff under test.

This slice adds source provenance fields to exported evidence:

- `source_revision` — Git tree revision for the A²D `crates` source scope at `HEAD`
- `source_tree_dirty` — whether that source scope has uncommitted changes
- `source_diff_scope` — currently `crates`
- `source_diff_hash` — `git diff --binary HEAD -- :(top)a2d-autopoietic-autocatalysis-deutero/crates | git hash-object --stdin`
- `evidence_command` — CLI command that produced the evidence

The source provenance intentionally binds to the tested source snapshot (`HEAD` source tree plus source diff hash), not to the eventual commit SHA. Run/docs artifacts are outside `source_diff_scope`; they document the evidence and are not included in the source diff hash.

## Validation

Focused tests:

```bash
cargo test -p a2d --test score_artifact score_artifact_export_provenance_works_from_crate_subdirectory -- --nocapture
cargo test -p a2d --test score_artifact score_artifact_export_provenance_works_from_parent_repo_root -- --nocapture
cargo test -p a2d --test score_artifact -- --nocapture
cargo test -p a2d fitness_evidence -- --nocapture
```

Full suite:

```bash
cargo test
```

Result: 257 passed, 2 ignored.

The nested-cwd regression test failed before the repo-root pathspec fix, proving the provenance code no longer silently hashes an empty/incorrect scope when evidence export is invoked from `crates/a2d-cli`. A parent-repo-root regression covers invocation from the monorepo/skunkworks level. A follow-up unit regression rejects untracked source files under `crates` so dirty-source evidence cannot validate with untracked contents outside the diff hash.

## Fresh evidence commands

Saved-artifact replay support evidence:

```bash
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260701-fitness-evidence-provenance/baseline-good \
cargo run -p a2d -- score-artifact sudoku \
  runs/20260701-score-artifact-fitness-evidence/good-sudoku-artifact.rs
```

Source-patch challenge smoke:

```bash
A2D_GERMLINE=seed \
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260701-fitness-evidence-provenance/challenge-smoke \
A2D_PROVIDER_TIMEOUT_SECS=90 \
A2D_MAX_CYCLE_SECS=120 \
cargo run -p a2d -- challenge sudoku 1
```

## Evidence artifacts

| Artifact | Purpose | Fitness | `all_tests_pass` | Source revision | Source diff hash |
|---|---|---:|---|---|---|
| `runs/20260701-fitness-evidence-provenance/challenge-smoke/sudoku-solver-cycle-0-fitness-evidence.json` | Fresh challenge export/provenance smoke for the source patch | 100% (6/6) | true | `ecdc3dc` | `db406660a8259a29169a6d72be4af2c62418703c` |
| `runs/20260701-fitness-evidence-provenance/baseline-good/baseline-sudoku-solver-cycle-0-fitness-evidence.json` | Saved-artifact replay support evidence through the new provenance export path | 100% (6/6) | true | `ecdc3dc` | `db406660a8259a29169a6d72be4af2c62418703c` |

Both artifacts validate as `a2d.fitness-evidence.v1`, `actual_tests_evaluated: true`, `non_regressing: true`, and `all_tests_pass: true`, with no failed cases. The challenge smoke is full-passing fresh source-patch gating evidence for provenance/export hardening; it is still only a one-cycle smoke, not evidence of repeated benchmark-reliability improvement.

## Interpretation

This is plumbing/provenance evidence, not a new claim that A²D has improved benchmark reliability. The source patch hardens exported evidence so future durable self-improvement decisions can reject stale or forged source provenance before accepting evidence.
