# Architect/Tester Provider Latency

**Created:** 2026-06-05
**Started:** 2026-06-05 — runtime-only tester/architect provider overrides implemented
**Enhanced:** 2026-06-05 — `validate-escalation` can isolate tester/architect with non-empty diagnostic inputs so forced-role smokes reach the intended enzyme
**Validation update:** 2026-06-05 — 30s forced tester comparison produced valid JSON but all candidates timed out; no default provider change justified
**Validation update:** 2026-06-11 — Added direct `compare-role-providers` harness so tester/architect provider assignments can be compared without waiting for coder to succeed. 5s tester and architect runs reached GLM, Kimi, and DeepSeek directly; all timed out, so no default provider change is justified yet.
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
- [x] A direct bounded smoke documents whether a faster tester/architect assignment reduces timeout waste under a small budget. `compare-role-providers sudoku tester ...` and `compare-role-providers sudoku architect ...` with 5s provider bounds invoked GLM, Kimi, and DeepSeek directly; all candidates timed out at ~5.1s with `failed: 1`, so there is no evidence to change defaults.
- [ ] Outcome-quality evidence with a larger bounded budget or cheaper prompt/provider remains pending before changing tester/architect defaults.

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
- 30s forced tester comparison:
  - Default: `/tmp/a2d-validate-tester-default-30s-20260605.json`
  - Kimi override: `/tmp/a2d-validate-tester-kimi-30s-20260605.json`
  - Stderr inspected; JSON parsed successfully for both runs.
  - Result: all candidates timed out after 30s, so this is still no evidence for changing tester defaults.
- 5s direct role-provider comparison harness:
  - Tester: `/tmp/a2d-compare-role-providers-tester-5s-20260611-v2.json`
  - Architect: `/tmp/a2d-compare-role-providers-architect-5s-20260611-v2.json`
  - Command: `A2D_PROVIDER_TIMEOUT_SECS=5 A2D_MAX_CYCLE_SECS=10 cargo run -q -p a2d -- compare-role-providers sudoku <tester|architect> opencode/zai-coding-plan/glm-5.1 opencode/kimi-for-coding/k2p6 opencode/opencode/deepseek-v4-flash-free`
  - Result: GLM, Kimi, and DeepSeek were assigned directly and invoked for the intended role; every candidate timed out at ~5.1s. The JSON field `assignment_accepted` only means the provider assignment was accepted; rank by `outcome`, `failed`, and elapsed time.

