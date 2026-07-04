# Senior SWE-Bench Retry Step — 2026-07-04

## Purpose

Turn a bounded retry plan plus one local evaluation result into the next deterministic action without invoking providers, running evaluators, or claiming fitness.

This is the decision gate between manual retry planning and any future autonomous retry executor.

## Change

Added CLI command:

```bash
a2d senior-swe-bench-retry-step \
  --retry-plan <retry-plan.json|-> \
  --attempt-index <n> \
  --task-cycle-input <task-cycle-input.json|-> \
  --local-evaluation <local-evaluation.json|->
```

The output schema is `a2d.senior-swe-bench-cycle-retry-step.v1`.

Decision behavior:

- failed non-final attempt → `decision: build_next_cycle_input`, with a safe feedback-enriched `next_cycle_input`;
- failed final attempt → `decision: stop`, `stop_reason: max_attempts_exhausted`;
- passed attempt with `fitness_evidence_path` → `decision: inspect_fitness_evidence`, with `fitness-evidence-inspect <path> --require-all-tests-pass` args;
- passed attempt without evidence path → `decision: stop`, `stop_reason: missing_fitness_evidence_path`.

The command validates retry-plan schema, bounds, stop criteria, information barriers, per-attempt gates/transitions, task/repo bindings, local-evaluation no-search policy, and candidate-patch hash shape. It starts no providers/evaluators and keeps `fitness_claim_allowed_before_evidence: false`.

## Validation

Focused checks:

```bash
cargo fmt --check
cargo test -p a2d --test senior_swe_bench_retry_step -- --nocapture
```

Full suite:

```bash
cargo test
```

Result: 325 passed, 2 ignored.

Independent reviewer initially found stale evidence and partial retry-plan validation; final implementation tightened retry-plan validation and regenerated evidence with the current source diff.

## Smokes

Run directory: `runs/20260704-senior-swe-bench-retry-step-evidence/`.

Positive smokes:

- `retry-step-smoke/failed-step.json` — failed non-final attempt emits `decision=build_next_cycle_input`, no provider/evaluator invocations, no fitness claim, and a `not_evaluated` / null-fitness next cycle input.
- `retry-step-smoke/passed-step.json` — passed evaluation emits `decision=inspect_fitness_evidence` and the required `fitness-evidence-inspect --require-all-tests-pass` args.

Negative smokes:

- `negative-smoke/mismatch.err` — mismatched retry-plan/cycle-input task fails closed.
- `negative-smoke/malformed.err` — schema-complete but unsafe plan with empty `stop_criteria`, null barrier, and null required gate fails closed.

## Source-patch gate evidence

Artifact: `runs/20260704-senior-swe-bench-retry-step-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`

Inspected fields:

- `schema_version: a2d.fitness-evidence.v1`
- `actual_tests_evaluated: true`
- `non_regressing: true`
- `fitness: 1.0` (6/6)
- `failed_cases: []`
- `source_diff_scope: crates`
- `source_tree_dirty: true`
- `source_diff_hash: f2073d167fcddc55903fa19d8e459d8b25d29e0c`

Verified hash:

```bash
git diff --binary HEAD -- crates | git hash-object --stdin
# f2073d167fcddc55903fa19d8e459d8b25d29e0c
```

## Claim boundary

This is deterministic retry-step decision plumbing and source-patch evidence. It is not an autonomous retry executor and not official Senior SWE-Bench mastery.
