---
module: a2d-cli
tags: [fitness-evidence, evidence-gating, cli, self-improvement]
problem_type: best-practice
---

# Fitness evidence inspection must be first-class

## Problem

A²D's hard invariant depends on humans and automation inspecting structured `a2d.fitness-evidence.v1` records before persisting source patches or self-improvements. Relying on ad hoc `jq` snippets makes it easy to skip source-provenance checks, conflate partial non-regressing evidence with full aggregate acceptance, or overclaim hidden-holdout status.

## Pattern

Provide a narrow CLI command for evidence review:

```bash
a2d fitness-evidence-inspect <evidence.json> [--require-all-tests-pass]
```

The command should reuse the same exported-evidence validator as persistence gates, then print a small reviewed summary. Promotion-style checks must be stricter than merely finding an `all_tests_pass` label: require zero failures, `passed == total`, and no failed result entries.

## Constraints

- Inspecting partial non-regressing evidence is useful for debugging, but it is not benchmark mastery.
- Hidden holdout details remain redacted; absence of a `hidden_acceptance` result is printed as `not_present`, not as a hidden-holdout pass.
- Source binding remains the core safety property: `source_diff_hash` must match the current `crates` diff for source-gating evidence.

## Evidence

Implemented in `crates/a2d-cli/src/main.rs` and tested by `fitness_evidence_inspect_requires_current_non_regressing_actual_tests` plus `crates/a2d-cli/tests/score_artifact.rs`.

Fresh gate evidence: `runs/20260704-fitness-evidence-inspect-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, full-passing with `source_diff_hash: 5370681c12650e4236e4fb1bcc2cc4600ebb4794`.
