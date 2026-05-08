---
title: Verification failure focus must survive passing-test noise
date: 2026-05-08
module: a2-workcell
problem_type: logic_error
component: prior-attempt-motif
severity: high
tags:
  - self-correction
  - lineage
  - prompts
  - verification
---

# Verification failure focus must survive passing-test noise

## Symptom

After `a2ctl` started preserving stdout in verification notes, `compound-hidden` still did not self-correct. Minimax attempts 1-3 continued to touch only `a2_core/src/lib.rs` and ignored the hidden `a2ctl` regression.

The persisted note did contain stdout, but the prompt motif rendered a compact head snippet of the whole verification output. Full `cargo test` output starts with many passing crates/tests, so the failing `a2ctl` assertion could still appear after the motif budget.

## Fix

`a2_workcell::runtime::render_prior_motif` now derives an explicit `failure_focus` line for failed external verification notes. It extracts lines containing failure indicators such as:

- `FAILED`
- `failures:`
- `panicked at`
- `assertion failed`
- assertion `left:` / `right:` lines

The motif now renders:

```text
external_verification:
  result: FAIL
  failure_focus: ...actionable failed tests/assertions...
  detail: ...bounded raw note...
```

This keeps exact failing tests/assertions near the front of the next prompt even when the raw verifier output begins with pages of passing tests.

## Rule

For loop self-correction, raw verifier output is not enough. Persisted memory needs a focused failure summary before bounded raw detail, otherwise passing-test noise can consume the prompt budget and hide the actual repair target.
