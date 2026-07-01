# Score-Artifact Fitness Evidence Export — 2026-07-01

## Purpose

Extend the same `a2d.fitness-evidence.v1` export contract used by live challenge/comparison runs to `a2d score-artifact`. This closes the baseline-comparison evidence gap: saved or one-shot artifacts can now be replayed through `Challenge::score_artifact()` / `Challenge::scoring_benchmark()` and exported as auditable actual-test evidence instead of only human-readable logs.

Lineage search before this change found that older one-shot baseline references and `sudoku 3` / `sudoku 5` full-fitness claims were mostly log or documentation claims, while the current hard invariant requires structured `a2d.fitness-evidence.v1` evidence. The repeated seed Sudoku challenge run in `runs/20260630-sudoku-repeat-evidence/` provided current A²D challenge evidence, but the one-shot/saved-artifact side still lacked a matching evidence export path.

## Implementation summary

- `a2d score-artifact` now honors `A2D_FITNESS_EVIDENCE_EXPORT_DIR` / `A2D_FITNESS_EVIDENCE_DIR`.
- Exports are named `baseline-<challenge>-cycle-0-fitness-evidence.json`.
- The exporter reuses the canonical `fitness_evidence_artifact()` shape and the same fail-closed validation used by challenge/comparison exports.
- Export happens before the existing nonzero exit for partial-fitness artifacts, so failed replays still leave auditable evidence.

## TDD validation

Focused regression:

```bash
cargo test -p a2d --test score_artifact score_artifact_exports_fitness_evidence_before_nonzero_exit -- --nocapture
```

The test first failed because `score-artifact` did not print/export fitness evidence, then passed after the export path was added. Full score-artifact integration coverage also passed:

```bash
cargo test -p a2d --test score_artifact -- --nocapture
```

## Fresh evidence commands

Baseline/saved-artifact replay:

```bash
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260701-score-artifact-fitness-evidence/baseline-good \
cargo run -p a2d -- score-artifact sudoku \
  runs/20260701-score-artifact-fitness-evidence/good-sudoku-artifact.rs \
  2>&1 | tee /tmp/a2d-score-artifact-good-sudoku-evidence-20260701.log
```

Source-patch challenge smoke:

```bash
A2D_GERMLINE=seed \
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260701-score-artifact-fitness-evidence/challenge-smoke \
A2D_PROVIDER_TIMEOUT_SECS=90 \
A2D_MAX_CYCLE_SECS=120 \
cargo run -p a2d -- challenge sudoku 1 \
  2>&1 | tee /tmp/a2d-score-artifact-change-challenge-smoke-20260701.log
```

## Evidence artifacts

| Artifact | Purpose | Fitness | `all_tests_pass` | SHA-256 |
|---|---|---:|---|---|
| `runs/20260701-score-artifact-fitness-evidence/baseline-good/baseline-sudoku-solver-cycle-0-fitness-evidence.json` | Saved-artifact/baseline replay evidence through the new export path | 100% (6/6) | true | `d3aa7557a7e62146d005bf883f42fafa3dafb609c20f754387949795803b07ad` |
| `runs/20260701-score-artifact-fitness-evidence/challenge-smoke/sudoku-solver-cycle-0-fitness-evidence.json` | Fresh challenge actual-test evidence after the source patch | 100% (6/6) | true | `6aa4f715aaa5dd155371519737ff569c3deb0233a01a18cc263e9ec0e2c62abe` |

Both artifacts validate as:

- `schema_version: a2d.fitness-evidence.v1`
- `actual_tests_evaluated: true`
- `cycle: 0`
- `non_regressing: true`
- `fitness: 1.0`
- `failed: 0`
- `all_tests_pass: true`
- no failed cases

## Interpretation

The baseline replay artifact is useful for comparing saved/one-shot solver behavior against A²D challenge behavior on the same hidden-holdout scoring helper. The source patch itself is backed by the fresh challenge-smoke evidence plus the focused/full test suite; the hand-written saved artifact is not treated as a substitute for source-patch fitness evidence.
