---
title: "One Acceptance Test Is Not Acceptance Testing"
date: 2026-04-02
category: best-practices
module: benchmark
problem_type: best_practice
component: tooling
severity: high
applies_when:
  - "Writing acceptance tests for benchmark challenges or evaluation harnesses"
  - "A single test passes and you are tempted to declare success"
  - "Validating solver correctness, model capability, or pipeline output"
  - "Building holdout benchmarks for fitness-gated evolution"
tags:
  - acceptance-testing
  - test-coverage
  - benchmark
  - false-confidence
  - edge-cases
  - validation
---

# One Acceptance Test Is Not Acceptance Testing

## Context

The sudoku challenge initially had a single puzzle as its acceptance test. It scored 100% and we nearly declared success. But one puzzle could be a fluke — the solver could have a bug that happens to not affect that specific puzzle, or the model could have memorized that particular puzzle-solution pair. A single passing test creates false confidence that is worse than no test at all, because it suppresses further investigation.

## Guidance

Every acceptance test suite must cover diverse inputs, edge cases, and negative cases. The question is never "does it pass A test" — it is "does it do what it is supposed to do across the range of inputs it will encounter."

### Minimum viable acceptance coverage

For any challenge or benchmark, expand from one test to at least these dimensions:

1. **Difficulty spread**: Multiple difficulty levels (e.g., easy / medium / hard). A solver that handles easy cases but fails hard ones is not a solver — it is a demo.

2. **Positive validation**: Verify that correct outputs are actually correct. For sudoku: does the solved board satisfy all constraints (rows, columns, blocks)? Do not trust the solver's own claim of success.

3. **Negative cases**: Feed invalid, malformed, or unsolvable inputs. The system should reject or report failure, not silently produce garbage. For sudoku: invalid puzzles (duplicate numbers in a row) should be rejected.

4. **Boundary cases**: Empty inputs, maximally constrained inputs, minimal inputs. For sudoku: an empty board (should be solvable), a nearly-complete board (trivial), a board with only one valid solution vs. multiple.

### The sudoku example

| Test | What it catches |
|------|----------------|
| Easy puzzle | Baseline: does it work at all? |
| Medium puzzle | Does it handle moderate constraint propagation? |
| Hard puzzle | Does it handle backtracking / advanced techniques? |
| Validate solved board | Does the output actually satisfy the rules? |
| Reject invalid puzzle | Does it detect bad input instead of producing nonsense? |
| Solve empty board | Does it handle the degenerate case? |

One test would have caught none of the failure modes in rows 2-6. Six tests cover the space that matters.

### How to decide "enough"

The heuristic: if you can describe a category of input that the system should handle, and no existing test covers it, add one. Stop when you cannot think of an uncovered category that a real user or upstream system would produce.

For benchmarks used in fitness-gated evolution, the bar is higher — a holdout benchmark with only one test is a single point of failure for the entire lineage. A fluke pass lets a broken germline into the archive.

## Why This Matters

- **False confidence compounds**: A single-test pass stops investigation. Bugs that would have been caught by a second test survive into production, evolution archives, or downstream systems.
- **Memorization is real**: LLMs can memorize specific puzzle-solution pairs from training data. A single test cannot distinguish memorization from capability. Multiple diverse tests can.
- **Negative cases catch category errors**: A solver that returns "solved!" for invalid input is fundamentally broken in a way that no number of positive tests will reveal.
- **Benchmark integrity**: In fitness-gated evolution, the holdout benchmark is the sole mechanical arbiter of germline quality. A weak benchmark lets regressions through, defeating the entire purpose of the fitness gate.

## When to Apply

- Every benchmark challenge, without exception
- Holdout benchmarks used for fitness gating (especially critical — see related document)
- Any evaluation harness where a "pass" triggers downstream action (commit, deploy, evolve)
- Model capability assessments where memorization is a confound

**Not needed for:**
- Smoke tests whose sole purpose is "does it start up"
- Exploratory tests during development (but convert to proper acceptance tests before declaring done)

## Design Considerations

- **Cost vs. coverage**: More tests cost more compute. For LLM-based solvers, each test is an API call. Six tests is a reasonable minimum — the cost of six calls is trivial compared to the cost of a false positive propagating through an evolution pipeline.
- **Independence**: Each test should be independently evaluable. Do not chain tests where failure of test 1 blocks test 2. Run all tests and report the full vector.
- **Scoring granularity**: Report pass-count / total-count as a percentage. "1/1 = 100%" hides fragility. "5/6 = 83%" reveals which category failed and guides debugging.
- **Test evolution**: As the system improves, add harder tests. A benchmark that never fails is no longer providing signal — it has become a rubber stamp.

## Related

- `docs/solutions/best-practices/fitness-gated-evolution-holdout-raf-lineage-2026-04-01.md` — holdout benchmarks as fitness gates; this document explains why those benchmarks must have adequate coverage
- `docs/solutions/best-practices/multi-model-dispatch-mechanical-selection-2026-04-01.md` — mechanical selection depends on benchmark quality; weak benchmarks undermine mechanical guarantees
