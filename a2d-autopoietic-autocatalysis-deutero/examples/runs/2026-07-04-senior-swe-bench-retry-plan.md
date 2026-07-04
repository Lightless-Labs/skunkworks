# Senior SWE-Bench Retry Plan — 2026-07-04

## Purpose

Make retry-after-evaluation orchestration explicit before wiring any autonomous provider loop. Prior evidence showed the remaining manual glue is:

```text
cycle-input → code artifact → extract-patch → evaluate → feedback → next cycle-input
```

The new slice provides a bounded, machine-readable plan for that loop without starting providers, running evaluators, or claiming fitness.

## Change

Added CLI command:

```bash
a2d senior-swe-bench-retry-plan <task-cycle-input.json|-> [max-attempts]
```

The output schema is `a2d.senior-swe-bench-cycle-retry-plan.v1`. It records:

- `max_attempts` bounded to 1..=8;
- per-attempt gates: run `cycle-input` with output artifacts, extract a unified diff, evaluate against checkout, then inspect `a2d.fitness-evidence.v1` only if evaluation passes;
- fail-closed stop criteria for patch-extraction failure, policy/evidence-binding mismatch, successful evidence validation, or attempt exhaustion;
- information barriers: public GitHub solution search remains false, official/hidden output is redacted, and local stdout/stderr is coder-visible only when declared public-local feedback;
- `provider_invocations_started: false` and `fitness_claim_allowed_before_evidence: false`.

## Validation

Focused checks:

```bash
cargo fmt --check
cargo test -p a2d --test senior_swe_bench_retry_plan -- --nocapture
```

Full suite:

```bash
cargo test
```

Result: 323 passed, 2 ignored.

Independent reviewer found no blockers or warnings.

## Smokes

Run directory: `runs/20260704-senior-swe-bench-retry-plan-evidence/`.

Positive smoke:

- input: `retry-plan-smoke/task-cycle-input.json`
- output: `retry-plan-smoke/retry-plan.json`

The output is a stdout-only planning artifact, starts no providers/evaluators, and requires `a2d.fitness-evidence.v1` with `actual_tests_evaluated:true` and `non_regressing:true` before success.

Negative smoke:

- `negative-smoke/unbounded.err` — `max_attempts=9` is rejected before any provider/evaluator execution.

## Source-patch gate evidence

Artifact: `runs/20260704-senior-swe-bench-retry-plan-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`

Inspected fields:

- `schema_version: a2d.fitness-evidence.v1`
- `actual_tests_evaluated: true`
- `non_regressing: true`
- `fitness: 1.0` (6/6)
- `failed_cases: []`
- `source_diff_scope: crates`
- `source_tree_dirty: true`
- `source_diff_hash: fa652ee3ca175bb5cb37d15ab106840d17c37f84`

Verified hash:

```bash
git diff --binary HEAD -- crates | git hash-object --stdin
# fa652ee3ca175bb5cb37d15ab106840d17c37f84
```

## Claim boundary

This is bounded retry-planning plumbing and source-patch evidence. It is not an autonomous retry executor and not official Senior SWE-Bench mastery.
