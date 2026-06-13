---
title: "OpenCode Artifact Roles Should Run in Pure Mode and Deny Tools"
date: 2026-06-13
category: runtime-bugs
module: providers
problem_type: provider_integration
component: opencode-cli
symptoms:
  - "Architect role comparison returned quickly but materialized no system_patch"
  - "Raw OpenCode stdout showed attempted tool reads unrelated to the prompt-supplied artifact contract"
  - "Provider comparison evidence was confounded by local OpenCode plugin/session behavior"
root_cause: opencode_cli_artifact_invocations_allowed_external_plugins_and_tools
resolution_type: hardening
severity: medium
tags:
  - opencode
  - provider-isolation
  - architect
  - artifact-contract
  - pure-mode
  - tool-denial
---

# OpenCode Artifact Roles Should Run in Pure Mode and Deny Tools

## Problem

A²D CLI providers already run in an empty temp cwd so model tools cannot mutate the repository outside the typed artifact gates. During a 30s architect provider comparison, Kimi/OpenCode returned quickly but produced no `system_patch`; raw stdout showed OpenCode trying to read `/Users/thomas/.claude/CLAUDE.md` and being denied.

That made the result hard to interpret: the failure mixed model behavior, OpenCode local plugin/session behavior, and A²D's intentional cwd isolation.

## Fix

First, `CliProvider::opencode` was changed to pass `--pure` to `opencode run` for artifact invocations:

```text
opencode run --model <model> --pure --format json <prompt>
```

OpenCode help describes `--pure` as running without external plugins. This does not weaken A²D's existing isolation: providers still run in an empty temp cwd, and source changes still must flow through typed artifacts (`SystemPatch`, `ProjectPatchset`) plus mechanical gates.

Follow-up live comparison showed `--pure` alone was not enough: a 60s Kimi architect run still emitted OpenCode `tool_use` events against the intentionally empty temp cwd and failed with no materialized `system_patch`. A²D now also writes an `opencode.json` into the temp provider cwd and selects an artifact-only agent:

```json
{
  "agent": {
    "a2d-artifact-no-tools": {
      "permission": { "*": "deny" }
    }
  }
}
```

The OpenCode invocation now includes:

```text
opencode run --model <model> --pure --agent a2d-artifact-no-tools --format json <prompt>
```

A direct temp-cwd probe verified OpenCode discovers this cwd-local `opencode.json` and accepts `--agent a2d-artifact-no-tools`.

## Validation

TDD sequence:

1. Added failing unit test `opencode_provider_uses_pure_mode_for_artifact_invocations`.
2. Confirmed it failed because `--pure` was absent.
3. Added `--pure` to the OpenCode args builder.
4. Re-ran focused test: passed.
5. Ran `cargo test -p a2d-providers`: 8 passed.
6. Ran full `cargo test`: 229 passed, 2 ignored.
7. Added unit coverage that OpenCode selects the artifact agent and that the generated config denies all tools.
8. Ran `cargo test -p a2d-providers`: 10 passed.
9. Ran full `cargo test`: 231 passed, 2 ignored.
10. Ran a direct temp-cwd OpenCode probe proving cwd-local `opencode.json` is discovered for `--agent a2d-artifact-no-tools`.
11. Ran a post-deny-tools Kimi architect comparison: `/tmp/a2d-compare-role-providers-architect-30s-no-tools-kimi-20260613.json`; Kimi produced a noop `system_patch` in 15.8s, and the captured JSON/stderr contained no `tool_use`/tool events.
12. Ran post-change escalation regression:

```bash
A2D_PROVIDER_TIMEOUT_SECS=1 A2D_MAX_CYCLE_SECS=1 A2D_RUNG6_MAX_PROVIDERS=2 \
  cargo run -q -p a2d -- validate-escalation sudoku coder
```

Artifact: `/tmp/a2d-validate-escalation-post-opencode-pure-20260613.json`.

Result: passed mechanically. The JSON preserves the external `escalation_rung` field contract, rung 4 keeps the failure marker visible, rungs 5/6 strip it, every rung reports `provider_policy_changed: false`, and rung 6 records two candidate evaluations.

## Follow-up

Continue judging bounded architect comparisons by materialized `system_patch` output, elapsed time, and `materialized_output_previews`. Pure/no-tools mode removes one provider-mode confound, not provider stochasticity or timeout risk.
