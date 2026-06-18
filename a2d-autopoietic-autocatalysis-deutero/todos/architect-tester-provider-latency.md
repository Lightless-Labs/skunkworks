# Architect/Tester Provider Latency

**Created:** 2026-06-05
**Started:** 2026-06-05 — runtime-only tester/architect provider overrides implemented
**Enhanced:** 2026-06-05 — `validate-escalation` can isolate tester/architect with non-empty diagnostic inputs so forced-role smokes reach the intended enzyme
**Validation update:** 2026-06-05 — 30s forced tester comparison produced valid JSON but all candidates timed out; no default provider change justified
**Validation update:** 2026-06-11 — Added direct `compare-role-providers` harness so tester/architect provider assignments can be compared without waiting for coder to succeed. 5s tester and architect runs reached GLM, Kimi, and DeepSeek directly; all timed out, so no default provider change is justified yet.
**Validation update:** 2026-06-13 — 30s direct tester comparisons are noisy: GLM and DeepSeek each succeeded once and timed out once; Kimi timed out twice. A 30s architect comparison produced no successful `system_patch`; Kimi returned quickly but attempted an OpenCode tool read outside the isolated cwd and materialized nothing, while GLM/DeepSeek timed out. No tester/architect default change is justified.
**Hardening:** 2026-06-13 — OpenCode artifact invocations now pass `--pure` and select `--agent a2d-artifact-no-tools` from a cwd-local `opencode.json` with `permission: {"*":"deny"}` to reduce external plugin/session/tool behavior during role-provider comparisons and live metabolism calls.
**Enhanced:** 2026-06-13 — `compare-role-providers` now includes `materialized_output_previews` and patch outcome fields (`accepted_patches`, `rejected_patches`, `noop_patches`, `patch_record`); post-`--pure`/no-tools Kimi architect can produce noop `system_patch` outputs, but treat it as plausible rather than a default-change basis until replicated.
**Enhanced:** 2026-06-14 — Kimi k2.7 code (`opencode/kimi-for-coding/k2p7`), GLM 5.2 (`opencode/zai-coding-plan/glm-5.2`), and provisional Minimax 3 aliases are opt-in registered when named by overrides/comparison commands; defaults remain unchanged.
**Enhanced:** 2026-06-16 — verified Pi model IDs with `pi --list-models` and added opt-in Pi lanes (`pi/kimi-coding/k2p7`, `pi/minimax/MiniMax-M3`, `pi/zai/glm-5.2`) through the same override/comparison auto-registration path; defaults remain unchanged.
**Validation update:** 2026-06-17 — two-replica 60s direct tester/architect comparisons across default OpenCode lanes and verified Pi lanes show timeout variability and preview-quality differences; no default change justified.
**Corrected:** 2026-06-17 — stale OpenCode Kimi k2.7 alias `opencode/kimi-k2.7-code` failed live; corrected opt-in provider name is `opencode/kimi-for-coding/k2p7`.
**Validation update:** 2026-06-17 — Pi-first 90s comparisons make `pi/minimax/MiniMax-M3` the strongest diagnostic architect candidate; prefer Pi for future probes, but require challenge-integrated/provider-policy-gated evidence before defaults change.
**Fixed:** 2026-06-17 — `compare-topologies` seed mode now honors runtime provider overrides while still bypassing persisted lineage policy, so seed/evolved provider comparisons are controlled.
**Validation update:** 2026-06-17 — provider-policy gate rejected durable Pi-Minimax tester/architect policy due invocation/wall-clock cost; keep defaults unchanged.
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
- [ ] Outcome-quality evidence with replicated larger-budget runs or a cheaper prompt/provider remains pending before changing tester/architect defaults. 2026-06-13: two 30s tester runs were inconsistent; post-`--pure` Kimi architect can produce a noop `system_patch`, but timed out on one immediate rerun, and `--pure` alone still allowed tool attempts. No-tools agent hardening produced Kimi noop successes with no captured tool events. 2026-06-14: new OpenCode Kimi k2.7 / GLM 5.2 / Minimax 3 lanes are mechanically probeable but not live-validated. 2026-06-16: Pi Kimi k2.7 / Minimax 3 / GLM 5.2 lanes are now mechanically probeable via `pi/kimi-coding/k2p7`, `pi/minimax/MiniMax-M3`, and `pi/zai/glm-5.2`. 2026-06-17: two-replica 60s direct comparisons found Kimi k2p6 most consistent for architect noop outputs, Kimi/DeepSeek consistent for tester materialization, GLM 5.1 unreliable at 60s, and Pi lanes mixed; however the runs also show timeout variability at the cutoff. A follow-up corrected new-lane smoke found the old OpenCode Kimi k2.7 alias failed (`Model not found: kimi-k2.7-code/.`), while corrected `opencode/kimi-for-coding/k2p7` produced an architect noop and tester output; OpenCode GLM 5.2 still failed architect with no materialized output. Pi-first 90s follow-up: `pi/minimax/MiniMax-M3` produced two fast architect noops and one clear tester report after one tester timeout; `pi/kimi-coding/k2p7` produced tester output twice but architect timed out once; `pi/zai/glm-5.2` timed out under the 90s role probes. A controlled `compare-topologies sudoku 2` run with Pi Minimax tester/architect overrides now applies overrides to both seed and evolved legs after the topology registry fix; both reached 100% best fitness, but GLM entered the coder portfolio once tester/architect moved to Pi, so this is encouraging rather than conclusive. Prefer Pi for future probes, with Minimax as the leading architect candidate, but keep defaults unchanged: `compare-provider-policy sudoku 2` rejected Pi-Minimax tester/architect policy after a full-policy rerun with fitness +0pp, +2 invocations, and +243.5s wall-clock. 2026-06-18: `compare-role-providers` now has `--replicas N` and per-result `replica` fields, so future replicated comparisons should produce a single structured JSON artifact instead of manual `r1`/`r2` files. Inspect `materialized_output_previews` and patch outcome fields; require replication before changing defaults.

## Notes

Use environment variables for experiments:

```bash
A2D_TESTER_PROVIDER=opencode/kimi-for-coding/k2p6
A2D_ARCHITECT_PROVIDER=opencode/kimi-for-coding/k2p6
# New opt-in lanes:
A2D_TESTER_PROVIDER=opencode/kimi-for-coding/k2p7
A2D_ARCHITECT_PROVIDER=opencode/zai-coding-plan/glm-5.2
```

Direct comparison can also name provisional Minimax 3 aliases, for example `opencode/minimax-coding-plan/MiniMax-3`. If that alias fails invocation, try the other recognized aliases documented in `docs/plans/architect-tester-provider-latency.md`; do not make it a default without replicated outcome evidence.

Pi note: prefer Pi-backed lanes when practical. Explicitly registered opt-in provider names are `pi/kimi-coding/k2p7`, `pi/minimax/MiniMax-M3`, and `pi/zai/glm-5.2`; other `pi/<model>` names still need `pi --list-models` verification and tests before use.

Next empirical slice: prefer Pi and use corrected new-lane names, but search for a lower-cost Pi path or a challenge where Pi role quality offsets its latency. If proposing a durable tester/architect policy, use `compare-provider-policy` or another provider-policy-gated path rather than direct defaults; account for GLM becoming an unassigned coder candidate when tester/architect move to Pi. Options: add more replicas, try timeout buckets (for example 60s vs 90s), or run challenge-integrated evidence for the most promising roles. Compare current/default-ish lanes against the verified Pi lanes for both `tester` and `architect`, for example:

```bash
A2D_PROVIDER_TIMEOUT_SECS=60 A2D_MAX_CYCLE_SECS=90 \
  cargo run -q -p a2d -- compare-role-providers sudoku architect --replicas 2 \
  opencode/zai-coding-plan/glm-5.1 \
  opencode/kimi-for-coding/k2p6 \
  opencode/opencode/deepseek-v4-flash-free \
  pi/kimi-coding/k2p7 \
  pi/minimax/MiniMax-M3 \
  pi/zai/glm-5.2
```

Repeat for `tester` and use `--replicas N` for repeated measurements in a single JSON artifact. Rank by `outcome`, `failed`, elapsed time, `materialized_output_previews`, and patch outcome fields, not `assignment_accepted`. Do not change defaults or write these to lineage unless replicated evidence justifies it and the existing provider-policy comparison gate accepts a proposed durable policy.

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
  - Historical one-shot command before `--replicas`: `A2D_PROVIDER_TIMEOUT_SECS=5 A2D_MAX_CYCLE_SECS=10 cargo run -q -p a2d -- compare-role-providers sudoku <tester|architect> opencode/zai-coding-plan/glm-5.1 opencode/kimi-for-coding/k2p6 opencode/opencode/deepseek-v4-flash-free`
  - Result: GLM, Kimi, and DeepSeek were assigned directly and invoked for the intended role; every candidate timed out at ~5.1s. The JSON field `assignment_accepted` only means the provider assignment was accepted; rank by `outcome`, `failed`, and elapsed time.
- 30s direct role-provider comparisons:
  - Tester run 1: `/tmp/a2d-compare-role-providers-tester-30s-20260613.json` — GLM succeeded in 15.2s, DeepSeek succeeded in 11.2s, Kimi timed out.
  - Tester run 2: `/tmp/a2d-compare-role-providers-tester-30s-20260613-r2.json` — all three timed out. Treat run 1 as provider variance, not default-change evidence.
  - Architect: `/tmp/a2d-compare-role-providers-architect-30s-20260613.json` — GLM and DeepSeek timed out; Kimi failed fast with no materialized `system_patch`, and raw stdout showed an attempted OpenCode tool read of `/Users/thomas/.claude/CLAUDE.md` rejected under the intentionally isolated provider cwd.
  - Learning: `docs/solutions/best-practices/role-provider-comparisons-must-account-for-isolated-cwd-2026-06-13.md`.
  - Follow-up hardening: `docs/solutions/runtime-bugs/opencode-pure-mode-for-artifact-roles-2026-06-13.md`; OpenCode provider calls now include `--pure` and a no-tools artifact agent.
- Post-`--pure` architect comparisons:
  - `/tmp/a2d-compare-role-providers-architect-30s-post-pure-20260613.json` — GLM timed out, Kimi produced `system_patch` in 14.4s, DeepSeek timed out.
  - `/tmp/a2d-compare-role-providers-architect-30s-post-pure-kimi-r2-20260613.json` — Kimi timed out.
  - `/tmp/a2d-compare-role-providers-architect-30s-post-pure-preview-kimi-20260613.json` — after output previews were added, Kimi produced a noop `system_patch` in 18.1s; preview says no source changes were warranted for the diagnostic marker.
  - `/tmp/a2d-compare-role-providers-architect-60s-post-preview-kimi-20260613.json` — `--pure` alone still allowed tool attempts against the empty temp cwd and failed with no materialized output.
  - `/tmp/a2d-compare-role-providers-architect-30s-no-tools-kimi-20260613.json` — after no-tools agent hardening, Kimi produced a noop `system_patch` in 15.8s and captured artifacts contained no `tool_use`/tool events.
  - `/tmp/a2d-compare-role-providers-architect-30s-no-tools-patchfields-kimi-20260613.json` — after patch outcome fields were added, Kimi produced a noop `system_patch` in 5.6s with `accepted_patches: 0`, `rejected_patches: 0`, `noop_patches: 1`, and stable `patch_record.noops` detail.

