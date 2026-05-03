---
title: Verification failures need stdout as well as stderr
date: 2026-05-03
module: a2ctl
problem_type: logic_error
component: apply-verification
severity: medium
tags:
  - self-correction
  - lineage
  - verification
  - prompts
---

# Verification failures need stdout as well as stderr

## Symptom

`compound-hidden` self-correction runs after core lineage reconciliation still failed on attempts 1-3 for both Minimax and Kimi. The new diff stats showed each attempt touched only the visible `a2_core` regression and never touched the hidden `a2ctl` regression.

The persisted external verification note came from `verify_and_rebuild()` → `run_workspace_command()` → `command_failure_message()`. That helper preferred stderr when stderr was non-empty and ignored stdout. For `cargo test`, the useful failing assertion output is often on stdout while stderr only says `error: test failed, to rerun pass ...`.

Result: prior-attempt motifs told the model that `cargo test` failed, but omitted the exact failing test/assertion that would point at the hidden regression.

## Fix

`a2ctl::command_failure_message()` now includes both streams when both are present:

```text
<label> failed: stderr:
...

stdout:
...
```

A regression test asserts both stdout and stderr appear in the generated failure message.

## Rule

For self-correction memory, verification detail is prompt input. Do not discard stdout just because stderr exists; many test runners put the actionable failure body on stdout.
