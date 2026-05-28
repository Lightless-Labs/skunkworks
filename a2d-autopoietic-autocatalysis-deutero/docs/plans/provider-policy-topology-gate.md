# Provider Policy Topology Gate

**Created:** 2026-05-26
**Started:** 2026-05-26 — bounded current-vs-proposed gate implemented
**Completed:** 2026-05-28 — live runtime provider_policy proposal exercised and durability was rejected without fitness evidence
**Todo:** `todos/provider-policy-topology-gate.md`

## Goal

Make durable provider-policy persistence outcome-gated, not merely schema-gated.

A provider policy may already be accepted in memory when it targets known germline enzymes and registered providers. This plan adds the next safety boundary: before an autonomous provider-policy change is written to lineage as the new default, A²D must compare the previous policy against the proposed policy with identical bounded challenge settings.

## Minimal slice

1. Add a policy comparison runner that executes `current` and `proposed` provider policies against the same challenge/cycle budget with persistence disabled.
2. Add a deterministic gate decision:
   - accept only when proposed best fitness is not worse;
   - reject if proposed materially increases invocation count or wall-clock cost;
   - reject inconclusive/noisy results instead of durably committing by default.
3. Route runtime `commit_provider_policy` calls through this gate when provider-policy changes were accepted in memory.
4. Add a CLI inspection path for operator experiments that prints current/proposed policy deltas and comparison summaries without lineage commits.
5. Keep invalid provider policies rejected by the existing schema/provider/enzyme gate before comparison.

## Implementation status

Implemented on 2026-05-26:

- `a2d compare-provider-policy <challenge> <cycles> [policy-json|@path]` runs current and proposed provider policies with persistence disabled.
- Runtime provider-policy lineage commits now route through `commit_provider_policy_if_gate_accepts` after a bounded current-vs-proposed comparison.
- The gate rejects missing fitness evidence, worse best fitness, zero-fitness inconclusive comparisons, material invocation increases, and material wall-clock increases.
- Provider-policy snapshots used for comparison/persistence are filtered to current germline enzymes so outer-loop-only assignments such as `maintainer` are not written as durable challenge-metabolism policy.

## Acceptance criteria

- [x] Unit test: valid provider policy can be accepted in memory but withheld from lineage when comparison evidence fails.
- [x] Unit test: a clearly better mock policy is persisted.
- [x] CLI smoke: provider-policy comparison output names `current` and `proposed` policy modes and prints policy deltas.
- [x] Live bounded run: a real provider-policy proposal does not become durable without comparison evidence.

## Non-goals

- No unbounded or repeated benchmark suite in this slice.
- No remote push or external policy registry.
- No weakening of existing provider-policy schema validation.

## Validation

- `cargo test`
- Bounded CLI smoke with tiny budgets, for example:

```bash
A2D_PROVIDER_TIMEOUT_SECS=1 A2D_MAX_CYCLE_SECS=1 cargo run -p a2d -- compare-provider-policy sudoku 1
```

- Live runtime proposal validation (2026-05-28): temporarily installed a probe lineage germline whose `maintainer` enzyme emitted a real `provider_policy` artifact through `pi/default`, proposing `maintainer: pi/default -> opencode/kimi-for-coding/k2p6`. The cycle accepted the policy in memory, ran the bounded current-vs-proposed durability gate, rejected lineage persistence for missing fitness evidence, and left no `.a2d/lineage/provider-policy.json`. Log: `/tmp/a2d-provider-policy-runtime-proposal-20260528162751.log`.
