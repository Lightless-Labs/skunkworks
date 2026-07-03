# Senior SWE-Bench Local Evaluator Wrapper — 2026-07-03

## Purpose

Move one step beyond catalog/task packaging by adding a CLI-only wrapper that can run a caller-provided local Senior SWE-Bench evaluator command against a benchmark-provided checkout and candidate patch. Senior SWE-Bench-specific code remains outside `a2d-core`.

## Lineage constraints

- The previous task-package slice was explicitly `not_evaluated`; it did not prove task fitness.
- Coding agents must not search GitHub/public web for benchmark solutions.
- Source patches require fresh non-regressing `a2d.fitness-evidence.v1` actual-test evidence with holdout status before persistence.

## Change

Added:

```bash
a2d senior-swe-bench-evaluate \
  --task-package <json> \
  --candidate-patch <diff> \
  --checkout <dir> \
  [--output <json>] \
  -- <local-evaluator> [args...]
```

Behavior:

- Parses `a2d.senior-swe-bench-task-package.v1`.
- Refuses packages with `github_solution_search_allowed: true`.
- Runs the provided evaluator command in the supplied checkout.
- Passes task metadata via `A2D_SENIOR_SWE_BENCH_TASK_ID`, `A2D_SENIOR_SWE_BENCH_REPO`, and `A2D_SENIOR_SWE_BENCH_CANDIDATE_PATCH`.
- Captures stdout/stderr to files while waiting, avoiding pipe-buffer deadlocks.
- Emits `a2d.senior-swe-bench-local-evaluation.v1`.
- Exports `a2d.fitness-evidence.v1` only for full-passing evaluator outcomes. Failed evaluator outcomes still emit evaluation JSON and exit nonzero, but do not produce non-regressing evidence.

## Validation

```bash
cargo fmt --check
cargo test -p a2d senior_swe_bench -- --nocapture
cargo test -p a2d failed_senior_swe_bench_local_evaluator_is_not_non_regressing_evidence -- --nocapture
cargo test
```

Full suite result: 273 passed, 2 ignored.

Architecture boundary:

```bash
rg -n "senior_swe_bench|SeniorSweBench|Senior SWE-Bench|senior-swe-bench" crates/a2d-core
```

No `a2d-core` matches.

## Fresh fitness evidence

Passing local evaluator smoke:

```bash
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260703-senior-swe-bench-local-evaluator-evidence/local-evaluator/fitness \
  cargo run -q -p a2d -- senior-swe-bench-evaluate \
  --task-package runs/20260703-senior-swe-bench-local-evaluator-evidence/task-package/firezone-fix-connlib-align-device-hard-package.json \
  --candidate-patch runs/20260703-senior-swe-bench-local-evaluator-evidence/local-evaluator/candidate.diff \
  --checkout runs/20260703-senior-swe-bench-local-evaluator-evidence/checkout \
  --output runs/20260703-senior-swe-bench-local-evaluator-evidence/local-evaluator/firezone-fix-connlib-align-device-hard-local-evaluation.json \
  -- $PWD/runs/20260703-senior-swe-bench-local-evaluator-evidence/local-evaluator/mock-official-evaluator.sh
```

Artifacts:

- `runs/20260703-senior-swe-bench-local-evaluator-evidence/local-evaluator/firezone-fix-connlib-align-device-hard-local-evaluation.json`
- `runs/20260703-senior-swe-bench-local-evaluator-evidence/local-evaluator/fitness/senior-swe-bench-firezone-fix-connlib-align-device-hard-cycle-0-fitness-evidence.json`

Evidence inspection:

- `schema_version: a2d.fitness-evidence.v1`
- `actual_tests_evaluated: true`
- `non_regressing: true`
- `fitness: 1.0`
- `failed_cases: []`
- result labels include `all_tests_pass`, `hidden_acceptance`, and `has_no_solution_search`
- `source_revision: 6a840b6`
- `source_diff_hash: 7e82eecf604c76426d6a998138980ccdd8791f85`

The evidence source scope is `crates`; documentation updates in this run record the evidence and are not part of the source diff hash.

Negative smokes:

- `runs/20260703-senior-swe-bench-local-evaluator-evidence/bad-package/` proves packages allowing GitHub solution search are refused before evaluator execution.
- `runs/20260703-senior-swe-bench-local-evaluator-evidence/failed-evaluator/` proves failed evaluator outcomes emit local evaluation JSON with no `fitness_evidence_path` and no non-regressing evidence export.

## Status

This is still a local evaluator wrapper smoke, not proof of official Senior SWE-Bench task mastery. The remaining gap is to point this wrapper at a real benchmark-provided official evaluator/hidden-holdout command and then wire a challenge/cycle path that uses it without exposing hidden tests or public solution search.
