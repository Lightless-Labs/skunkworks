# Architect/Tester Provider Latency

**Created:** 2026-06-05
**Started:** 2026-06-05 — runtime-only tester/architect provider overrides implemented
**Enhanced:** 2026-06-05 — `validate-escalation` can isolate tester/architect with non-empty diagnostic inputs so forced-role smokes reach the intended enzyme
**Validation update:** 2026-06-05 — 30s forced tester comparison produced valid JSON but all candidates timed out; no default provider change justified
**Validation update:** 2026-06-11 — Added direct `compare-role-providers` harness so tester/architect provider assignments can be compared without waiting for coder to succeed. 5s tester and architect runs reached GLM, Kimi, and DeepSeek directly; all timed out, so no default provider change is justified yet.
**Validation update:** 2026-06-13 — 30s direct tester comparisons are noisy: GLM and DeepSeek each succeeded once and timed out once; Kimi timed out twice. A 30s architect comparison produced no successful `system_patch`; Kimi returned quickly but attempted an OpenCode tool read outside the isolated cwd and materialized nothing, while GLM/DeepSeek timed out. No tester/architect default change is justified.
**Hardening:** 2026-06-13 — OpenCode artifact invocations now pass `--pure` to reduce external plugin/session behavior during role-provider comparisons and live metabolism calls. Re-run architect comparisons before interpreting pre-`--pure` Kimi architect failure as model quality.
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
- [ ] Outcome-quality evidence with replicated larger-budget runs or a cheaper prompt/provider remains pending before changing tester/architect defaults. 2026-06-13: two 30s tester runs were inconsistent, and architect failures mixed provider timeout with isolated-cwd/tool-use behavior. OpenCode now runs with `--pure`; repeat architect comparison before making decisions from the old artifact.

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
- 30s direct role-provider comparisons:
  - Tester run 1: `/tmp/a2d-compare-role-providers-tester-30s-20260613.json` — GLM succeeded in 15.2s, DeepSeek succeeded in 11.2s, Kimi timed out.
  - Tester run 2: `/tmp/a2d-compare-role-providers-tester-30s-20260613-r2.json` — all three timed out. Treat run 1 as provider variance, not default-change evidence.
  - Architect: `/tmp/a2d-compare-role-providers-architect-30s-20260613.json` — GLM and DeepSeek timed out; Kimi failed fast with no materialized `system_patch`, and raw stdout showed an attempted OpenCode tool read of `/Users/thomas/.claude/CLAUDE.md` rejected under the intentionally isolated provider cwd.
  - Learning: `docs/solutions/best-practices/role-provider-comparisons-must-account-for-isolated-cwd-2026-06-13.md`.
  - Follow-up hardening: `docs/solutions/runtime-bugs/opencode-pure-mode-for-artifact-roles-2026-06-13.md`; OpenCode provider calls now include `--pure`.

