# Escalation Rungs 4-6: Model Swap + Multi-Model

**Created:** 2026-04-10
**Empirical update:** 2026-04-17 — live sudoku run confirms rungs 0–3 alone don't halt output repetition.
**Provider-health update:** 2026-04-23 — provider failures/timeouts now open a temporary provider-level circuit breaker and route subsequent invocations to healthy alternatives; this is not full rung 4 history-aware model swap.
**Provider-policy update:** 2026-05-23 — provider assignment is now a typed, gated, durable `provider_policy` artifact persisted as lineage `provider-policy.json`. This gives rung 4+ a safer mechanism for provider-role changes, but durable policy still needs topology-comparison gating.
**Implementation-status update:** 2026-05-29 — SUPERSEDED by the 2026-05-31 and 2026-06-01 updates below. At this point rungs 4–6 handler code had not been added to `invoke_scheduled`; the circuit-breaker and provider-policy infrastructure existed, but swap/consensus logic still belonged in `crates/a2d-core/src/metabolism.rs` and `crates/a2d-core/src/provider.rs`.
**Implementation-status update:** 2026-05-31 — rung 4 ephemeral provider swap is implemented and unit-tested in `crates/a2d-core/src/metabolism.rs` and `crates/a2d-core/src/provider.rs`. Rung 4 preserves failure history for the swapped provider; rung 5+ remains the clean-session swap path. Rungs 5–6 are still not implemented as distinct mechanisms beyond the rung-4 swap and existing clean-session behavior.
**Implementation-status update:** 2026-06-01 — rung 5 is now explicit and unit-tested in `crates/a2d-core/src/metabolism.rs`; invocation lineage, provider-health reports, and topology comparison output expose `escalation_rung`, `provider_swap`, and `clean_session`, with clean-session lineage recording provider-visible inputs.
**Implementation-status update:** 2026-06-01 — rung 6 bounded provider consensus is implemented and unit-tested in `crates/a2d-core/src/metabolism.rs`; it invokes a capped role-isolated provider portfolio, records candidate evaluations, selects highest code fitness when benchmarked, and uses deterministic fallback for non-code enzymes.
**Validation-harness update:** 2026-06-04 — `a2d validate-escalation <challenge> [enzyme]` now forces rungs 4, 5, and 6 through a diagnostic-only in-memory hook, runs the real registry with persistence disabled, and emits JSON using the external `escalation_rung` field contract.
**Eligibility-scope update:** 2026-06-05 — rung-6 consensus keeps the safe default of assigned + unassigned providers while excluding other-role assignments, and adds opt-in `A2D_RUNG6_PROVIDER_SCOPE=broad` for bounded probes that include all healthy registered providers.
**Push-sync update:** 2026-06-05 — scope probe is committed as `0409195 Add rung 6 provider scope probe`; next work is outcome-quality comparison of default vs broad scope, not more mechanism proof.
**Depends on:** Rungs 0-3 (implemented), cycle iteration/firing cap (implemented), cycle wall-clock cap (implemented), provider-policy topology gate (`todos/provider-policy-topology-gate.md`).

## What's Built (observed firing live 2026-04-17)

- **Rung 0:** Loop detection (fitness signature + byte hash, persistent across cycles). Both variants confirmed firing.
- **Rung 1:** Inject awareness ("you're stuck, try fundamentally differently"). Fired for coder, evolver, architect.
- **Rung 2:** Consult another model (alternative provider gives advice, injected into primary's prompt). Fired for coder and architect.
- **Rung 3:** Clean session (strip failure_report context, start fresh). Fired for coder and evolver.
- **Rung 4:** Ephemeral provider swap (implemented 2026-05-31). When an enzyme's loop counter reaches 4, the current invocation routes to a non-assigned provider without mutating durable `provider_policy`; the swap resets automatically when the output signature changes.
- **Rung 5:** Clean swapped session (implemented 2026-06-01). Keeps the ephemeral provider swap active and strips `failure_report`, with lineage/provider-health/topology metadata proving clean-session routing.
- **Rung 6:** Bounded provider consensus (implemented 2026-06-01, scope probe added 2026-06-05). Runs a capped provider portfolio (`A2D_RUNG6_MAX_PROVIDERS`, default 3), records candidate evaluations, picks highest code fitness when benchmarked, otherwise uses deterministic materialization fallback. Default eligibility is assigned + unassigned providers only, excluding other-role assignments; `A2D_RUNG6_PROVIDER_SCOPE=broad` opt-in includes other-role providers for experiments.
- **Provider circuit breaker (adjacent to rung 4):** provider invocation failures/timeouts temporarily cool down the failed provider and route subsequent invocations to healthy alternatives. Cooldown expiry makes the original provider eligible again, avoiding permanent bans. See `docs/solutions/runtime-bugs/provider-circuit-breaker-temporary-cooldown-2026-04-23.md`.

All 6 rungs layer: rung 3 includes awareness + consultation + clean session; rung 4 swaps; rung 5 swaps + cleans; rung 6 uses bounded provider consensus.

## Empirical motivation for rungs 4–6

Live run on sudoku (Kimi/Gemini/GLM), 2026-04-17: every dynamic enzyme climbed through rungs 0→1→2→3 within cycle 1 without producing fitness-improving output. Evolver entered rung 4 (unimplemented) — behaved as rung 3 since no handler exists. Rungs 4–6 are the next differentiated intervention. See `docs/solutions/architectural-insights/escalation-ladder-detects-but-doesnt-halt-degradation-2026-04-17.md`.

## Implementation Status

- **Rung 4 (swap with history):** IMPLEMENTED. `invoke_scheduled` in `crates/a2d-core/src/metabolism.rs` now uses an ephemeral provider override when the internal escalation counter reaches rung 4, backed by `ProviderRegistry::swapped_provider_for_avoiding()` and `role_isolated_swapped_provider_for_avoiding()`. It does not mutate provider assignments or durable provider policy.
- **Rung 5 (swap + clean):** IMPLEMENTED. The rung-5 path combines provider swap with clean-session failure-context stripping; lineage now records rung/swap/clean metadata and provider-visible inputs, provider-health reports carry escalation fields, and topology comparison prints escalation flags.
- **Rung 6 (multi-model consensus):** IMPLEMENTED. The rung-6 path invokes a bounded provider portfolio, records candidate evaluations, selects highest-fitness code under a benchmark, and falls back deterministically for non-code outputs. Provider eligibility is now explicit: the default includes assigned + unassigned providers while excluding providers assigned to other enzymes; opt-in `A2D_RUNG6_PROVIDER_SCOPE=broad` includes all healthy registered providers for bounded comparison.
- **Validation harness:** IMPLEMENTED. `validate-escalation` forces rungs 4/5/6 through a validation-only API, proves failure history is preserved at rung 4 and stripped at rungs 5/6 with a non-empty marker, records rung-6 candidate evaluations, and reports that provider policy did not change.
- **Provider circuit breaker:** IMPLEMENTED (adjacent to rung 4). Temporary cooldown + reroute works. Durable policy swap via `provider-policy.json` exists but topology gate is not yet wired.

Next-action targets:
1. Use `a2d validate-escalation sudoku coder` as the bounded smoke harness before changing rung behavior.
2. Compare default rung-6 scope (assigned + unassigned providers, excluding other-role assignments) against opt-in broad scope (`A2D_RUNG6_PROVIDER_SCOPE=broad`) on bounded challenge runs; keep broad out of defaults unless outcome evidence justifies the extra provider-window consumption.
3. Decide whether sequential rung-6 consensus is sufficient or whether a timeout-bounded concurrent variant is worth the loser-wait risk.
4. If bounded outcome evidence stays inconclusive, move to architect/tester provider-latency work rather than continuing mechanism-only escalation validation.

## Rung 2 status

Failed rung-2 consultation is now bounded: if consultation fails/timeouts, the workcell fails immediately instead of spending a second full provider timeout on the primary invocation. This fixed the live double-timeout failure mode documented on 2026-05-20.

## What's Next

### Keep rung 4–6 mechanism validation in the regression lane

**Trigger:** force escalation rungs 4/5/6 with `a2d validate-escalation`, or naturally reach those rungs through repeated behavioral signatures.
**Intervention evidence to inspect:**
- rung 4: provider swap with failure history preserved;
- rung 5: provider swap plus clean-session stripping;
- rung 6: bounded provider consensus with candidate evaluations and fitness/materialization selection.

Use the deterministic harness before and after provider-routing or rung changes. The 2026-06-04 bounded smoke confirmed real-registry JSON shows escalation metadata and that provider assignments/durable `provider_policy` remain unchanged. The 2026-06-05 scope smoke confirmed default rung 6 considers Kimi + DeepSeek for coder, while opt-in broad scope with cap 4 considers Kimi + DeepSeek + GLM + Pi. Remaining validation is whether broader eligibility or concurrency improves challenge outcomes enough to justify cost.

## Design Decisions for All Rungs

- **No model persistence:** When an enzyme escapes a loop (counter resets), the provider assignment reverts to the original. Temporary swaps don't persist.
- **No human gate:** All rungs fire mechanically. The counter drives the rung, the rung drives the intervention.
- **Fitness is the oracle:** Multi-model consensus (rung 6) doesn't use model agreement — it uses sandbox fitness. Models can unanimously agree on a wrong solution; the sandbox catches it.
- **Counter ceiling:** If rung 6 still doesn't work (counter reaches 7+), clamp at rung 6 and keep trying with multi-model. Eventually the fitness landscape will change (architect modifies the system) or the challenge is beyond the models' capability.

## Test Plan

Implemented unit coverage now includes:
1. mock tests proving rung 4 fires at the correct counter and does not fire below threshold;
2. reset coverage proving fresh output returns routing to the assigned provider;
3. rung 5 coverage for swapped clean-session routing, prompt shape, and provider-visible lineage inputs;
4. rung 6 coverage proving highest-fitness code selection under a benchmark;
5. rung 6 coverage for non-code deterministic materialized-success fallback;
6. rung-6 provider-scope parsing and default-vs-broad eligibility selection.

Remaining validation is live/bounded rather than unit-level: force rungs 4–6 under the real registry and inspect lineage/topology output plus provider-policy immutability.
