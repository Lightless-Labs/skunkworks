# Challenge Acceptance Test Expansion

**Created:** 2026-06-08
**Completed:** 2026-06-08

## Problem

The handoff identified weak non-sudoku acceptance coverage: chess needed castling, en-passant, checkmate/legal-move invariants, and Rubik's needed scramble/solve roundtrip coverage. Weak holdout suites let challenge outputs score well while missing core behavior.

## Scope

- Strengthen `chess` hidden acceptance tests in `crates/a2d-core/src/challenges.rs`.
- Add a concrete Rubik's hidden acceptance suite in `crates/a2d-core/src/challenges.rs`.
- Tighten challenge requirement strings where hidden tests require an explicit callable API.
- Add lightweight unit coverage that the challenge definitions retain the intended hidden acceptance dimensions.

## Implementation

- Chess requirements now specify the public `Board`, `Move`, `piece_at`, `generate_moves`, `apply_move`, and `is_check` surface needed by holdout tests.
- Chess hidden tests now cover:
  - generated moves do not leave the moving king in check,
  - kingside castling generation and rook relocation after path clearance,
  - immediate en-passant generation and captured-pawn removal,
  - Fool's mate checkmate as checked side with zero legal moves.
- Rubik's requirements now specify `Cube`, `Move`, `rotate`, `is_solved`, deterministic `scramble`, and `solve` returning moves.
- Rubik's hidden tests now cover:
  - solved initialization,
  - every rotation changes solved state and inverse restores,
  - quarter-turn order four,
  - known sequence inverse roundtrip,
  - solver solves known scrambles,
  - seeded scramble replayability and solve roundtrip.

## Validation

- `cargo test -p a2d-core challenges::tests -- --nocapture`
- `cargo test`

Both passed on 2026-06-08.
