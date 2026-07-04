---
module: a2d-cli
tags: [cycle-input, fitness-evidence, senior-swe-bench, information-barriers]
problem_type: best-practice
---

# Cycle input must not seed runtime evidence

## Problem

A²D can represent a task as a JSON artifact bundle (`requirements`, `design`, `plan`, benchmark context, evaluation metadata). Making this bundle consumable from a first-class CLI file/stdin path is useful for Senior SWE-Bench orchestration, but it also risks letting user-provided task context masquerade as mechanical runtime evidence.

Reserved artifacts such as `fitness_report`, `failure_report`, `provider_health_report`, `provider_policy`, and `system_code` are produced by benchmark/runtime mechanisms. If a cycle-input bundle can seed them, coding-agent context could bypass information barriers or confuse self-improvement gates.

## Pattern

Expose a narrow bridge:

```bash
a2d cycle-input <artifact-bundle.json|-> [cycles]
```

but validate before running the cycle:

- input must be a JSON object;
- allowed task-context artifacts can seed the existing artifact bundle parser;
- reserved runtime/mechanical artifacts are rejected before provider invocation.

## Evidence

Implemented in `crates/a2d-cli/src/main.rs` with integration coverage in `crates/a2d-cli/tests/cycle_input.rs`.

Fresh source-patch gate evidence: `runs/20260704-cycle-input-bridge-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, full-passing with `source_diff_hash: 918971a1f1be72793a3fb2f6c68dfa9e300c0825`.
