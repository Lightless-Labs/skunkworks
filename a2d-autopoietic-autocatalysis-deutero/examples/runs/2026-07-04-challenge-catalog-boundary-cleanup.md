# Challenge Catalog Boundary Cleanup — 2026-07-04

## Purpose

Move built-in domain benchmark catalogs (`sudoku`, `chess`, `rubiks`) out of `a2d-core` so core exposes generic benchmark/sandbox/evidence/gating primitives rather than task-specific hidden-holdout definitions.

## Lineage constraints

- Senior SWE-Bench integration already established the boundary: benchmark-specific adapters belong in the CLI/evaluation layer, not core.
- Foundry-style hidden holdouts are behavioral oracles; automated `SystemPatch` edits must not be able to weaken those oracles.
- Prior CLI-provider isolation bug showed direct challenge-catalog mutation is dangerous even when the cycle reports no accepted patch.

## Change

- Moved `crates/a2d-core/src/challenges.rs` to `crates/a2d-cli/src/challenges.rs`.
- Removed `pub mod challenges` from `a2d-core`.
- Kept CLI challenge commands (`challenge`, `score-artifact`, topology/provider-policy comparisons) wired to the moved local module.
- Updated self-sandbox allowlist so neither the old core path nor the new CLI challenge catalog is automated-modifiable.
- Added/kept regression checks that `a2d-core` contains no Senior SWE-Bench adapter text or built-in domain challenge catalog terms.

## Validation

Focused checks:

```bash
cargo fmt --check
cargo test -p a2d-core challenge_catalogs_are_not_core_modifiable_surface -- --nocapture
cargo test -p a2d challenges:: -- --nocapture
```

Full suite:

```bash
cargo test
```

Result: 293 passed, 2 ignored.

Boundary checks:

```bash
rg -n "pub mod challenges|a2d_core::challenges|sudoku_solver|rubiks_cube|chess_engine|sudoku-solver|rubiks-cube|chess-engine|senior_swe_bench|Senior SWE-Bench|senior-swe-bench" crates/a2d-core
```

No matches.

## Fresh fitness evidence

Command:

```bash
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260704-challenge-catalog-boundary-evidence/actual-test-score-artifact \
cargo run -q -p a2d -- score-artifact sudoku runs/20260701-score-artifact-fitness-evidence/good-sudoku-artifact.rs
```

Artifact: `runs/20260704-challenge-catalog-boundary-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`

Evidence fields inspected:

- `schema_version: a2d.fitness-evidence.v1`
- `actual_tests_evaluated: true`
- `non_regressing: true`
- `fitness: 1.0` (6/6)
- `failed_cases: []`
- result labels include `all_tests_pass`
- `source_diff_scope: crates`
- `source_diff_hash: da45da61907809b691d825410e4d0fdf3b0a6f67`

This gates the source-boundary refactor and hidden-holdout replay path. It is not a new benchmark-mastery claim.
