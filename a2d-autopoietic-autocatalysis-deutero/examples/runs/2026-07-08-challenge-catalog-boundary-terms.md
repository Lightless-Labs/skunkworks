# Challenge Catalog Boundary Term Hardening — 2026-07-08

## Purpose

Extend the 2026-07-04 challenge-catalog core-boundary cleanup so `a2d-core` is checked not only for broad challenge names but also for concrete built-in hidden-oracle identifiers from Sudoku, Chess, and Rubik's.

## Lineage constraints

- `crates/a2d-cli/src/challenges.rs` owns domain benchmark catalogs; `a2d-core` owns generic benchmark/sandbox/evidence mechanics.
- Hidden holdouts are behavioral backstops and must not become automated-modifiable or core-coupled mechanics.
- Prior review of the initial boundary cleanup required keeping challenge catalogs outside the self-sandbox allowlist.
- The current reviewer warned that hard-coded sentinels can go stale; the test now documents that the list must be updated with challenge-catalog oracle changes.

## TDD baseline

The baseline inventory is stored at:

- `runs/20260708-challenge-catalog-boundary-terms-evidence/tdd-baseline/missing-boundary-terms.txt`

It showed missing sentinel coverage for medium/hard Sudoku, empty-board Sudoku, chess castling/en-passant/Fool's mate, and Rubik's inverse/solver oracle names.

## Change

- Expanded `a2d_core_does_not_contain_domain_challenge_catalog_code` in `crates/a2d-cli/src/main.rs` with concrete domain acceptance identifiers.
- Added a comment tying the sentinel list to `crates/a2d-cli/src/challenges.rs` maintenance.
- Kept generic `a2d_acceptance` out of the forbidden list after it false-positived on legitimate core mechanics.
- Replaced a chess-specific redaction fixture in `crates/a2d-core/src/metabolism.rs` with synthetic `private_acceptance_case_42`, preserving generic hidden-case redaction coverage without leaking a domain oracle name into core tests.

## Validation

Focused checks:

```bash
cargo fmt --check
CARGO_BUILD_JOBS=2 cargo test -p a2d a2d_core_does_not_contain_domain_challenge_catalog_code -- --nocapture
CARGO_BUILD_JOBS=2 cargo test -p a2d-core fitness_evidence_redacts_hidden_case_names -- --nocapture
```

Full suite:

```bash
CARGO_BUILD_JOBS=2 cargo test
```

Result: full suite passed.

Reviewer: independent `reviewer` subagent found no blockers. Warning recorded: hard-coded challenge oracle names are safe for this slice but must be kept in sync with catalog changes; generic names may false-positive.

## Fresh fitness evidence

Command:

```bash
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260708-challenge-catalog-boundary-terms-evidence/actual-test-score-artifact \
  cargo run -q -p a2d -- score-artifact sudoku runs/20260701-score-artifact-fitness-evidence/good-sudoku-artifact.rs
cargo run -q -p a2d -- fitness-evidence-inspect \
  runs/20260708-challenge-catalog-boundary-terms-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json \
  --require-all-tests-pass
```

Artifact: `runs/20260708-challenge-catalog-boundary-terms-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`

Inspected fields:

- `schema_version: a2d.fitness-evidence.v1`
- `actual_tests_evaluated: true`
- `non_regressing: true`
- `fitness: 1.0` (6/6)
- `failed_cases: []`
- result labels include `all_tests_pass`
- `source_diff_scope: crates`
- `source_tree_dirty: true`
- `source_diff_hash: 65d232d8f8f405098ddb59fe7f7363a79078c1fe`

The evidence hash matched:

```bash
git diff --binary HEAD -- crates | git hash-object --stdin
```

## Scope

This gates a CLI-only boundary sentinel/source-fixture hardening slice. It is not official Senior SWE-Bench mastery, not hidden official holdout proof, not OS/network no-egress proof, and not repeated autonomous benchmark-solving evidence.
