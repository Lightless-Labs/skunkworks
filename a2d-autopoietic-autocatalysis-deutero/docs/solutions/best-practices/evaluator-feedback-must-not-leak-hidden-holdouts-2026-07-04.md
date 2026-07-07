---
module: a2d-cli
tags: [senior-swe-bench, feedback-loop, hidden-holdouts, information-barriers]
problem_type: best-practice
---

# Evaluator feedback must not leak hidden holdouts

## Problem

Closing the feedback loop is necessary: a coder cannot improve a failed Senior SWE-Bench candidate patch if it never sees any evaluator result. But evaluator output can also contain hidden-holdout details or public GitHub solution references. Feeding that output directly into the next coder prompt would violate the benchmark information barrier.

## Pattern

Treat evaluator feedback as task-context food, not runtime evidence:

- inject it only into coder-visible `design` / `plan` context;
- reset task `evaluation` to `not_evaluated` / `fitness: null` for the next attempt;
- never seed `fitness_report`, `failure_report`, `system_patch`, `test_results`, `enzyme_defs`, `code`, or provider-policy artifacts from cycle input;
- redact official/hidden-holdout evaluator stdout/stderr by default;
- show local evaluator stdout/stderr only when explicitly declared `public_local_test_output`;
- reject public GitHub solution references in both preserved cycle-input text and visible evaluator fields;
- enum-validate metadata before formatting it into coder-visible prompts.

## Evidence

Implemented by `a2d senior-swe-bench-cycle-input-feedback` in `crates/a2d-cli/src/senior_swe_bench.rs`, with CLI coverage in `crates/a2d-cli/tests/senior_swe_bench_cycle_input_feedback.rs`.

Fresh source-patch gate evidence: `runs/20260704-senior-swe-bench-cycle-input-feedback-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, full-passing with `source_diff_hash: dd390ea40414bb9c16a7aded24adae854c094d09`.

Run doc: `examples/runs/2026-07-04-senior-swe-bench-cycle-input-feedback.md`.

## 2026-07-07 public-solution-reference parity follow-up

The same feedback barrier that redacts hidden/official output must also reject public solution-reference variants consistently. `senior-swe-bench-cycle-input-feedback` now uses the shared artifact public GitHub detector before preserving cycle-input text or making public local-test output visible to the next coder, covering obfuscated hosts, percent encoding, and GitHub CLI command references.

Fresh source-patch evidence: `runs/20260707-senior-swe-bench-feedback-solution-reference-normalization-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, full-passing `a2d.fitness-evidence.v1` with `source_diff_hash: b8d552350c3a5d24f46e3ba7b6a0e299f80c120e`.

Postcommit clean-HEAD evidence for implementation commit `8a5a7c7`: `runs/20260707-postcommit-fitness-evidence-8a5a7c7-feedback/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, full-passing with clean crates diff hash `e69de29bb2d1d6434b8b29ae775ad8c2e48c5391`.
