# Provider Policy Topology Gate

**Created:** 2026-05-23
**Started:** 2026-05-26 — bounded current-vs-proposed comparison gate implemented
**Completed:** 2026-05-28 — live runtime provider_policy proposal rejected for missing comparison evidence; no durable lineage policy written
**Hardened:** 2026-07-08 — direct unit coverage now pins material invocation, wall-clock cost, missing-evidence, and zero-fitness rejection branches
**Plan:** `docs/plans/provider-policy-topology-gate.md`
**Depends on:** `provider_policy` typed artifact and lineage persistence (implemented 2026-05-23).

## Problem

Provider policy is now typed, mechanically schema-gated, and durable in lineage as `provider-policy.json`. That closes the persistence loop, but the durability gate is still too weak for autonomous provider-role adaptation:

- target enzyme must exist;
- provider must be registered;
- cycle must not regress if a policy change is committed.

Those checks prevent malformed or impossible policies, but they do not prove that a provider assignment improves outcomes. A bad-but-valid policy can still become durable if it happens to be accepted during a noisy non-regressing cycle.

## Desired Gate

Before a provider policy becomes a durable default, compare the current policy against the proposed policy using bounded topology/challenge runs.

Minimal acceptance rule:

1. Run seed/evolved or current/proposed with identical challenge, cycle count, provider timeout, and cycle wall-clock budget.
2. Accept proposed policy only if it is not worse on best fitness and does not materially increase wall-clock/invocation cost.
3. Reject or keep transient if the result is noisy or inconclusive.

## Implementation Sketch

1. Add a non-persistent policy comparison path, likely reusing `compare-topologies` internals:
   - current provider policy;
   - proposed provider policy;
   - no lineage commits;
   - no patch application.
2. Make `LineageArchive::commit_provider_policy` reachable only after the comparison gate for autonomous changes.
3. Keep operator/manual runs able to inspect and force policy experiments explicitly.
4. Print provider-policy deltas in topology comparison summaries.

## Acceptance Criteria

- [x] Unit test: valid provider policy can be accepted in memory but withheld from lineage when the gate fails.
- [x] Unit test: a clearly better mock policy is persisted.
- [x] CLI smoke: comparison output includes current/proposed policy names and policy deltas.
- [x] Live bounded run: a real provider-policy proposal does not become durable without comparison evidence.

## Notes

This is the next safety step after `provider-policy-lineage-persistence-2026-05-23.md`. Provider routing is now an evolvable mechanism; durable evolution needs outcome evidence, not just schema validity.

## Coverage hardening

2026-07-08: Added direct regression coverage for the existing cost rejection branches. `provider_policy_gate_rejects_material_invocation_cost_increase` and `provider_policy_gate_rejects_material_wall_clock_cost_increase` keep best fitness equal and assert `fitness_delta == 0.0`, then exceed invocation or wall-clock slack so durable provider-policy changes still fail closed on cost. Fresh source-patch evidence: `runs/20260708-provider-policy-cost-gate-coverage-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json` (`source_diff_hash: f41c4653f0fecd35ec3a5eac4a6647796664df65`). This is coverage only; no provider default or durable policy changed.

2026-07-08: Added direct regression coverage for the missing-evidence and zero-fitness inconclusive branches. `provider_policy_gate_rejects_missing_fitness_evidence` proves absent comparison evidence cannot persist a policy, and `provider_policy_gate_rejects_zero_fitness_comparison_as_inconclusive` proves no-signal comparisons fail closed instead of becoming durable defaults. Fresh source-patch evidence: `runs/20260708-provider-policy-fail-closed-coverage-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json` (`source_diff_hash: 816612ba83ba9365d45014f9d555c6016a1503b4`). This is coverage only; no provider default or durable policy changed.

## Live validation

2026-05-28: Temporarily installed a probe lineage germline with a `maintainer` enzyme that produced a real `provider_policy` artifact via `pi/default`, proposing `maintainer: pi/default -> opencode/kimi-for-coding/k2p6`. Runtime accepted the policy in memory (`Provider policy accepted: 1`), then the durability gate ran current-vs-proposed comparison and rejected persistence for missing fitness evidence. Confirmed `.a2d/lineage/provider-policy.json` was absent afterward. Log: `/tmp/a2d-provider-policy-runtime-proposal-20260528162751.log`. Learning: `docs/solutions/architectural-insights/provider-policy-runtime-proposals-must-stay-comparison-gated-2026-05-28.md`.
