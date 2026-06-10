# A²D Challenge Examples

Documented runs of the catalytic cycle against real challenges.
Each run records: configuration, per-cycle results, baseline comparison,
artifacts produced, and learnings.

## Historical Score Card (2026-04-04, pre-expanded chess holdouts)

These numbers were produced under the original scoring contracts. Current chess scoring has a stricter 9-case replay contract after the 2026-06-08 acceptance expansion, so compare new chess artifacts with `a2d score-artifact chess <path>` rather than mixing them with this historical table.

| System | Sudoku | Chess | Invocations |
|--------|--------|-------|-------------|
| Gemini 3 Pro one-shot | **100%** (7/7) | **100%** (9/9) | 1 |
| Codex gpt-5.4 one-shot | **100%** (6/6) | **100%** (11/11) | 1 |
| A²D + Codex coder | 83% (5/6) | 50% (4/8) | 3-128 |
| A²D + Kimi k2.5 coder | 83% (5/6) | pending | 3+ |

The cycle currently makes things worse, not better. The bottleneck is
in the cycle itself, not the coder model (Kimi matches Codex at 83%).

## Runs

| Date | Challenge | Cycles | Best Fitness | Baseline | Notes |
|------|-----------|--------|-------------|----------|-------|
| 2026-04-02 | [Sudoku Solver](runs/2026-04-02-sudoku-solver.md) | 2 | 83% (5/6) | 100% (Gemini 3) | Honest with 6 acceptance tests |
| 2026-04-04 | Chess Engine | 1 | 50% (4/8) | 100% (Gemini 3) | No acceptance tests at time of run |
| 2026-04-04 | Sudoku (Kimi) | 3 | 83% (5/6) | 100% (Gemini 3) | Non-frontier model matches frontier |
| 2026-06-10 | [Chess Artifact Replay](runs/2026-06-10-chess-artifact-replay.md) | replay | 33% (3/9) / 22% (2/9) | n/a | Existing saved artifacts fail the current expanded chess contract; replay gate exits 2 |

## What "Better" Means

The system is better than a single model when:

1. **It does what it's supposed to do** — the artifact works, verified by hidden acceptance tests the coder never sees
2. **More reliably** — higher first-pass success rate across diverse tasks
3. **With fewer iterations** — reaches working output in fewer cycles
4. **Needing less outside feedback** — zero human corrections needed

The baseline is always a single-model one-shot on the same task with the same evaluation.

## Key Finding

The 83% ceiling is model-independent. Swapping Codex (frontier) for Kimi k2.5
(non-frontier) produces identical fitness. The bottleneck is the cycle
architecture: prompt routing, artifact flow, evolver feedback, or how
acceptance test failures feed back to the coder.
