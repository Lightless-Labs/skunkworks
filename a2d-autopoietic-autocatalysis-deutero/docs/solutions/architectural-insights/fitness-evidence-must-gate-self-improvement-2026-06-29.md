---
title: "Fitness Evidence Must Gate Self-Improvement"
date: 2026-06-29
module: metabolism
tags:
  - autopoiesis
  - fitness-evidence
  - feedback-loop
  - lineage
  - hidden-holdouts
problem_type: architectural-insight
---

# Fitness Evidence Must Gate Self-Improvement

## Problem

A²D could accept RAF-valid mutations or self-sandbox-valid patches without proving that the same cycle had actually evaluated generated code against the benchmark. RAF closure and `cargo test` are necessary plumbing gates, but they are not evidence of self-improvement on actual tasks.

The previous `fitness_report` artifact was also too terse:

```text
fitness: 0.83, passed: 5, failed: 1, total: 6
```

That scalar summary made evolver/architect feedback less actionable and made durability checks ambiguous: a cycle might have mutation/patch activity but no current benchmark evidence.

## Fix

The metabolism now emits `fitness_report` as structured JSON evidence (`schema_version: a2d.fitness-evidence.v1`) after a code artifact is benchmarked. The artifact includes:

- source `cycle`;
- aggregate fitness, passed/failed/total;
- `delta_from_last_non_regressing_fitness`;
- `non_regressing`;
- redacted per-case pass/fail evidence;
- failed case labels after redaction;
- `diagnostic_present` so consumers know whether `failure_report` has sandbox details.

Hidden holdout source and hidden-specific test names remain withheld. Public visible checks such as `compiles`, `has_tests`, `all_tests_pass`, and visible `has_*` string checks may appear; other case names collapse to `hidden_acceptance`.

The CLI durability gate now treats self-improvement as durable only when the report contains non-regressing actual-test evidence: either current `CycleReport` benchmark fitness with nonnegative `fitness_delta`, or a freshly consumed previous-cycle structured `fitness_report` with `non_regressing: true` and nonnegative delta. Accepted mutations, provider-policy commits, and automatic patch application are skipped when the cycle has no fresh non-regressing benchmark evidence or regressed fitness.

## Why this matters

This pins the minimal measurable acceptance path:

```text
hidden holdout / actual test run
  → structured fitness evidence artifact
  → non-regressing durability gate
  → mutation/policy/patch can become durable
```

That is the difference between "the graph stayed closed" and "the harness improved or at least did not regress on actual tests."

## Validation

Unit coverage added/updated:

- `feedback_loop_populates_failure_report_on_benchmark_failure` now verifies `fitness_report` is structured evidence, includes failed behavioral checks, and still pairs with `failure_report` diagnostics.
- `fitness_evidence_redacts_hidden_case_names` verifies hidden-specific case names are not leaked through `fitness_report`.
- `lineage_durability_gate_requires_actual_fitness_evidence` verifies no-evidence, legacy scalar evidence, structured regressing evidence, stale-cycle evidence, and regressing current cycles fail the durability predicate, while fresh non-regressing structured/current evidence passes.

Commands run:

```bash
cargo fmt --check
cargo test -p a2d-core metabolism -- --nocapture
cargo test -p a2d
cargo test
```

Original gate slice full suite: 244 passed, 2 ignored. After the live export/inspection slice, latest full suite: 246 passed, 2 ignored. After comparison export/provenance hardening, latest full suite: 252 passed, 2 ignored. The CLI package name is `a2d`, so use `cargo test -p a2d` rather than `cargo test -p a2d-cli`.

## Live export validation

A follow-up slice added an opt-in challenge evidence export path: set `A2D_FITNESS_EVIDENCE_EXPORT_DIR=<dir>` (or legacy alias `A2D_FITNESS_EVIDENCE_DIR`) and `a2d challenge` writes the current cycle's validated `fitness_report` JSON to that directory. The exporter fails closed: if export is requested and no actual-test fitness exists, if the schema is incomplete, if an unreviewed field appears, if the cycle is stale, if non-regression is false, or if non-public hidden case names appear, the CLI exits with an error instead of writing evidence.

Live smoke:

```bash
A2D_GERMLINE=seed \
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260629-fitness-evidence \
A2D_PROVIDER_TIMEOUT_SECS=120 \
A2D_MAX_CYCLE_SECS=180 \
cargo run -p a2d -- challenge sudoku 1
```

Exported evidence: `runs/20260629-fitness-evidence/sudoku-solver-cycle-0-fitness-evidence.json`. The artifact is `a2d.fitness-evidence.v1`, `cycle: 0`, `actual_tests_evaluated: true`, `non_regressing: true`, and contains public/aggregate holdout status (`all_tests_pass: false`) without hidden-specific case names. The run reached 67% (4/6), so it proves the evidence export/inspection path but not full Sudoku performance. See `examples/runs/2026-06-29-fitness-evidence-export.md`.

## Comparison export validation

A follow-up slice extended export to comparison modes without synthesizing evidence:

```bash
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260630-topology-fitness-evidence \
A2D_PROVIDER_TIMEOUT_SECS=90 \
A2D_MAX_CYCLE_SECS=120 \
cargo run -p a2d -- compare-topologies sudoku 1

A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260630-provider-policy-fitness-evidence \
A2D_PROVIDER_TIMEOUT_SECS=90 \
A2D_MAX_CYCLE_SECS=120 \
cargo run -p a2d -- compare-provider-policy sudoku 1
```

The exporter now prefixes paths with comparison labels (`seed-`, `evolved-`, `current-`, `proposed-`) while still writing the canonical `fitness_report` JSON. Provenance hardening matters: current artifact-store evidence is trusted only when `report.fitness` proves a benchmark ran in the current cycle, and feedback/durability evidence is trusted only from lineage inputs, never provider-produced `outputs[fitness_report]`.

Topology smoke artifacts `runs/20260630-topology-fitness-evidence/seed-sudoku-solver-cycle-0-fitness-evidence.json` and `runs/20260630-topology-fitness-evidence/evolved-sudoku-solver-cycle-0-fitness-evidence.json` both report `schema_version: a2d.fitness-evidence.v1`, `actual_tests_evaluated: true`, `non_regressing: true`, `fitness: 1.0`, and `all_tests_pass: true` with SHA-256 `6aa4f715aaa5dd155371519737ff569c3deb0233a01a18cc263e9ec0e2c62abe`. The provider-policy smoke exported labeled artifacts too, but it had no policy delta and one leg failed aggregate acceptance (`all_tests_pass: false`), so it is observational plumbing evidence only.

## Next step

Use the export path for future bounded challenge/topology/provider-policy smokes so every self-improvement decision has an auditable actual-test artifact. A future stronger gate can compare pre-mutation and post-mutation topology in the same challenge context before accepting mutations in memory, not only before durable commits/apply.
