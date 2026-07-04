---
title: "Checkout Context Must Be Bounded Artifact Context"
date: 2026-07-04
category: best-practices
module: senior-swe-bench
problem_type: benchmark_integration
component: cycle-input
severity: high
tags:
  - senior-swe-bench
  - checkout-context
  - provider-isolation
  - no-solution-search
  - evidence-gates
---

# Checkout Context Must Be Bounded Artifact Context

## Problem

Senior SWE-Bench task bundles told coders to inspect a local checkout, but A²D CLI providers intentionally run pure/no-tools in isolated working directories. A live artifact diagnosis classified the provider output as `checkout_context_not_exercised`: the model said it would inspect the checkout rather than producing a patch.

Giving the provider direct mutable filesystem access would violate A²D's provider-isolation boundary. Passing only a checkout path is also insufficient because artifact-only providers cannot read it.

## Resolution

`a2d cycle-input` now supports `--checkout <dir>`. The CLI reads a bounded, read-only snapshot from the checkout and injects it into coder-visible artifacts before provider invocation.

Safety properties:

- The provider receives text context, not filesystem access.
- `benchmark_checkout_context` is reserved; user bundles cannot spoof it.
- Root checkout symlinks are rejected; file symlinks are skipped and canonicalized/revalidated before read.
- Secret-like files and directories are excluded.
- Absolute checkout paths are redacted from provider-facing context.
- Context size/file-count limits bound prompt exposure.

## Validation

- Focused tests cover argument parsing, reserved artifact spoofing, context visibility in seeded artifacts, secret exclusion, root/file symlink handling, and missing/empty checkout rejection.
- Full test suite passed after implementation.
- Fresh source-patch evidence: `runs/20260704-cycle-input-checkout-context-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, `source_diff_hash: c003e6f4143413a5b40973ae7093ea14516bf12f`.
- Boundary check: `runs/20260704-cycle-input-checkout-context-evidence/boundary/a2d-core-boundary-rg.txt` confirms the implementation remains outside `a2d-core`.

## Follow-up

The next benchmark step is to run a bounded Senior SWE-Bench `cycle-input --checkout ... --output-artifacts ...` smoke against a benchmark-provided local checkout, then feed any captured provider artifact through extraction/evaluation. Do not describe checkout-context plumbing as Senior SWE-Bench mastery.
