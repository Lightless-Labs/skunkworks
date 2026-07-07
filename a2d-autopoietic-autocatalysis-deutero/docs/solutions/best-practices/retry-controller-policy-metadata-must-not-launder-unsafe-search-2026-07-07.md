---
module: a2d-cli
tags: [senior-swe-bench, retry-loop, no-search-policy, metadata]
problem_type: best-practice
---

# Retry controller policy metadata must not launder unsafe search flags

## Problem

Retry controller summaries are audit artifacts. If a child gate or prior status says `github_solution_search_allowed: true`, the controller must surface that unsafe state instead of normalizing it to `false`. Otherwise downstream readers can mistake a tainted retry path for a no-search-compliant path.

## Pattern

Treat `github_solution_search_allowed` as taint metadata in controller summaries:

- preserve `true` from prior status;
- preserve `true` from child gate artifacts;
- default to `false` only when reviewed inputs are absent/false;
- persist the same value the in-memory controller result reports.

This is transparency, not authorization. Separate gates still reject solution-search-allowed task inputs and retry boundaries, provider subprocesses still receive no-search policy env flags, and evidence labels must remain policy-scoped rather than claiming network isolation.

## Evidence

Implemented in `crates/a2d-cli/src/main.rs` by OR-preserving `github_solution_search_allowed` in `retry_next_gate_execution_value` and routing non-terminal next-gate call sites through `write_retry_next_gate_execution_artifact`.

Regression: `retry_next_gate_execution_surfaces_github_solution_search_policy_from_inputs` covers safe false, prior-status true, child true, and persisted controller artifact behavior.

Fresh source-patch evidence: `runs/20260707-retry-next-gate-policy-surfacing-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, full-passing `a2d.fitness-evidence.v1` with `source_diff_hash: d2e57b7f0b565118116999e97d78e9b1e2c9f43d` matching the scoped crates diff.
