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

Latest full suite: 244 passed, 2 ignored. The CLI package name is `a2d`, so use `cargo test -p a2d` rather than `cargo test -p a2d-cli`.

## Next step

Live-validate the new evidence artifact in a bounded challenge run and inspect the emitted `fitness_report` lineage. A future stronger gate can compare pre-mutation and post-mutation topology in the same challenge context before accepting mutations in memory, not only before durable commits/apply.
