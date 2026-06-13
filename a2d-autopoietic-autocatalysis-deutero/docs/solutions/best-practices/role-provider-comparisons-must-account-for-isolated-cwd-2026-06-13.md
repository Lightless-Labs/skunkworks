---
title: "Role Provider Comparisons Must Account for Isolated CWD"
date: 2026-06-13
category: best-practices
module: cli
problem_type: benchmark_methodology
component: provider-comparison
symptoms:
  - "Architect provider comparisons report no materialized system_patch even when the model invocation returns quickly"
  - "OpenCode raw stdout shows attempted tool reads from outside the isolated provider cwd"
  - "Provider latency results are tempting to treat as pure model-quality evidence"
root_cause: role_provider_comparisons_mix_model_quality_with_cli_tool_and_isolation_behavior
resolution_type: methodology
severity: medium
tags:
  - provider-comparison
  - architect
  - tester
  - isolated-cwd
  - live-validation
---

# Role Provider Comparisons Must Account for Isolated CWD

## Problem

`a2d compare-role-providers` is useful for reaching tester and architect directly, but its results are not always pure model-quality signals.

CLI providers intentionally run in an empty temp cwd so a provider cannot mutate the repository outside A²D's typed artifact gates. That isolation is correct for safety, but it affects providers that try to inspect files through their own tools instead of using the prompt-supplied context.

## Observation

A 2026-06-13 30s architect comparison produced:

- GLM 5.1: timed out after 30s;
- Kimi k2p6: returned quickly but produced no `system_patch`; raw stdout showed an attempted read of `/Users/thomas/.claude/CLAUDE.md` rejected by OpenCode permissions;
- DeepSeek v4 flash: timed out after 30s.

The Kimi result is therefore not simply "Kimi cannot be architect." It is evidence that the current OpenCode CLI mode may spend the invocation on tool behavior that is intentionally constrained by provider cwd isolation.

## Practice

When ranking role-provider comparison results:

1. Separate `assignment_accepted` from provider outcome.
2. Rank by `outcome`, `failed`, `elapsed_ms`, and materialized outputs.
3. Treat timeouts separately from no-materialized-output failures.
4. For architect runs, inspect raw-output previews when available; attempted repo/tool reads under isolated cwd are a harness/provider-mode interaction, not clean quality evidence.
5. Do not change durable provider defaults from one role comparison run. Repeat under bounded budgets and prefer evidence that the provider materializes the required artifact contract.

## Validation artifacts

- Tester 30s run 1: `/tmp/a2d-compare-role-providers-tester-30s-20260613.json`
- Tester 30s run 2: `/tmp/a2d-compare-role-providers-tester-30s-20260613-r2.json`
- Architect 30s run: `/tmp/a2d-compare-role-providers-architect-30s-20260613.json`

The tester runs were noisy: GLM and DeepSeek each succeeded once and timed out once; Kimi timed out twice. No tester default change is justified from this evidence.

## 2026-06-13 follow-up

OpenCode artifact invocations now pass `--pure` to reduce external plugin/session behavior during A²D provider calls. See `docs/solutions/runtime-bugs/opencode-pure-mode-for-artifact-roles-2026-06-13.md`.

Role-provider comparison JSON now includes `materialized_output_previews`, which closes the inspection gap where a run could report `system_patch` without exposing whether it was a patch or a no-op. Post-`--pure` Kimi architect evidence is promising but flaky: one comparable run produced a `system_patch`, one immediate solo rerun timed out, and a later preview-enabled run produced a noop explaining that the diagnostic marker was false-positive. Treat this as a reason to replicate, not as default-change evidence.
