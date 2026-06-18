# Architect/Tester Provider Latency

**Created:** 2026-06-05
**Started:** 2026-06-05 — runtime-only tester/architect provider overrides implemented and smoked through registry-building validation path
**Enhanced:** 2026-06-05 — forced tester/architect validation now uses a validation-only single-enzyme germline and non-empty diagnostic inputs
**Enhanced:** 2026-06-11 — added direct `compare-role-providers` harness for tester/architect provider assignment comparisons without waiting for coder to succeed
**Reviewed:** 2026-06-13 — ran repeated 30s direct role-provider comparisons; tester results were noisy and architect results were confounded by OpenCode isolated-cwd/tool behavior, so defaults remain unchanged
**Hardened:** 2026-06-13 — OpenCode provider invocations now include `--pure` and select a cwd-local no-tools artifact agent to reduce external plugin/session/tool behavior during artifact-role calls
**Enhanced:** 2026-06-13 — role-provider comparison JSON now includes `materialized_output_previews` plus explicit patch outcome fields so successful architect outputs can be inspected and separated from accepted/rejected/noop patch outcomes
**Enhanced:** 2026-06-14 — Kimi k2.7 code, GLM 5.2, and provisional Minimax 3 aliases are opt-in registered when named by overrides/comparison commands; defaults remain unchanged
**Enhanced:** 2026-06-16 — verified Pi model IDs with `pi --list-models` and added opt-in Pi lanes (`pi/kimi-coding/k2p7`, `pi/minimax/MiniMax-M3`, `pi/zai/glm-5.2`) through the same override/comparison auto-registration path; defaults remain unchanged
**Validation update:** 2026-06-17 — ran two-replica 60s direct tester/architect comparisons across default OpenCode lanes and verified Pi lanes; results show timeout variability and preview-quality differences, not enough evidence for default changes
**Corrected:** 2026-06-17 — replaced stale OpenCode Kimi k2.7 alias `opencode/kimi-k2.7-code` with listed model `opencode/kimi-for-coding/k2p7`; one corrected-lane smoke proves mechanical invocation but not default-change quality
**Validation update:** 2026-06-17 — Pi-first 90s direct comparisons make Pi Minimax the strongest architect candidate in the diagnostic harness; prefer Pi for future probes, but do not persist defaults without challenge-integrated/provider-policy-gated evidence
**Fixed:** 2026-06-17 — `compare-topologies` seed mode now honors runtime provider overrides while bypassing persisted lineage policy, so seed/evolved provider experiments are controlled
**Validation update:** 2026-06-17 — provider-policy gate rejected durable Pi-Minimax tester/architect policy due invocation/wall-clock cost despite Pi preference
**Enhanced:** 2026-06-18 — `compare-role-providers` now accepts `--replicas N`, labels every result with `replica`, and emits a top-level per-provider `summary`, so replicated evidence can be gathered and compared in one mechanically structured JSON artifact instead of separate ad hoc files
**Todo:** `todos/architect-tester-provider-latency.md`

## Problem

A²D removed GLM from coder/evolver critical paths, but tester and architect remain assigned to GLM 5.1. Live runs still show architect/tester windows consuming bounded cycles through timeouts or slow failures. The 2026-06-05 rung-6 broad-scope smoke also showed that adding slow other-role providers can spend extra timeout windows without proving outcome value.

Current default registry:

- coder/evolver/default: `opencode/kimi-for-coding/k2p6`;
- coder race fallback: `opencode/opencode/deepseek-v4-flash-free`;
- tester/architect: `opencode/zai-coding-plan/glm-5.1`;
- maintainer: `pi/default`.

Newly available OpenCode experimental lanes are intentionally not in the default registry portfolio. A²D auto-registers these names only when an override, loaded/provider-comparison policy, or direct role comparison names them:

- `opencode/kimi-for-coding/k2p7`;
- `opencode/zai-coding-plan/glm-5.2`;
- provisional Minimax 3 aliases: `opencode/minimax-coding-plan/MiniMax-3`, `opencode/minimax-coding-plan/Minimax-3`, `opencode/minimax-coding-plan/MiniMax-M3`.

Operator preference is to favor Pi over OpenCode where practical. A²D still registers only `pi/default` in the default runtime portfolio for the outer-loop `maintainer`, but verified Pi-backed model lanes can now be auto-registered when explicitly named by overrides, loaded/provider-comparison policy, or direct role comparison:

- `pi/kimi-coding/k2p7`;
- `pi/minimax/MiniMax-M3`;
- `pi/zai/glm-5.2`.

These IDs were verified with `pi --list-models <kimi|minimax|glm> --offline` on 2026-06-16. They are not default tester/architect assignments and do not enter coder races or broad rung-6 scope unless named.

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

The direct role-comparison harness added later is preferred when coder timeouts would prevent tester/architect from being reached. Use `--replicas N` to gather repeated measurements in one structured JSON artifact; every result includes a `replica` field:

```bash
A2D_PROVIDER_TIMEOUT_SECS=5 A2D_MAX_CYCLE_SECS=10 \
  cargo run -q -p a2d -- compare-role-providers sudoku tester --replicas 2 \
  opencode/zai-coding-plan/glm-5.1 \
  opencode/kimi-for-coding/k2p6 \
  opencode/opencode/deepseek-v4-flash-free
```

Opt-in new-lane comparison example:

```bash
A2D_PROVIDER_TIMEOUT_SECS=30 A2D_MAX_CYCLE_SECS=45 \
  cargo run -q -p a2d -- compare-role-providers sudoku architect --replicas 2 \
  opencode/zai-coding-plan/glm-5.1 \
  opencode/zai-coding-plan/glm-5.2 \
  opencode/kimi-for-coding/k2p6 \
  opencode/kimi-for-coding/k2p7 \
  opencode/minimax-coding-plan/MiniMax-3 \
  pi/kimi-coding/k2p7 \
  pi/minimax/MiniMax-M3 \
  pi/zai/glm-5.2
```

## Non-goals for first slice

- Do not promote broad rung-6 eligibility.
- Do not promote unverified Pi model IDs beyond the verified opt-in lanes (`pi/kimi-coding/k2p7`, `pi/minimax/MiniMax-M3`, `pi/zai/glm-5.2`); additional Pi provider names need `pi --list-models` verification and explicit tests before registration.
- Do not persist provider assignment changes; durable policy remains comparison-gated.

## Validation

- Unit tests for default registry and override application. **Done 2026-06-05:** `runtime_provider_overrides_*` tests cover valid overrides, invalid provider rejection, and non-experimental role rejection without process-global env mutation.
- `cargo test`. **Done 2026-06-05:** 211 passing, 2 ignored after diagnostic validation isolation test.
- Bounded smoke with invalid override to verify rejection is visible and defaults remain usable. **Done 2026-06-05:** `validate-escalation` with `A2D_TESTER_PROVIDER=missing` printed a visible rejection three times (one fresh registry per forced rung) and completed JSON output.
- Bounded smoke with valid override to verify assignment messages. **Done 2026-06-05:** `validate-escalation` with tester+architect set to Kimi printed accepted override messages for both roles.
- Forced-role validation. **Done 2026-06-05:** `validate-escalation sudoku tester` and `validate-escalation sudoku architect` now isolate the target enzyme and seed non-empty inputs so the intended role is invoked directly. 10s smokes reached the target roles but timed out, so they are mechanism evidence only.
- Optional bounded comparison smoke for Kimi/DeepSeek tester/architect assignment if provider budget allows. **Partially addressed 2026-06-11:** added `a2d compare-role-providers <challenge> <enzyme> [providers...]`, which builds validation-only single-enzyme runs and applies one provider assignment per run with persistence disabled. 5s tester and architect smokes reached GLM, Kimi, and DeepSeek directly; all timed out, so no default change is justified yet. **Updated 2026-06-13:** two 30s tester runs were noisy (`GLM success/timeout`, `DeepSeek success/timeout`, `Kimi timeout/timeout`), and a 30s architect run produced no successful `system_patch`. Kimi architect returned quickly but attempted an OpenCode tool read outside the isolated provider cwd, so that failure is a provider-mode/harness interaction as much as a model-quality signal. **Updated 2026-06-18:** current command shape is `a2d compare-role-providers <challenge> <enzyme> [--replicas N] [providers...]`; use `--replicas N` for future replicated comparisons instead of separate manual run files. Outcome evidence still needs replicated larger-budget runs or a cheaper prompt/provider.

## 2026-06-13 comparison artifacts

```bash
A2D_PROVIDER_TIMEOUT_SECS=30 A2D_MAX_CYCLE_SECS=45 \
  cargo run -q -p a2d -- compare-role-providers sudoku tester --replicas 2 \
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

Result: Kimi is a plausible post-`--pure` architect candidate, but still flaky under 30s. `--pure` alone did not fully prevent tool behavior; a later 60s Kimi run emitted `tool_use` events against the empty temp cwd and failed. OpenCode provider calls now also select `--agent a2d-artifact-no-tools` from a cwd-local `opencode.json` with `permission: {"*":"deny"}`. A direct temp-cwd probe verified the agent is discovered, and `/tmp/a2d-compare-role-providers-architect-30s-no-tools-kimi-20260613.json` produced a noop `system_patch` in 15.8s with no captured tool events. Follow-up `/tmp/a2d-compare-role-providers-architect-30s-no-tools-patchfields-kimi-20260613.json` confirmed the new JSON distinguishes provider materialization from patch outcome: `outcome: success: 1 output(s)`, `accepted_patches: 0`, `rejected_patches: 0`, `noop_patches: 1`, and `patch_record.noops` carries the no-op reason. No durable/default provider change without replicated outcome evidence.

## 2026-06-16 Pi lane verification

Verified model IDs with `pi --list-models <kimi|minimax|glm> --offline`; transcript saved at `/tmp/a2d-pi-list-models-kimi-minimax-glm-20260616.txt`. Registered only the verified opt-in A²D provider names:

- `pi/kimi-coding/k2p7`
- `pi/minimax/MiniMax-M3`
- `pi/zai/glm-5.2`

A 1s direct architect comparison smoke confirmed the actual runtime override/comparison registry path accepts all three names and invokes Pi with the expected lineage provider before timing out under the intentionally tiny budget: `/tmp/a2d-compare-role-providers-architect-pi-lanes-1s-20260616.json`. This is mechanism evidence only, not outcome quality evidence.

## 2026-06-17 controlled Pi-Minimax topology comparison

A Pi-Minimax tester/architect override topology run exposed a harness bug: `compare-topologies` used the bare default registry for `TopologyMode::Seed`, so `A2D_TESTER_PROVIDER` and `A2D_ARCHITECT_PROVIDER` affected evolved mode but not seed mode. That made provider comparisons uncontrolled.

Fix: topology registry construction now routes through `build_runtime_registry_with_options`. Seed mode still bypasses persisted lineage provider policy, but explicit runtime overrides apply to both seed and evolved legs. Regression coverage:

- `seed_mode_runtime_registry_still_applies_explicit_overrides`
- `topology_seed_registry_path_still_applies_explicit_overrides`

Controlled run:

```bash
A2D_TESTER_PROVIDER=pi/minimax/MiniMax-M3 \
A2D_ARCHITECT_PROVIDER=pi/minimax/MiniMax-M3 \
A2D_PROVIDER_TIMEOUT_SECS=90 A2D_MAX_CYCLE_SECS=180 \
  cargo run -p a2d -- compare-topologies sudoku 2
```

Artifact: `/tmp/a2d-compare-topologies-sudoku2-pi-minimax-tester-architect-post-topology-override-fix-20260617.log`.

Both seed and evolved legs printed the override lines and invoked architect/tester through Pi Minimax. Seed and evolved both reached best 100% at cycle 1; evolved used the same invocation count and was 23.7s faster. Caveat: moving tester/architect to Pi makes GLM unassigned, so GLM can enter the coder portfolio; this run is a controlled tester/architect override comparison, not a Pi-only topology run. Treat it as encouraging but insufficient for durable provider-policy changes.

## 2026-06-17 provider-policy gate for Pi-Minimax

Ran provider-policy comparisons for moving tester and architect to `pi/minimax/MiniMax-M3`.

Artifacts:

- `/tmp/a2d-compare-provider-policy-sudoku2-pi-minimax-tester-architect-20260617.log`
- `/tmp/a2d-compare-provider-policy-sudoku2-pi-minimax-tester-architect-fullpolicy-20260617.log`

The first partial-policy proposal improved best fitness (+17pp) but was rejected for material wall-clock increase (+54.4s beyond slack). It also showed that partial proposal JSON can display omitted current assignments as deltas such as `evolver -> ∅`, so rerun with full explicit policy when assessing durable changes.

The full-policy rerun removed that confound and was still rejected: fitness +0pp, +2 invocations, +243.5s wall-clock. This is exactly the gate doing its job. Prefer Pi for future probes, but do not persist Pi-Minimax tester/architect defaults from current evidence.

## 2026-06-17 Pi-first 90s comparisons

After operator guidance to prefer Pi over OpenCode, ran Pi-only direct comparisons with `A2D_PROVIDER_TIMEOUT_SECS=90 A2D_MAX_CYCLE_SECS=120`.

Artifacts:

- `/tmp/a2d-compare-role-providers-architect-pi-only-90s-20260617-r1.json`
- `/tmp/a2d-compare-role-providers-tester-pi-only-90s-20260617-r1.json`
- `/tmp/a2d-compare-role-providers-architect-pi-kimi-minimax-90s-20260617-r2.json`
- `/tmp/a2d-compare-role-providers-tester-pi-kimi-minimax-90s-20260617-r2.json`

Summary:

- Architect: `pi/minimax/MiniMax-M3` produced valid noop `system_patch` twice and was fast (16.8s, 12.3s). `pi/kimi-coding/k2p7` produced one valid noop in 24.0s, then timed out at 90s. `pi/zai/glm-5.2` timed out at 90s.
- Tester: `pi/kimi-coding/k2p7` materialized output twice (12.8s, 73.3s), but the second preview looked code-like rather than a clear test-run report. `pi/minimax/MiniMax-M3` timed out once, then produced a clearer test-run report in 12.8s. `pi/zai/glm-5.2` timed out at 90s.

Interpretation: for Pi-preferred experiments, Pi Minimax is the strongest architect candidate in this diagnostic harness and is plausible for tester; Pi Kimi may be higher-quality when it returns but is not reliable enough for architect at the 90s cutoff. This is still direct diagnostic evidence, not challenge-integrated proof. Do not persist a default/provider-policy change without the comparison gate; next evidence should compare a Pi-preferred runtime override in an actual challenge or provider-policy comparison.

## 2026-06-17 corrected OpenCode Kimi k2.7 lane

A follow-up live check showed the previously registered A²D provider name `opencode/kimi-k2.7-code` is stale for the current OpenCode installation. `opencode models` lists `kimi-for-coding/k2p7`, and direct probes of the stale alias failed with OpenCode raw output `Model not found: kimi-k2.7-code/.`:

- `/tmp/a2d-compare-role-providers-architect-newlanes-60s-20260617-r1.json`
- `/tmp/a2d-compare-role-providers-tester-newlanes-60s-20260617-r1.json`

A²D now registers the corrected opt-in provider name `opencode/kimi-for-coding/k2p7` and rejects the stale `opencode/kimi-k2.7-code` alias in coverage.

Corrected-lane 60s smoke artifacts:

- `/tmp/a2d-compare-role-providers-architect-corrected-newlanes-60s-20260617-r1.json`
- `/tmp/a2d-compare-role-providers-tester-corrected-newlanes-60s-20260617-r1.json`

Result: OpenCode Kimi k2.7 produced a noop `system_patch` in 21.8s for architect and materialized `test_results` in 53.6s for tester. OpenCode GLM 5.2 failed architect with only `step_start`/no materialized output but materialized tester output in 39.9s. Pi Kimi k2.7 and Pi GLM 5.2 timed out for architect; both materialized tester outputs. This validates the corrected alias and invocation path only; it does not justify default changes.

## 2026-06-18 replicated comparison harness

`compare-role-providers` now supports `--replicas N` / `--replicas=N`. The command loops each named provider for each replica and emits top-level `replicas`/`providers` metadata, a top-level per-provider `summary`, and a per-result `replica` field, while preserving existing materialized-output previews and patch outcome fields. The summary counts attempts, accepted/rejected assignments, successes, failures, killed runs, timeout failures, materialized-output runs, patch outcomes, and min/max/mean elapsed milliseconds when available. This avoids scattering replicated evidence across manually named `r1`/`r2` files and makes downstream summarization less error-prone.

Validation:

- Focused CLI tests: `cargo test -p a2d role_provider_comparison -- --nocapture` passed, including parser coverage for provider lists, zero-replica rejection, and per-provider summary aggregation.
- No-provider-call JSON smokes: `cargo run -q -p a2d -- compare-role-providers sudoku architect --replicas 2 missing-provider` produced valid JSON with `replicas: 2`, `providers: ["missing-provider"]`, two rejected assignment rows labeled replicas 1 and 2, and summary counts for rejected assignments. Artifacts: `/tmp/a2d-compare-role-providers-replicas-invalid-smoke-20260618.json` and `/tmp/a2d-compare-role-providers-summary-invalid-smoke-20260618.json`.
- Full `cargo test` passed (242 passing, 2 ignored).

Next empirical runs should prefer this single-artifact replicated form rather than separate ad hoc `r1`/`r2` files.

## 2026-06-17 two-replica 60s default/Pi comparison

Commands used the same direct harness with `A2D_PROVIDER_TIMEOUT_SECS=60 A2D_MAX_CYCLE_SECS=90` for `architect` and `tester`, comparing:

- `opencode/zai-coding-plan/glm-5.1`
- `opencode/kimi-for-coding/k2p6`
- `opencode/opencode/deepseek-v4-flash-free`
- `pi/kimi-coding/k2p7`
- `pi/minimax/MiniMax-M3`
- `pi/zai/glm-5.2`

Artifacts:

- `/tmp/a2d-compare-role-providers-architect-pi-lanes-60s-20260617-r1.json`
- `/tmp/a2d-compare-role-providers-architect-pi-lanes-60s-20260617-r2.json`
- `/tmp/a2d-compare-role-providers-tester-pi-lanes-60s-20260617-r1.json`
- `/tmp/a2d-compare-role-providers-tester-pi-lanes-60s-20260617-r2.json`

Summary:

- Architect: Kimi k2p6 produced valid noop `system_patch` in both replicas (11.2s, 33.4s). GLM 5.1 produced one noop then timed out. DeepSeek materialized output twice but both were malformed/rejected. Pi Minimax produced one noop then timed out. Pi Kimi produced one malformed/rejected output then timed out. Pi GLM 5.2 timed out twice.
- Tester: Kimi k2p6 and DeepSeek materialized `test_results` in both replicas. GLM 5.1 failed both replicas (empty/no materialized output, then timeout). Pi Kimi timed out then materialized output. Pi Minimax and Pi GLM 5.2 materialized output in both replicas, but previews were often prose/command-intent rather than clearly mechanical test execution.

Interpretation: this is useful reliability/latency evidence but not a deterministic ranking. The same provider can flip between success and timeout at a 60s cutoff, so future runs should separate provider reliability from cutoff selection (for example more replicas, larger timeout buckets, or challenge-integrated evidence). Do not treat `assignment_accepted: true` as provider success; rank by `outcome`, `failed`, elapsed time, `materialized_output_previews`, and patch outcome fields. No default or durable provider-policy change is justified from this slice.
