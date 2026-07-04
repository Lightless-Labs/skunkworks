---
title: "Checkout Context Prompts Must State the No-Tools Boundary"
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
  - no-tools
  - artifact-provenance
---

# Checkout Context Prompts Must State the No-Tools Boundary

## Problem

Supplying a bounded checkout snapshot is not enough if the prompt still tells the coding agent to "inspect the checkout" without explaining that provider invocations are no-tools/artifact-only. A live smoke after checkout-context plumbing showed the model trying to run `ls` in the isolated provider temp directory instead of using the supplied snapshot text.

## Resolution

The `cycle-input --checkout` enriched `design` artifact now states that provider invocations are no-tools/artifact-only in isolated temporary working directories, that `ls`/`cat`/`find`/`grep`/shell/filesystem inspection tools cannot inspect the benchmark checkout, and that the coder must solve from the supplied snapshot text and return only a unified diff candidate patch.

## Validation

- Focused regression: `cargo test -p a2d cycle_input_checkout_context_enriches_design_without_trusting_bundle -- --nocapture` verifies the injected context includes the no-tools and unified-diff contract.
- Full suite: `cargo test` passed (309 passed, 2 ignored).
- Live diagnostic smoke produced an extractable non-empty unified diff from the captured provider artifact under `runs/20260704-cycle-input-no-tools-prompt-evidence/live-cycle/`.
- Local-wrapper Senior SWE-Bench evidence: `runs/20260704-cycle-input-no-tools-prompt-evidence/local-evaluator/fitness/senior-swe-bench-firezone-fix-connlib-align-device-hard-cycle-0-fitness-evidence.json` is `a2d.fitness-evidence.v1`, `actual_tests_evaluated: true`, `non_regressing: true`, and includes `hidden_acceptance`, but has `evaluator_kind: provided_local_command`.
- Source-patch gate: `runs/20260704-cycle-input-no-tools-prompt-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json` is full-passing with `source_diff_hash: 49431e677d4f4338b5cc71a125d5602e3c1eb3ca`.

## Claim Boundary

This is prompt/context hardening plus local-wrapper pipeline evidence. It is not official Senior SWE-Bench mastery; official claims still require `official_senior_swe_bench` evaluator evidence with a benchmark-provided manifest and hidden holdouts.
