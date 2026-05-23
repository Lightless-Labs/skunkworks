# Provider Policy Artifact

**Created:** 2026-05-22

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

## Explicit non-goals for this slice

- No automatic bounded topology-comparison gate yet.
- No default extra policy-management enzyme in the live germline yet; provider latency is already a bottleneck.
- No lineage archive file for policy yet; accepted changes are lineage-visible through cycle reports and the `provider_policy` artifact, with archive persistence as a follow-up.

## Follow-ups

- Add a lightweight provider-policy enzyme only after bounded tests show it does not starve coder/feedback metabolism.
- Persist accepted policy in the lineage archive alongside `germline.json`.
- Gate provider-policy changes with repeated bounded topology comparisons before making them durable defaults.
