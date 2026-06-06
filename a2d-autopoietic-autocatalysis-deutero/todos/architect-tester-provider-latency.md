# Architect/Tester Provider Latency

**Created:** 2026-06-05
**Started:** 2026-06-05 — runtime-only tester/architect provider overrides implemented
**Enhanced:** 2026-06-05 — `validate-escalation` can isolate tester/architect with non-empty diagnostic inputs so forced-role smokes reach the intended enzyme
**Plan:** `docs/plans/architect-tester-provider-latency.md`
**Depends on:** provider circuit breaker, provider-policy topology gate, rung-6 scope probe.

## Problem

Tester and architect still default to GLM 5.1. GLM is off coder/evolver critical paths, but live evidence shows architect/tester provider windows can still dominate bounded runs. The rung-6 broad-scope smoke also showed extra GLM/Pi windows are a real cost when not backed by outcome evidence.

## Acceptance criteria

- [x] Runtime-only provider overrides exist for tester and architect. Implemented via `A2D_TESTER_PROVIDER` and `A2D_ARCHITECT_PROVIDER`.
- [x] Overrides accept only registered provider names.
- [x] Invalid overrides are rejected visibly without changing defaults.
- [x] Defaults remain unchanged when no override is set.
- [x] Override behavior is unit-tested without relying on process-global env mutation.
- [x] `cargo test` passes. 2026-06-05: 211 passing, 2 ignored after diagnostic validation isolation test.
- [x] Mechanism smoke exercises a command path that actually builds the runtime registry. 2026-06-05: `validate-escalation` invalid/valid override smokes passed; earlier `status` probe was discarded because `status` does not build the registry.
- [x] Forced-role validation can reach tester/architect directly. 2026-06-05: `validate-escalation sudoku tester` and `validate-escalation sudoku architect` use a validation-only single-enzyme germline plus non-empty seeded inputs.
- [ ] A bounded smoke documents whether a faster tester/architect assignment reduces timeout waste. Attempted `compare-topologies sudoku 2` with 20s bounds did not reach tester/architect because coder timed out first; this remains pending.

## Notes

Use environment variables for experiments:

```bash
A2D_TESTER_PROVIDER=opencode/kimi-for-coding/k2p6
A2D_ARCHITECT_PROVIDER=opencode/kimi-for-coding/k2p6
```

Do not write these to lineage unless the existing provider-policy comparison gate accepts a proposed durable policy.

## Validation notes

- Invalid override smoke:
  - Command shape: `A2D_TESTER_PROVIDER=missing A2D_PROVIDER_TIMEOUT_SECS=1 A2D_MAX_CYCLE_SECS=1 A2D_RUNG6_MAX_PROVIDERS=1 cargo run -q -p a2d -- validate-escalation sudoku coder`
  - Stderr artifact: `/tmp/a2d-invalid-tester-provider-override-validate-20260605.err`
  - Result: visible `Rejected runtime provider override for tester -> missing: provider is not registered` for each fresh validation registry.
- Valid override smoke:
  - Command shape: `A2D_TESTER_PROVIDER=opencode/kimi-for-coding/k2p6 A2D_ARCHITECT_PROVIDER=opencode/kimi-for-coding/k2p6 ... validate-escalation sudoku coder`
  - Stderr artifact: `/tmp/a2d-valid-tester-architect-provider-override-validate-20260605.err`
  - Result: visible runtime override messages for tester and architect.
- Forced-role smokes:
  - Tester default: `/tmp/a2d-validate-tester-default-20260605.json`
  - Tester Kimi override: `/tmp/a2d-validate-tester-kimi-20260605.json`
  - Architect Kimi override: `/tmp/a2d-validate-architect-kimi-20260605.json`
  - Result: intended enzymes were invoked directly; 10s provider bound was too tight for quality conclusions.

