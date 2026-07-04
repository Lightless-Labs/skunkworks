# Chess Artifact Replay Against Expanded Holdouts — 2026-06-10

## Purpose

Use the new `a2d score-artifact` replay command to validate existing saved chess artifacts against the current `chess` challenge without spending another live provider window.

This isolates artifact quality from provider latency. It is not evidence about current provider ability to generate chess code; it is evidence that older saved chess artifacts do not satisfy the current API/holdout contract.

## Command

```bash
cargo run -q -p a2d -- score-artifact chess chess_engine_single.rs
cargo run -q -p a2d -- score-artifact chess chess_engine_temp.rs
```

Artifacts:

- stdout: `/tmp/a2d-score-artifact-chess-single-20260610.out`
- stderr: `/tmp/a2d-score-artifact-chess-single-20260610.err`
- stdout: `/tmp/a2d-score-artifact-chess-temp-20260610.out`
- stderr: `/tmp/a2d-score-artifact-chess-temp-20260610.err`

## Results

| Artifact | Exit | Fitness | Notes |
|----------|------|---------|-------|
| `chess_engine_single.rs` | 2 | 33% (3/9) | Did not compile under the current challenge acceptance harness; only visible `Board`, `Move`, and `apply_move` string checks passed. |
| `chess_engine_temp.rs` | 2 | 22% (2/9) | Did not compile under the current challenge acceptance harness; only visible `Board` and `apply_move` string checks passed. |

`score-artifact` exits 2 for any non-perfect fitness. That nonzero exit is the intended gate behavior, not a harness failure.

## Interpretation

The existing root-level chess artifacts predate the 2026-06-08 API/acceptance expansion and are not useful as current chess baselines. They fail before hidden behavioral quality can be assessed because the current harness cannot compile them against the expected public API.

Next useful validation remains one of:

1. replay a newly generated chess candidate that targets the current challenge contract (`crates/a2d-cli/src/challenges.rs` after the 2026-07-04 core-boundary cleanup); or
2. run a bounded live chess challenge and then replay any captured candidate artifact with `a2d score-artifact chess <path>`.

## Learning

Replay prevents wasting provider time on stale artifacts. It also distinguishes two different failures that the previous live smoke conflated:

- **provider latency/no output** — no artifact to score;
- **artifact incompatibility/quality** — artifact exists but fails the current mechanical holdouts.
