---
module: a2d-cli
tags: [senior-swe-bench, retry-loop, evidence-gates, information-barriers]
problem_type: best-practice
---

# Retry loops must be machine-verifiable before provider calls

## Problem

After a failed Senior SWE-Bench candidate evaluation, it is tempting to wire an automatic retry loop immediately. That risks creating cosmetic autonomy: providers may be invoked repeatedly without bounded attempts, clear stop criteria, or a fresh `a2d.fitness-evidence.v1` gate for any success claim.

## Pattern

Before starting providers or evaluators, emit a machine-readable retry plan that states:

- maximum attempts and explicit exhaustion behavior;
- the required per-attempt gates (`cycle-input` output capture, patch extraction, evaluator run, evidence inspection);
- fail-closed stops for extraction failure and policy/evidence-binding mismatch;
- success only after non-regressing actual-test `a2d.fitness-evidence.v1`;
- information barriers for no public GitHub solution search and hidden-holdout redaction.

The plan itself is not evidence and must start no providers or evaluators.

## Evidence

Implemented by `a2d senior-swe-bench-retry-plan` in `crates/a2d-cli/src/main.rs` / `crates/a2d-cli/src/senior_swe_bench.rs`, with CLI coverage in `crates/a2d-cli/tests/senior_swe_bench_retry_plan.rs`.

Fresh source-patch gate evidence: `runs/20260704-senior-swe-bench-retry-plan-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, full-passing with `source_diff_hash: fa652ee3ca175bb5cb37d15ab106840d17c37f84`.

Run doc: `examples/runs/2026-07-04-senior-swe-bench-retry-plan.md`.
