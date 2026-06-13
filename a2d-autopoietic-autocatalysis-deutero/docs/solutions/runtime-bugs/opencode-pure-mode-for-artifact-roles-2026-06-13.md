---
title: "OpenCode Artifact Roles Should Run in Pure Mode"
date: 2026-06-13
category: runtime-bugs
module: providers
problem_type: provider_integration
component: opencode-cli
symptoms:
  - "Architect role comparison returned quickly but materialized no system_patch"
  - "Raw OpenCode stdout showed attempted tool reads unrelated to the prompt-supplied artifact contract"
  - "Provider comparison evidence was confounded by local OpenCode plugin/session behavior"
root_cause: opencode_cli_artifact_invocations_allowed_external_plugins
resolution_type: hardening
severity: medium
tags:
  - opencode
  - provider-isolation
  - architect
  - artifact-contract
  - pure-mode
---

# OpenCode Artifact Roles Should Run in Pure Mode

## Problem

A²D CLI providers already run in an empty temp cwd so model tools cannot mutate the repository outside the typed artifact gates. During a 30s architect provider comparison, Kimi/OpenCode returned quickly but produced no `system_patch`; raw stdout showed OpenCode trying to read `/Users/thomas/.claude/CLAUDE.md` and being denied.

That made the result hard to interpret: the failure mixed model behavior, OpenCode local plugin/session behavior, and A²D's intentional cwd isolation.

## Fix

`CliProvider::opencode` now passes `--pure` to `opencode run` for artifact invocations:

```text
opencode run --model <model> --pure --format json <prompt>
```

OpenCode help describes `--pure` as running without external plugins. This does not weaken A²D's existing isolation: providers still run in an empty temp cwd, and source changes still must flow through typed artifacts (`SystemPatch`, `ProjectPatchset`) plus mechanical gates.

## Validation

TDD sequence:

1. Added failing unit test `opencode_provider_uses_pure_mode_for_artifact_invocations`.
2. Confirmed it failed because `--pure` was absent.
3. Added `--pure` to the OpenCode args builder.
4. Re-ran focused test: passed.
5. Ran `cargo test -p a2d-providers`: 8 passed.
6. Ran full `cargo test`: 229 passed, 2 ignored.
7. Ran post-change escalation regression:

```bash
A2D_PROVIDER_TIMEOUT_SECS=1 A2D_MAX_CYCLE_SECS=1 A2D_RUNG6_MAX_PROVIDERS=2 \
  cargo run -q -p a2d -- validate-escalation sudoku coder
```

Artifact: `/tmp/a2d-validate-escalation-post-opencode-pure-20260613.json`.

Result: passed mechanically. The JSON preserves the external `escalation_rung` field contract, rung 4 keeps the failure marker visible, rungs 5/6 strip it, every rung reports `provider_policy_changed: false`, and rung 6 records two candidate evaluations.

## Follow-up

Re-run a bounded architect comparison after this change. A successful run should still be judged by materialized `system_patch` output and elapsed time; `--pure` only removes one provider-mode confound, not provider stochasticity or timeout risk.
