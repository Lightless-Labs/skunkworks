---
title: "Fitness-Scored Coder Portfolio"
date: 2026-05-20
category: best-practices
module: metabolism
problem_type: architecture_change
component: provider-dispatch
symptoms:
  - "Parallel coder dispatch waited for multiple providers but selected by provider order/materialization"
  - "Waiting for slow candidates did not buy higher quality or learning signal"
  - "Provider comparisons were not recorded as structured lineage evidence"
root_cause: parallel_coder_race_was_not_using_mechanical_outcome_selection
resolution_type: code_fix
severity: high
tags:
  - provider
  - portfolio
  - fitness
  - learning
  - coder
---

# Fitness-Scored Coder Portfolio

## Principle

Learning/adapting/self-improving > quality/correctness > speed.

The coder path should not pick the first artifact merely because it arrived quickly. Waiting for multiple providers is valuable when the system uses that wait to learn which candidate works best.

## Problem

The prior parallel coder implementation launched multiple providers but selected the first provider-order response that materialized code. That meant the system could wait for slow candidates without giving those candidates a chance to win by outcome.

This violated A²D's core selector:

```text
multi-model diversity + hidden holdout tests + mechanical selection
```

## Solution

Parallel coder dispatch is now a fitness-scored portfolio:

1. launch configured coder providers concurrently;
2. collect returned candidates;
3. materialize each candidate's `code` artifact;
4. evaluate every materialized code artifact with the current benchmark/sandbox;
5. select the highest-fitness candidate;
6. route only the winner into metabolism;
7. store every candidate's provider/materialization/error/fitness in invocation lineage.

Tie-breaker remains deterministic by provider order when fitness is equal. If no benchmark is attached, the path falls back to materialized-output selection.

The topology comparison CLI now prints candidate portfolio fitness for each cycle, turning provider choice into observable learning data rather than hidden dispatch behavior.

## Validation

Unit coverage added: `parallel_coder_selects_highest_fitness_candidate` verifies that a weaker first provider loses to a stronger second provider when sandbox fitness differs.

Full test suite passes: 146 tests passing, 2 ignored.

Smoke validation:

```bash
A2D_TRACE=1 A2D_PROVIDER_TIMEOUT_SECS=1 A2D_MAX_CYCLE_SECS=1 \
  cargo run -p a2d -- compare-topologies sudoku 1
```

The topology comparison output now prints each coder candidate with materialization/error state. Log: `/tmp/a2d-topology-compare-sudoku1-portfolio-smoke-20260520.log`.

## Related

- `docs/solutions/best-practices/parallel-cheap-coder-race-2026-05-19.md`
- `docs/solutions/runtime-bugs/coder-glm-critical-path-timeouts-2026-05-20.md`
- `todos/bounded-live-benchmarks.md`
