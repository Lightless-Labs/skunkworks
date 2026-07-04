# Challenge Acceptance Test Expansion

**Created:** 2026-06-08
**Completed:** 2026-06-08

## Problem

The handoff identified weak non-sudoku acceptance coverage: chess needed castling, en-passant, checkmate/legal-move invariants, and Rubik's needed scramble/solve roundtrip coverage. Weak holdout suites let challenge outputs score well while missing core behavior.

## Scope

- Strengthen `chess` hidden acceptance tests in the challenge catalog (`crates/a2d-cli/src/challenges.rs` after the 2026-07-04 core-boundary cleanup).
- Add a concrete Rubik's hidden acceptance suite in the challenge catalog (`crates/a2d-cli/src/challenges.rs` after the 2026-07-04 core-boundary cleanup).
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

## Live smoke follow-up

**2026-06-09:** Ran bounded seed chess smoke after expanding holdouts:

```bash
A2D_GERMLINE=seed \
A2D_PROVIDER_TIMEOUT_SECS=180 \
A2D_MAX_CYCLE_SECS=300 \
cargo run -p a2d -- challenge chess 1
```

Result: inconclusive for acceptance quality. The seed coder invocation timed out before producing code, so none of the expanded chess holdouts executed. Log: `/tmp/a2d-chess1-expanded-acceptance-20260609221901.log`.

A trace-only 1s probe confirmed the coder portfolio mechanics were correct: both Kimi k2p6 and DeepSeek v4 flash launched in parallel for `coder`; both timed out under the artificial 1s bound. Log: `/tmp/a2d-chess1-expanded-acceptance-trace-20260609222406.log`.

Next validation should avoid spending more provider windows on the same uncontrolled one-cycle smoke. Prefer either a provider-policy comparison that isolates DeepSeek as coder, a longer controlled chess run, or a replay fixture that injects candidate chess code to exercise the hidden holdouts mechanically.
