# Provider Policy Topology Gate

**Created:** 2026-05-23
**Started:** 2026-05-26 — bounded current-vs-proposed comparison gate implemented
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
- [ ] Live bounded run: a real provider-policy proposal does not become durable without comparison evidence.

## Notes

This is the next safety step after `provider-policy-lineage-persistence-2026-05-23.md`. Provider routing is now an evolvable mechanism; durable evolution needs outcome evidence, not just schema validity.
