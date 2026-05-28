# Provider Policy Artifact

**Created:** 2026-05-22
**Enhanced:** 2026-05-23 — provider policy lineage persistence
**Addendum:** 2026-05-28 — durability now routes through the bounded provider-policy topology gate; live runtime proposal validated rejection without fitness evidence

## Goal

Make provider-role assignment a typed, gated mechanism rather than an operator-only Rust edit.

The previous slice routed `provider_health_report` into the metabolism. This plan closes the next edge:

```text
provider_health_report → provider_policy proposal → mechanical gate → active ProviderRegistry policy → lineage-visible artifact
```

## Minimal slice

1. Define a typed `provider_policy` artifact schema:
   - `assignments`: map of enzyme id → registered provider name.
2. Expose the active `ProviderRegistry` policy as a typed artifact.
3. Allow any enzyme that produces `provider_policy` to propose assignment changes.
4. Gate proposals mechanically before application:
   - target enzyme must exist in the current germline;
   - provider name must be registered;
   - malformed JSON is rejected without changing the registry.
5. Record accepted/rejected provider policy changes in invocation lineage and cycle summaries.
6. Include current provider policy in evolver/architect prompts when available.

## Explicit non-goals for the first slice

- No automatic bounded topology-comparison gate yet.
- No default extra policy-management enzyme in the live germline yet; provider latency is already a bottleneck.

## Persistence slice

Accepted provider policy is now persisted in the lineage archive as `provider-policy.json` beside `germline.json`.

Rules:

1. Normal runtime loads `provider-policy.json` when present and mechanically reapplies it to the default live provider registry.
2. `A2D_GERMLINE=seed` bypasses lineage provider policy, preserving seed-mode comparisons.
3. `compare-topologies` keeps seed on hardcoded defaults and lets evolved topology load lineage policy, so the evolved system can include both topology and provider-policy lineage.
4. Runtime commits accepted provider-policy changes through `LineageArchive::commit_provider_policy` only after the cycle did not regress and the bounded provider-policy topology gate accepts the current-vs-proposed comparison.

## Follow-ups

- Add a lightweight provider-policy enzyme only after bounded tests show it does not starve coder/feedback metabolism.
- Consider repeated bounded topology comparisons before making high-impact provider-policy changes durable defaults; the first bounded current-vs-proposed gate is implemented and live-validated.
- Consider a combined lineage commit for cycles that change both germline and provider policy, rather than two sequential lineage commits.
