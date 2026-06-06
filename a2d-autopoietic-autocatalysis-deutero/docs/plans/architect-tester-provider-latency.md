# Architect/Tester Provider Latency

**Created:** 2026-06-05
**Started:** 2026-06-05 — runtime-only tester/architect provider overrides implemented and smoked through registry-building validation path
**Enhanced:** 2026-06-05 — forced tester/architect validation now uses a validation-only single-enzyme germline and non-empty diagnostic inputs
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
- Optional bounded comparison smoke for Kimi/DeepSeek tester/architect assignment if provider budget allows. **Still pending:** `compare-topologies sudoku 2` with 20s bounds did not reach tester/architect because coder timed out first; outcome evidence still needs a seeded/direct role comparison or a successful coder cycle.
