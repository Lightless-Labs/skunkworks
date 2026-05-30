# Escalation Rungs 4-6: Model Swap + Multi-Model

**Created:** 2026-04-10
**Empirical update:** 2026-04-17 — live sudoku run confirms rungs 0–3 alone don't halt output repetition.
**Provider-health update:** 2026-04-23 — provider failures/timeouts now open a temporary provider-level circuit breaker and route subsequent invocations to healthy alternatives; this is not full rung 4 history-aware model swap.
**Provider-policy update:** 2026-05-23 — provider assignment is now a typed, gated, durable `provider_policy` artifact persisted as lineage `provider-policy.json`. This gives rung 4+ a safer mechanism for provider-role changes, but durable policy still needs topology-comparison gating.
**Implementation-status update:** 2026-05-29 — rungs 4–6 handler code has not been added to `invoke_scheduled` escalation branching. The circuit-breaker and provider-policy infrastructure exists, but swap/consensus logic still belongs in the current mechanism files (`crates/a2d-core/src/metabolism.rs` and `crates/a2d-core/src/provider.rs`).
**Depends on:** Rungs 0-3 (implemented), cycle iteration/firing cap (implemented), cycle wall-clock cap (implemented), provider-policy topology gate (`todos/provider-policy-topology-gate.md`).

## What's Built (observed firing live 2026-04-17)

- **Rung 0:** Loop detection (fitness signature + byte hash, persistent across cycles). Both variants confirmed firing.
- **Rung 1:** Inject awareness ("you're stuck, try fundamentally differently"). Fired for coder, evolver, architect.
- **Rung 2:** Consult another model (alternative provider gives advice, injected into primary's prompt). Fired for coder and architect.
- **Rung 3:** Clean session (strip failure_report context, start fresh). Fired for coder and evolver.
- **Provider circuit breaker (adjacent to rung 4):** provider invocation failures/timeouts temporarily cool down the failed provider and route subsequent invocations to healthy alternatives. Cooldown expiry makes the original provider eligible again, avoiding permanent bans. See `docs/solutions/runtime-bugs/provider-circuit-breaker-temporary-cooldown-2026-04-23.md`.

All 4 rungs layer: rung 3 includes awareness + consultation + clean session.

## Empirical motivation for rungs 4–6

Live run on sudoku (Kimi/Gemini/GLM), 2026-04-17: every dynamic enzyme climbed through rungs 0→1→2→3 within cycle 1 without producing fitness-improving output. Evolver entered rung 4 (unimplemented) — behaved as rung 3 since no handler exists. Rungs 4–6 are the next differentiated intervention. See `docs/solutions/architectural-insights/escalation-ladder-detects-but-doesnt-halt-degradation-2026-04-17.md`.

## Implementation Status

- **Rung 4 (swap with history):** NOT IMPLEMENTED. `invoke_scheduled` in `metabolism_workcell.rs` matches on `escalation_rung` but has no arm for rung ≥ 4 beyond falling through to rung-3 logic. Need to add `ProviderRegistry::swap_provider()` and wire it at `enzyme_loop_count >= 4`.
- **Rung 5 (swap + clean):** NOT IMPLEMENTED. Will reuse rung-4 swap + rung-3 clean-session code path.
- **Rung 6 (multi-model consensus):** NOT IMPLEMENTED. Requires parallel invocation and fitness-based selection.
- **Provider circuit breaker:** IMPLEMENTED (adjacent to rung 4). Temporary cooldown + reroute works. Durable policy swap via `provider-policy.json` exists but topology gate is not yet wired.

Next-action targets:
1. `crates/a2d-core/src/metabolism.rs` — add rung-4 provider override in `invoke_scheduled` / provider-selection flow
2. `crates/a2d-core/src/provider.rs` — add non-persistent alternative-provider selection helpers if the existing `alternative_provider_for*` APIs are insufficient
3. Write mock tests before live tests (per test plan below)

## Rung 2 status

Failed rung-2 consultation is now bounded: if consultation fails/timeouts, the workcell fails immediately instead of spending a second full provider timeout on the primary invocation. This fixed the live double-timeout failure mode documented on 2026-05-20.

## What's Next

### Rung 4: Swap models with history
**Trigger:** enzyme_loop_count >= 4
**Intervention:** Replace the enzyme's provider with the alternative provider. Pass the full failure history so the new model can learn from the old model's mistakes.
**Implementation:**
- Add `swap_provider(&mut self, enzyme_id: &EnzymeId)` to ProviderRegistry that swaps primary ↔ alternative
- In `invoke_scheduled`, when rung >= 4, swap before invoking
- The prompt should include: "A different model attempted this N times and failed with these fitness signatures: [...]"
- Swap persists until the loop counter resets (escape)

### Rung 5: Swap models with clean session
**Trigger:** enzyme_loop_count >= 5
**Intervention:** Like rung 4 (different model) but also strip all accumulated context (rung 3 clean session). The new model gets only the raw requirements — no failure history, no consultation, no prior context.
**Implementation:**
- Combine rung 4 (swap) + rung 3 (clean session) logic
- The prompt just says: "Solve this from scratch." + requirements only

### Rung 6: Multi-model consensus
**Trigger:** enzyme_loop_count >= 6
**Intervention:** Run N providers in parallel on the same task. Each produces an artifact. Benchmark all of them. Pick the highest-fitness result.
**Implementation:**
- In `invoke_scheduled`, when rung >= 6:
  1. Collect all registered providers
  2. Invoke each with the same request (in sequence — Rust is sync)
  3. For each response, extract the artifact and benchmark it
  4. Use the highest-fitness artifact as the cycle's output
- This is expensive (N provider calls per invocation) but it's the last resort
- Borrow from refinery patterns: cross-evaluation is optional; fitness-based selection is sufficient because the sandbox is the oracle

## Design Decisions for All Rungs

- **No model persistence:** When an enzyme escapes a loop (counter resets), the provider assignment reverts to the original. Temporary swaps don't persist.
- **No human gate:** All rungs fire mechanically. The counter drives the rung, the rung drives the intervention.
- **Fitness is the oracle:** Multi-model consensus (rung 6) doesn't use model agreement — it uses sandbox fitness. Models can unanimously agree on a wrong solution; the sandbox catches it.
- **Counter ceiling:** If rung 6 still doesn't work (counter reaches 7+), clamp at rung 6 and keep trying with multi-model. Eventually the fitness landscape will change (architect modifies the system) or the challenge is beyond the models' capability.

## Test Plan

Each rung needs:
1. A mock test proving the intervention fires at the correct counter value
2. A mock test proving it doesn't fire at lower counter values
3. A mock test proving counter reset reverts the intervention

Rung 6 additionally needs:
4. A mock test proving the highest-fitness artifact is selected
5. A mock test with a benchmark attached
