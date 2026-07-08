---
module: provider-policy
tags:
  - provider-policy
  - evidence-gates
  - topology-comparison
  - test-coverage
problem_type: test-coverage-gap
---

# Provider policy fail-closed branches need direct coverage

## Problem

Provider-policy persistence is an autonomous self-improvement mechanism. The topology gate rejects proposals when comparison evidence is absent, when both sides have zero fitness, when proposed fitness is worse, or when equal-fitness proposals cost materially more.

The missing-evidence and zero-fitness branches are especially easy to treat as harmless defaults during refactors. If either branch silently accepted, A²D could durably persist provider policies without a meaningful outcome signal.

## Decision

Pin the fail-closed branches directly with unit tests in `crates/a2d-cli/src/main.rs`:

- `provider_policy_gate_rejects_missing_fitness_evidence`
- `provider_policy_gate_rejects_zero_fitness_comparison_as_inconclusive`

The missing-evidence fixture removes the current summary's test denominator. The zero-fitness fixture supplies complete current/proposed summaries with no successful fitness signal. Both assert rejection and the user-facing gate reason.

## Evidence

Fresh source-bound evidence for the coverage hardening:

- `runs/20260708-provider-policy-fail-closed-coverage-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`
- `schema_version: a2d.fitness-evidence.v1`
- `actual_tests_evaluated: true`
- `non_regressing: true`
- `fitness: 1.0`
- `all_tests_pass: true`
- `source_tree_dirty: true`
- `source_diff_scope: crates`
- `source_diff_hash: 816612ba83ba9365d45014f9d555c6016a1503b4`

Validation included focused provider-policy fail-closed tests, full `CARGO_BUILD_JOBS=2 cargo test`, reviewer with no blockers, and `fitness-evidence-inspect --require-all-tests-pass`.

## Scope

This is test coverage for an existing provider-policy durability gate. It does not change default provider assignments, persist provider policy, prove a benchmark-useful provider-policy proposal, prove OS/network no-egress, or claim official Senior SWE-Bench mastery.
