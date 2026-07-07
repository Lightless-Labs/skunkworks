---
module: a2d-cli
tags: [senior-swe-bench, retry-status, evidence-gates, no-search-policy]
problem_type: best-practice
---

# Retry status must revalidate terminal run-result boundaries

## Problem

A terminal retry-run result is a summary artifact, not authoritative evidence. Even when retry status re-reads the final `a2d.fitness-evidence.v1`, a tampered terminal summary can still carry unsafe boundary fields such as `github_solution_search_allowed: true`, `provider_invocations_started: true`, or a non-boolean evidence-inspection pass flag.

If status accepts those fields implicitly, downstream controllers can confuse a successful evidence-backed retry summary with a summary that also claims unsafe side effects or malformed evidence gates.

## Pattern

For successful retry executions, status should rebuild claims from inspected evidence and independently revalidate terminal run-result boundary booleans:

- no GitHub/public solution search allowed;
- no provider/evaluator invocation started by the run-result summary command;
- evidence inspection did start and pass;
- no fitness claim before evidence and a claim only after evidence inspection.

This keeps `senior-swe-bench-retry-status` read-only and prevents terminal summaries from becoming trusted claim sources.

## Evidence

Implemented in `crates/a2d-cli/src/main.rs` by validating `terminal_run_result` booleans inside `build_senior_swe_bench_retry_status` before accepting a successful retry execution.

Regression coverage in `crates/a2d-cli/tests/senior_swe_bench_retry_execute.rs` tampers a successful retry execution and proves status rejects unsafe or malformed terminal-run-result fields.

Fresh source-patch evidence: `runs/20260707-retry-status-terminal-run-result-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, full-passing `a2d.fitness-evidence.v1` with `source_diff_hash: afae99ef0c9a88073a4af6ea1dbd4c7880bc577e` matching the scoped crates diff.
