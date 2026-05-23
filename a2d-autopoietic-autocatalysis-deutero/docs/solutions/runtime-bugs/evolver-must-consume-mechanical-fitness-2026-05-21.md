---
title: "Evolver Must Consume Mechanical Fitness Directly"
date: 2026-05-21
category: runtime-bugs
module: metabolism
problem_type: feedback-loop
component: evolver-scheduling
symptoms:
  - "Evolver was gated behind model-generated test_results even though sandbox fitness_report already existed"
  - "Slow tester providers could block germline adaptation"
  - "After code production, learning waited for a model summary of evidence the system already had mechanically"
root_cause: evolver_reactants_required_test_results_instead_of_mechanical_fitness_report
resolution_type: code_fix
severity: high
tags:
  - evolver
  - feedback-loop
  - fitness-report
  - scheduler
  - learning
---

# Evolver Must Consume Mechanical Fitness Directly

## Principle

Learning / adapting / self-improving > quality / correctness > speed.

A²D's most authoritative learning signal is not a model-generated `test_results` artifact. It is the sandbox's mechanical `fitness_report` plus the diagnostic `failure_report`.

## Problem

The evolver's reactant contract required `test_results`. That made germline adaptation wait for the tester model even though benchmark evaluation already produced:

- `fitness_report`
- `failure_report`

This created an unnecessary model gate between outcome evidence and adaptation.

## Solution

The evolver now reacts directly to mechanical fitness:

```text
Evolver: {fitness_report} -> {enzyme_defs}
catalysts: {enzyme_defs, failure_report, fitness_report}
```

Changes:

- seed germline uses `fitness_report` as the evolver reactant;
- loaded lineage germlines are normalized to the same contract;
- `test_results` is removed as an evolver gate;
- evolver prompt now documents the full food set and says `test_results` is optional supporting evidence, not a gate;
- scheduler priority now puts feedback metabolism first once `fitness_report` exists: evolver, architect, tester, coder retry;
- same-enzyme self-catalyst re-firing in a single cycle is prevented by recording post-output input revisions.

## Validation

Unit coverage:

- `normalize_loaded_enzymes_upgrades_evolver_to_mechanical_fitness`
- `feedback_metabolism_precedes_tester_and_coder_once_fitness_exists`

Full test suite passes: 149 tests passing, 2 ignored.

Live validation:

```bash
A2D_TRACE=1 A2D_PROVIDER_TIMEOUT_SECS=90 A2D_MAX_CYCLE_SECS=300 \
  cargo run -p a2d -- compare-topologies sudoku 2
```

Observed:

- seed cycle 1 produced code at 83%;
- seed cycle 2 ready order was `evolver`, `architect`, `tester`, `coder`, proving adaptation was no longer gated by tester;
- seed evolver still timed out on GLM, exposing provider assignment as the next bottleneck;
- evolved topology reached 100% at cycle 2.

Log: `/tmp/a2d-topology-compare-sudoku2-mechanical-evolver-20260521.log`.

## Related

- `docs/solutions/runtime-bugs/coder-retry-starves-feedback-metabolism-2026-05-21.md`
- `docs/solutions/best-practices/fitness-scored-coder-portfolio-2026-05-20.md`
- `todos/bounded-live-benchmarks.md`
