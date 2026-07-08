# Provider Policy Cost-Gate Coverage — 2026-07-08

## Purpose

Pin the cost side of the provider-policy durability gate with direct unit coverage. Prior plans required rejecting provider-policy proposals that preserve fitness but materially increase invocation count or wall-clock cost; previous unit tests covered worse fitness and clearly better persistence, but not these two cost branches directly.

## Lineage constraints

- `docs/plans/provider-policy-topology-gate.md` requires durable provider-policy changes to be outcome-gated, not merely schema-gated.
- `todos/provider-policy-topology-gate.md` specifies that current/proposed comparisons must reject material wall-clock or invocation increases.
- `docs/plans/provider-policy-artifact.md` notes that high-impact provider-policy changes should remain comparison-gated and not become durable defaults without evidence.
- Recent Pi-Minimax tester/architect comparisons showed equal or improved fitness can still be rejected for invocation/wall-clock cost, so the cost branches are safety-relevant.

## Change

Added two tests in `crates/a2d-cli/src/main.rs`:

- `provider_policy_gate_rejects_material_invocation_cost_increase`
- `provider_policy_gate_rejects_material_wall_clock_cost_increase`

Both fixtures hold best fitness equal and assert `fitness_delta == 0.0`, so the rejection is pinned to cost. The invocation fixture exceeds the `max(1, current_invocations / 4)` slack, and the wall-clock fixture exceeds `current_elapsed * 0.25 + 5.0` slack.

## Validation

Focused checks:

```bash
cargo fmt --check
CARGO_BUILD_JOBS=2 cargo test -p a2d provider_policy_gate_rejects_material -- --nocapture
```

Full suite:

```bash
CARGO_BUILD_JOBS=2 cargo test
```

Reviewer: independent `reviewer` subagent found no blockers. It suggested asserting `fitness_delta == 0.0`; the final tests include those assertions.

## Fresh fitness evidence

Command:

```bash
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260708-provider-policy-cost-gate-coverage-evidence/actual-test-score-artifact \
  cargo run -q -p a2d -- score-artifact sudoku runs/20260701-score-artifact-fitness-evidence/good-sudoku-artifact.rs
cargo run -q -p a2d -- fitness-evidence-inspect \
  runs/20260708-provider-policy-cost-gate-coverage-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json \
  --require-all-tests-pass
```

Artifact: `runs/20260708-provider-policy-cost-gate-coverage-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`

Inspected fields:

- `schema_version: a2d.fitness-evidence.v1`
- `actual_tests_evaluated: true`
- `non_regressing: true`
- `fitness: 1.0` (6/6)
- `failed_cases: []`
- result labels include `all_tests_pass`
- `source_diff_scope: crates`
- `source_tree_dirty: true`
- `source_diff_hash: f41c4653f0fecd35ec3a5eac4a6647796664df65`

The evidence hash matched:

```bash
git diff --binary HEAD -- crates | git hash-object --stdin
```

## Scope

This is provider-policy cost-gate test coverage only. It does not change provider defaults, persist provider policy, prove a benchmark-useful policy, prove OS/network no-egress, or claim official Senior SWE-Bench mastery.
