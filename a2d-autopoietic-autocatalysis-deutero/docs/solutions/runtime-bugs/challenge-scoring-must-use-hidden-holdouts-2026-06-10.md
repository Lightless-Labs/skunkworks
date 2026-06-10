---
title: "Challenge Scoring Must Use Hidden Holdouts Through One Helper"
date: 2026-06-10
category: runtime-bugs
module: challenges
problem_type: benchmark_validity
component: challenge-scoring
symptoms:
  - "Baseline/replay scoring paths can accidentally evaluate visible string checks without appended acceptance tests"
  - "Post-expansion chess/Rubik's validation needed a way to replay artifacts against holdouts without another provider run"
root_cause: direct_public_challenge_benchmark_access_bypassed_acceptance_test_attachment
resolution_type: code_fix
severity: high
tags:
  - benchmark
  - acceptance-tests
  - information-barrier
  - replay
---

# Challenge Scoring Must Use Hidden Holdouts Through One Helper

## Problem

`Challenge` carried `benchmark` and `acceptance_test` as separate public fields. Most challenge runs remembered to copy `acceptance_test` into the benchmark before calling `BenchmarkSuite::evaluate`, but `Challenge::establish_baseline()` did not. Any future replay or baseline path could make the same mistake and report fitness from visible checks only.

This is especially risky after expanding chess and Rubik's acceptance coverage: a provider may satisfy visible API/string checks while failing the hidden behavioral holdouts.

## Solution

`Challenge` now owns the composition point:

- `Challenge::scoring_benchmark()` returns a cloned benchmark with hidden acceptance tests attached;
- `Challenge::score_artifact()` scores generated code through that benchmark;
- raw `benchmark` / `acceptance_test` fields are private outside `challenges.rs`;
- CLI challenge/topology/provider-policy/escalation paths use `scoring_benchmark()` instead of manual field copying;
- `a2d score-artifact <challenge> <path|->` replays a saved/generated Rust artifact against the same challenge scoring path and exits nonzero unless fitness is perfect.

`score-artifact` prints case names and pass/fail counts but does not print sandbox diagnostics by default, because diagnostics may contain hidden acceptance names, assertion text, or source snippets. Failed replay exits with status 2 so CI/replay scripts cannot mistake partial fitness for a pass.

## Validation

Unit coverage proves:

- scoring benchmarks carry acceptance tests for all challenges;
- a fake Sudoku artifact with visible functions and passing local tests still fails `all_tests_pass` through `score_artifact()`;
- `establish_baseline()` uses the same hidden-holdout path;
- `score-artifact` output redacts diagnostic contents;
- `score-artifact` exits 2 for partial/zero fitness and 0 only for perfect fitness.

Full `cargo test` passes: 218 tests passing, 2 ignored.

Live smoke:

```bash
cargo run -q -p a2d -- score-artifact sudoku /tmp/a2d-bad-sudoku-artifact.rs
```

The fake artifact scored 83% (5/6): visible API checks passed, hidden acceptance failed, diagnostics were captured but not printed, and the process exited with status 2.

## Related

- `todos/test-evolution.md`
- `docs/plans/challenge-acceptance-test-expansion.md`
- `docs/solutions/best-practices/acceptance-test-coverage-one-test-is-not-testing-2026-04-02.md`
