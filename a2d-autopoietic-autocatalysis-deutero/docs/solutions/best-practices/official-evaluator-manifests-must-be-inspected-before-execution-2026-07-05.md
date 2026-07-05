---
module: a2d-cli
tags: [senior-swe-bench, official-evaluator, evidence-gates, information-barriers]
problem_type: best-practice
---

# Official evaluator manifests must be inspected before execution

## Problem

A local evaluator wrapper can produce valid `a2d.fitness-evidence.v1`, but that evidence must not be mistaken for official Senior SWE-Bench mastery. Conversely, an official evaluator command should not be run until its provenance is mechanically checked against the task, hidden-holdout policy, no-public-solution-search policy, and exact benchmark-provided argv.

## Pattern

Add a read-only manifest-inspection gate before evaluator execution. The gate should:

- accept a task package or task-cycle-input and validate no public GitHub solution search;
- load the official evaluator manifest and require matching task/repo;
- require hidden holdouts and `github_solution_search_allowed: false`;
- require exact evaluator argv equality with the benchmark-provided command;
- record manifest path and `git hash-object` hash for later provenance binding;
- explicitly state that no evaluator, evidence inspection, fitness claim, or official mastery has started.

This turns "official evaluator" from a convention into data that later evaluator/evidence gates can bind to. It also gives controllers an auditable preflight step that can fail closed before any hidden evaluator side effects occur.

## Evidence

Implemented by `a2d senior-swe-bench-official-evaluator-manifest-inspect` in `crates/a2d-cli/src/main.rs` with integration coverage in `crates/a2d-cli/tests/senior_swe_bench_official_evaluator_manifest.rs`. The tests use side-effecting evaluator sentinel scripts and assert the sentinel is absent on both success and command-mismatch failure.

Fresh source-patch evidence: `runs/20260705-official-evaluator-manifest-inspect-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, full-passing with `source_diff_hash: ea715543a40f05f1f0ae918a855f6a5da470224a`. This is CLI/source-patch evidence only, not official Senior SWE-Bench mastery.
