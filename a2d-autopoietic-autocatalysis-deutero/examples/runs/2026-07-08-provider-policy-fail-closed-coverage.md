# Provider Policy Fail-Closed Coverage — 2026-07-08

## Purpose

Complete direct unit coverage for the remaining provider-policy durability gate fail-closed branches. The earlier same-day slice pinned material invocation and wall-clock cost rejections; this slice pins the branches that reject missing fitness evidence and zero-fitness comparisons as inconclusive.

## Lineage constraints

- `docs/plans/provider-policy-topology-gate.md` requires durable provider-policy changes to be outcome-gated, not merely schema-gated.
- `todos/provider-policy-topology-gate.md` specifies rejecting noisy/inconclusive proposals and withholding lineage persistence when evidence is absent.
- Provider-policy changes are evolvable mechanisms, so these rejection branches must stay mechanically covered even when implementation refactors change comparison plumbing.

## Change

Added two tests in `crates/a2d-cli/src/main.rs`:

- `provider_policy_gate_rejects_missing_fitness_evidence`
- `provider_policy_gate_rejects_zero_fitness_comparison_as_inconclusive`

The missing-evidence fixture zeroes the current summary denominator so `decide_provider_policy_gate` cannot treat the comparison as fitness evidence. The zero-fitness fixture gives both current and proposed policies complete but zero-fitness summaries, proving the gate rejects the result as inconclusive instead of durably accepting a no-signal policy.

## Validation

Focused checks:

```bash
cargo fmt --check
CARGO_BUILD_JOBS=2 cargo test -p a2d provider_policy_gate_rejects -- --nocapture
```

Full suite:

```bash
CARGO_BUILD_JOBS=2 cargo test
```

Reviewer: independent `reviewer` subagent found no blockers.

## Fresh fitness evidence

Command:

```bash
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260708-provider-policy-fail-closed-coverage-evidence/actual-test-score-artifact \
  cargo run -q -p a2d -- score-artifact sudoku runs/20260701-score-artifact-fitness-evidence/good-sudoku-artifact.rs
cargo run -q -p a2d -- fitness-evidence-inspect \
  runs/20260708-provider-policy-fail-closed-coverage-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json \
  --require-all-tests-pass
```

Artifact: `runs/20260708-provider-policy-fail-closed-coverage-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`

Inspected fields:

- `schema_version: a2d.fitness-evidence.v1`
- `actual_tests_evaluated: true`
- `non_regressing: true`
- `fitness: 1.0` (6/6)
- `failed_cases: []`
- result labels include `all_tests_pass`
- `source_diff_scope: crates`
- `source_tree_dirty: true`
- `source_diff_hash: 816612ba83ba9365d45014f9d555c6016a1503b4`

The evidence hash matched:

```bash
git diff --binary HEAD -- crates | git hash-object --stdin
```

## Scope

This is provider-policy fail-closed test coverage only. It does not change provider defaults, persist provider policy, prove a benchmark-useful policy, prove OS/network no-egress, or claim official Senior SWE-Bench mastery.
