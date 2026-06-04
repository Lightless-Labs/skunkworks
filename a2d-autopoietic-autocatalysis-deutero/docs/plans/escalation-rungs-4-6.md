# Escalation Rungs 4–6

**Created:** 2026-05-31
**Started:** 2026-05-31 — rung 4 ephemeral provider swap implemented
**Enhanced:** 2026-06-01 — rung 5 explicit clean swapped-session lineage and prompt coverage added
**Completed:** 2026-06-01 — rung 6 bounded provider consensus implemented
**Validated:** 2026-06-04 — deterministic `validate-escalation` harness added and live-smoked with bounded provider timeouts
**Todo:** `todos/escalation-rungs-4-6.md`

## Problem

Rungs 0–3 detect repeated behavioral signatures and progressively strengthen the prompt path:

1. loop awareness;
2. alternate-provider consultation;
3. clean session with failure context stripped.

Live runs showed this detects degradation but does not reliably halt it. A model can keep producing semantically equivalent outputs while still succeeding at the provider/process level, so provider circuit breakers do not fire.

## Goal

Implement mechanical, no-human escalation beyond prompt shaping:

- **Rung 4:** swap to a different provider/model with failure history preserved;
- **Rung 5:** swap to a different provider/model with clean session;
- **Rung 6:** bounded multi-model consensus/portfolio, selected by mechanical fitness when available.

Provider-policy durability remains separate and comparison-gated. Escalation rungs must be ephemeral runtime interventions unless bounded comparison evidence justifies durable policy changes.

## Rung 4 slice — implemented 2026-05-31

Rung 4 is the smallest useful non-prompt intervention:

- add `ProviderRegistry::swapped_provider_for_avoiding()`;
- add `ProviderRegistry::role_isolated_swapped_provider_for_avoiding()` for evolver/role-isolated paths;
- in `Metabolism::invoke_scheduled`, when the internal escalation counter reaches rung 4, route the current invocation to a non-assigned provider;
- do not mutate provider assignments or persisted `provider_policy`;
- preserve `failure_report` at rung 4 so the new model can learn from previous failures;
- skip rung-2 consultation at rung 4+ because the alternative provider is now the primary intervention;
- reset naturally when the loop counter resets after a changed fitness/output signature.

Coverage added:

- rung 4 invokes swapped provider and not primary;
- rung 4 does not fire below threshold;
- changed output resets loop state and next invocation returns to assigned provider;
- rung-4 prompt includes provider-swap notice and preserves failure history;
- provider swap helpers avoid mutating assignments;
- role-isolated swap excludes other-role providers.

## Rung 5 slice — implemented 2026-06-01

Rung 5 is now explicit as the clean-session variant of rung 4:

- provider swap remains active;
- `failure_report` is stripped before request construction;
- invocation lineage records `escalation_rung`, `provider_swap`, and `clean_session`;
- clean-session lineage records provider-visible inputs, so stripped failure context does not appear as if it reached the provider;
- provider-health `recent_invocations` carries the same escalation fields;
- topology lineage output annotates escalated invocations, for example `{rung 5, swap, clean}`.

Coverage added:

- rung 5 invokes the swapped provider and not the primary;
- rung 5 marks swap + clean-session metadata in lineage;
- rung 5 omits `failure_report` from provider-visible lineage inputs;
- rung-5 prompt contains provider-swap and clean-session notices while excluding previous-failure and consultation text;
- topology comparison formatting prints escalation flags;
- provider-health recent invocation JSON includes escalation fields.

Learning: `docs/solutions/architectural-insights/escalation-rung-5-clean-swapped-session-lineage-2026-06-01.md`.

## Rung 6 slice — implemented 2026-06-01

Rung 6 is now a bounded provider portfolio/consensus path:

1. collect role-isolated eligible providers for the enzyme while avoiding cooled-down providers;
2. cap the portfolio with `A2D_RUNG6_MAX_PROVIDERS` (default 3);
3. invoke candidates sequentially to avoid unbounded concurrent provider-window consumption;
4. materialize candidate outputs and record candidate evaluations in lineage;
5. if output includes `code` and a benchmark is attached, pick highest fitness;
6. otherwise use a deterministic fallback: first materialized success, then first success, then first error.

Coverage added:

- rung 6 selects the higher-fitness code candidate under a benchmark;
- rung 6 records candidate evaluations and rung/swap/clean metadata;
- rung 6 works for non-code enzymes by selecting the first materialized success after an earlier provider failure.

Learning: `docs/solutions/architectural-insights/escalation-rung-6-bounded-provider-consensus-2026-06-01.md`.

## Validation

Current validation after the harness:

```text
cargo test
39 CLI tests + 147 core tests + 11 bootstrap + 7 provider + 1 doctest = 205 passing, 2 ignored
```

A bounded live smoke now exists:

```bash
A2D_PROVIDER_TIMEOUT_SECS=1 A2D_MAX_CYCLE_SECS=1 A2D_RUNG6_MAX_PROVIDERS=2 \
  cargo run -q -p a2d -- validate-escalation sudoku coder
```

Observed 2026-06-04: rung 4 routed to DeepSeek with failure history marker visible; rung 5 routed to DeepSeek with clean-session marker stripped; rung 6 invoked a two-provider Kimi + DeepSeek portfolio and recorded two candidate evaluations. Provider calls timed out under the intentional 1s bound, provider policy stayed unchanged, and the JSON report exposed `escalation_rung` without internal counter names.
