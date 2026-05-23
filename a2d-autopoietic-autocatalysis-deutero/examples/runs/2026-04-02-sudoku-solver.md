# Challenge Run: Sudoku Solver

**Date:** 2026-04-02 / 2026-04-03
**Challenge:** sudoku-solver

## Run 1 — Pre-acceptance tests (2026-04-02)

**Cycles:** 2 | **Best fitness:** 100% (6/6) | **Acceptance tests:** none

| Enzyme | Provider | Model |
|--------|----------|-------|
| Coder | Codex CLI | gpt-5.4 (reasoning: high) |
| Tester | Gemini CLI | gemini-2.5-flash |
| Evolver | Gemini CLI | gemini-3.1-pro-preview |

### Cycle 1
```
10 invocations, 9 mutations accepted, 2 rejected
RAF: 100% | Fitness: 100% (6/6) ↑
Lineage committed: 148f288
```

### Cycle 2
```
6 invocations, 0 mutations accepted
RAF: 100% | Fitness: 100% (6/6) →
```

### Verdict

**Misleading.** 100% scored against string checks + sandbox compilation. No
acceptance tests — the system never verified whether `solve()` actually
solves puzzles. This result triggered the expansion to 6 acceptance tests.

---

## Run 2 — With 6 acceptance tests (2026-04-03)

**Cycles:** 1 (cycle 2 cut off by session limit) | **Best fitness:** 83% (5/6)
**Acceptance tests:** 6 (easy/medium/hard puzzles, validation, invalid rejection, empty board)

### Cycle 1
```
19 invocations, 18 mutations accepted, 3 rejected
RAF: 100% | Fitness: 83% (5/6) ↑
Lineage committed: f814ca8
```

One acceptance test failed. The solver produced by the coder handles 5 of 6
holdout tests but fails on one case. Session ended before cycle 2.

### Verdict

**Honest.** The system produces a solver that mostly works but isn't perfect.
One test failing out of six means there's a real bug — the solver can't handle
one class of input (likely the hard puzzle or the empty board edge case).

---

## Baseline Comparison

Codex one-shot (same task, direct CLI invocation):
- 264 lines of working Rust
- Compiles clean, 6/6 of its own tests pass
- **Solves real sudoku puzzles** (verified manually)
- One invocation, ~2 minutes, zero iterations

| Metric | Codex one-shot | A²D Run 1 (no acceptance) | A²D Run 2 (6 acceptance) |
|--------|---------------|--------------------------|-------------------------|
| Claimed fitness | N/A | 100% | 83% |
| Actually works? | Yes | Unknown | 5/6 tests |
| Invocations | 1 | 16 | 19 |
| Human intervention | 0 | 0 | 0 |
| False positive? | No | Possibly | No |

## Learnings

1. **One acceptance test is not testing.** Run 1 scored 100% with zero acceptance tests. Run 2 scored 83% with six. The difference is honesty.
2. **The false positive rate is the real metric.** Run 1 could have been a false positive — we'll never know because it wasn't tested. Run 2 is honest: 5/6.
3. **The system is currently slower AND less accurate than Codex one-shot.** 19 invocations across 3 providers to produce a solver that fails 1 test, vs. 1 invocation that works.
4. **The system needs to justify its existence through reliability across diverse tasks, not speed on one.** Rerun pending with 3 cycles to see if evolution closes the gap.
