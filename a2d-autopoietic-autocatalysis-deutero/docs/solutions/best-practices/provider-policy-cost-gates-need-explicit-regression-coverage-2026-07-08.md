---
module: provider-policy
tags:
  - provider-policy
  - cost-gate
  - evidence-gates
  - topology-comparison
problem_type: test-coverage-gap
---

# Provider policy cost gates need explicit regression coverage

## Problem

Provider policy changes are evolvable mechanisms: a model can propose new role-provider assignments, and A²D can make accepted assignments durable in lineage. The topology gate already rejects policies that worsen best fitness, lack fitness evidence, or materially increase invocation count / wall-clock cost.

The fitness rejection and clearly-better persistence paths had unit coverage, but the cost-specific rejection branches were only documented and exercised indirectly through live Pi-Minimax policy rejection evidence. That left a narrow regression risk: a future refactor could accidentally stop rejecting equal-fitness policies that spend many more provider invocations or much more wall-clock time.

## Decision

Pin the cost branches directly with unit tests in `crates/a2d-cli/src/main.rs`:

- `provider_policy_gate_rejects_material_invocation_cost_increase`
- `provider_policy_gate_rejects_material_wall_clock_cost_increase`

Both fixtures keep `fitness_delta == 0.0` so the rejection reason must come from cost, not worse fitness. The invocation fixture exceeds `max(1, current_invocations / 4)` slack; the wall-clock fixture exceeds `current_elapsed * 0.25 + 5.0` slack.

## Evidence

Fresh source-bound evidence for the coverage hardening:

- `runs/20260708-provider-policy-cost-gate-coverage-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`
- `schema_version: a2d.fitness-evidence.v1`
- `actual_tests_evaluated: true`
- `non_regressing: true`
- `fitness: 1.0`
- `all_tests_pass: true`
- `source_tree_dirty: true`
- `source_diff_scope: crates`
- `source_diff_hash: f41c4653f0fecd35ec3a5eac4a6647796664df65`

Validation included focused provider-policy cost-gate tests, full `CARGO_BUILD_JOBS=2 cargo test`, reviewer with no blockers, and `fitness-evidence-inspect --require-all-tests-pass`.

## Scope

This is test coverage for an existing provider-policy durability gate. It does not change default provider assignments, persist provider policy, prove a benchmark-useful provider-policy proposal, prove OS/network no-egress, or claim official Senior SWE-Bench mastery.
