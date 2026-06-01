# Escalation Rungs 4–6

**Created:** 2026-05-31
**Started:** 2026-05-31 — rung 4 ephemeral provider swap implemented
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
- in `Metabolism::invoke_scheduled`, when `enzyme_loop_count >= 4`, route the current invocation to a non-assigned provider;
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

## Rung 5 next

Rung 5 should be the explicit clean-session variant of rung 4:

- provider swap remains active;
- `failure_report` is stripped;
- prompt should state that the new model is solving from scratch;
- tests should assert both swapped provider selection and absence of failure context.

Current code already applies `clean_session` for `loop_rung >= 5`, but this still needs explicit rung-5 tests and lineage/prompt clarity before considering the rung complete.

## Rung 6 next

Rung 6 should be a bounded provider portfolio/consensus path:

1. collect eligible providers for the enzyme;
2. invoke them sequentially or with bounded concurrency;
3. materialize candidate outputs;
4. if output includes `code` and a benchmark is attached, pick highest fitness;
5. otherwise record candidates and use a deterministic fallback selection rule;
6. record candidate evaluations in lineage.

The existing coder portfolio is a useful precedent, but rung 6 should be generalized and bounded for non-coder enzymes.

## Validation

Current validation after rung 4:

```text
cargo test
36 CLI tests + 140 core tests + 11 bootstrap + 7 provider + 1 doctest = 195 passing, 2 ignored
```

Before live provider validation, prefer a deterministic harness or short bounded run that forces `enzyme_loop_count = 4` without waiting for natural provider repetition.
