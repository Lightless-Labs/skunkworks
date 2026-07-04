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

Then add a deterministic retry-step gate that consumes the plan plus a single evaluation result and chooses only the next safe action. The step should validate schema-complete plan fields (including stop criteria, barriers, and per-attempt transitions), produce feedback-enriched next cycle input only for failed non-final attempts, and treat passed evaluations as instructions to inspect evidence rather than as fitness claims.

Before wiring a loop executor, add a retry-attempt planner that composes existing gates into command arguments only. It should select the exact coder artifact by manifest provenance, verify unified-diff extractability, emit planned extraction/evaluation/retry-step args, and stop with no evaluator args when extraction fails.

The plan/step/attempt artifacts are not evidence and must start no providers or evaluators.

## Evidence

Implemented by `a2d senior-swe-bench-retry-plan`, `a2d senior-swe-bench-retry-step`, and `a2d senior-swe-bench-retry-attempt-plan` in `crates/a2d-cli/src/main.rs` / `crates/a2d-cli/src/senior_swe_bench.rs`, with CLI coverage in `crates/a2d-cli/tests/senior_swe_bench_retry_plan.rs`, `crates/a2d-cli/tests/senior_swe_bench_retry_step.rs`, and `crates/a2d-cli/tests/senior_swe_bench_retry_attempt_plan.rs`.

Fresh retry-plan source-patch gate evidence: `runs/20260704-senior-swe-bench-retry-plan-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, full-passing with `source_diff_hash: fa652ee3ca175bb5cb37d15ab106840d17c37f84`.

Fresh retry-step source-patch gate evidence: `runs/20260704-senior-swe-bench-retry-step-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, full-passing with `source_diff_hash: f2073d167fcddc55903fa19d8e459d8b25d29e0c`.

Fresh retry-attempt-plan source-patch gate evidence: `runs/20260704-senior-swe-bench-retry-attempt-plan-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, full-passing with `source_diff_hash: 95e5f526834ef5bce322ab1f474e9f9ef5fcba0b`.

Run docs: `examples/runs/2026-07-04-senior-swe-bench-retry-plan.md`, `examples/runs/2026-07-04-senior-swe-bench-retry-step.md`, `examples/runs/2026-07-04-senior-swe-bench-retry-attempt-plan.md`.
