# Architect/Tester Provider Latency

**Created:** 2026-06-05
**Started:** 2026-06-05 — runtime-only tester/architect provider overrides implemented and smoked through registry-building validation path
**Enhanced:** 2026-06-05 — forced tester/architect validation now uses a validation-only single-enzyme germline and non-empty diagnostic inputs
**Enhanced:** 2026-06-11 — added direct `compare-role-providers` harness for tester/architect provider assignment comparisons without waiting for coder to succeed
**Reviewed:** 2026-06-13 — ran repeated 30s direct role-provider comparisons; tester results were noisy and architect results were confounded by OpenCode isolated-cwd/tool behavior, so defaults remain unchanged
**Hardened:** 2026-06-13 — OpenCode provider invocations now include `--pure` and select a cwd-local no-tools artifact agent to reduce external plugin/session/tool behavior during artifact-role calls
**Enhanced:** 2026-06-13 — role-provider comparison JSON now includes `materialized_output_previews` so successful architect outputs can be inspected
**Todo:** `todos/architect-tester-provider-latency.md`

## Problem

A²D removed GLM from coder/evolver critical paths, but tester and architect remain assigned to GLM 5.1. Live runs still show architect/tester windows consuming bounded cycles through timeouts or slow failures. The 2026-06-05 rung-6 broad-scope smoke also showed that adding slow other-role providers can spend extra timeout windows without proving outcome value.

Current default registry:

- coder/evolver/default: `opencode/kimi-for-coding/k2p6`;
- coder race fallback: `opencode/opencode/deepseek-v4-flash-free`;
- tester/architect: `opencode/zai-coding-plan/glm-5.1`;
- maintainer: `pi/default`.

## Goal

Make architect/tester latency experiments safe, bounded, and inspectable without changing defaults blindly.

## First Slice — implemented 2026-06-05

Add non-durable runtime assignment overrides for tester and architect:

- `A2D_TESTER_PROVIDER=<registered-provider-name>`;
- `A2D_ARCHITECT_PROVIDER=<registered-provider-name>`.

The override must:

1. accept only already registered provider names;
2. affect only runtime registry assignment, not lineage `provider-policy.json`;
3. leave defaults unchanged when unset;
4. make rejected override names visible on stderr;
5. preserve the existing coder race by default.

Overrides are applied after lineage provider policy loading so they are truly runtime experiment overrides, not durable lineage changes.

This gives controlled commands such as:

```bash
A2D_TESTER_PROVIDER=opencode/kimi-for-coding/k2p6 \
A2D_ARCHITECT_PROVIDER=opencode/kimi-for-coding/k2p6 \
A2D_PROVIDER_TIMEOUT_SECS=30 A2D_MAX_CYCLE_SECS=120 \
  cargo run -q -p a2d -- compare-topologies sudoku 1
```

The direct role-comparison harness added later is preferred when coder timeouts would prevent tester/architect from being reached:

```bash
A2D_PROVIDER_TIMEOUT_SECS=5 A2D_MAX_CYCLE_SECS=10 \
  cargo run -q -p a2d -- compare-role-providers sudoku tester \
  opencode/zai-coding-plan/glm-5.1 \
  opencode/kimi-for-coding/k2p6 \
  opencode/opencode/deepseek-v4-flash-free
```

## Non-goals for first slice

- Do not promote broad rung-6 eligibility.
- Do not register unverified Pi model IDs; `pi --help` exceeded a 30s probe window in this session, so Pi Kimi/Minimax IDs need separate verification.
- Do not persist provider assignment changes; durable policy remains comparison-gated.

## Validation

- Unit tests for default registry and override application. **Done 2026-06-05:** `runtime_provider_overrides_*` tests cover valid overrides, invalid provider rejection, and non-experimental role rejection without process-global env mutation.
- `cargo test`. **Done 2026-06-05:** 211 passing, 2 ignored after diagnostic validation isolation test.
- Bounded smoke with invalid override to verify rejection is visible and defaults remain usable. **Done 2026-06-05:** `validate-escalation` with `A2D_TESTER_PROVIDER=missing` printed a visible rejection three times (one fresh registry per forced rung) and completed JSON output.
- Bounded smoke with valid override to verify assignment messages. **Done 2026-06-05:** `validate-escalation` with tester+architect set to Kimi printed accepted override messages for both roles.
- Forced-role validation. **Done 2026-06-05:** `validate-escalation sudoku tester` and `validate-escalation sudoku architect` now isolate the target enzyme and seed non-empty inputs so the intended role is invoked directly. 10s smokes reached the target roles but timed out, so they are mechanism evidence only.
- Optional bounded comparison smoke for Kimi/DeepSeek tester/architect assignment if provider budget allows. **Partially addressed 2026-06-11:** added `a2d compare-role-providers <challenge> <enzyme> [providers...]`, which builds validation-only single-enzyme runs and applies one provider assignment per run with persistence disabled. 5s tester and architect smokes reached GLM, Kimi, and DeepSeek directly; all timed out, so no default change is justified yet. **Updated 2026-06-13:** two 30s tester runs were noisy (`GLM success/timeout`, `DeepSeek success/timeout`, `Kimi timeout/timeout`), and a 30s architect run produced no successful `system_patch`. Kimi architect returned quickly but attempted an OpenCode tool read outside the isolated provider cwd, so that failure is a provider-mode/harness interaction as much as a model-quality signal. Outcome evidence still needs replicated larger-budget runs or a cheaper prompt/provider.

## 2026-06-13 comparison artifacts

```bash
A2D_PROVIDER_TIMEOUT_SECS=30 A2D_MAX_CYCLE_SECS=45 \
  cargo run -q -p a2d -- compare-role-providers sudoku tester \
  opencode/zai-coding-plan/glm-5.1 \
  opencode/kimi-for-coding/k2p6 \
  opencode/opencode/deepseek-v4-flash-free
```

Artifacts:

- `/tmp/a2d-compare-role-providers-tester-30s-20260613.json`
- `/tmp/a2d-compare-role-providers-tester-30s-20260613-r2.json`
- `/tmp/a2d-compare-role-providers-architect-30s-20260613.json`

Result: no provider-default change. Tester success at 30s was not replicated; architect comparison did not produce a valid `system_patch`; and isolated-cwd/tool-use behavior must be accounted for when interpreting OpenCode architect failures. Documented learning: `docs/solutions/best-practices/role-provider-comparisons-must-account-for-isolated-cwd-2026-06-13.md`.

Follow-up hardening: `CliProvider::opencode` now passes `--pure` to `opencode run`, with unit coverage and full `cargo test` validation. Documented learning: `docs/solutions/runtime-bugs/opencode-pure-mode-for-artifact-roles-2026-06-13.md`.

Post-`--pure` architect checks:

- `/tmp/a2d-compare-role-providers-architect-30s-post-pure-20260613.json` — GLM timed out, Kimi materialized `system_patch` in 14.4s, DeepSeek timed out.
- `/tmp/a2d-compare-role-providers-architect-30s-post-pure-kimi-r2-20260613.json` — Kimi timed out.
- `/tmp/a2d-compare-role-providers-architect-30s-post-pure-preview-kimi-20260613.json` — after adding output previews, Kimi materialized a noop `system_patch` in 18.1s; preview says the diagnostic marker looked false-positive and no source changes were warranted.

Result: Kimi is a plausible post-`--pure` architect candidate, but still flaky under 30s. `--pure` alone did not fully prevent tool behavior; a later 60s Kimi run emitted `tool_use` events against the empty temp cwd and failed. OpenCode provider calls now also select `--agent a2d-artifact-no-tools` from a cwd-local `opencode.json` with `permission: {"*":"deny"}`. A direct temp-cwd probe verified the agent is discovered, and `/tmp/a2d-compare-role-providers-architect-30s-no-tools-kimi-20260613.json` produced a noop `system_patch` in 15.8s with no captured tool events. No durable/default provider change without replicated outcome evidence.
