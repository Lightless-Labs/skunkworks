# Senior SWE-Bench Cycle Input — 2026-07-03

## Purpose

Move one step closer to cycle integration without claiming official Senior SWE-Bench fitness. The new CLI mode emits a JSON artifact bundle that can seed A²D's existing `requirements` / `design` / `plan` inputs from a Senior SWE-Bench task, while keeping benchmark-specific parsing outside `a2d-core`.

## Change

Added:

```bash
a2d senior-swe-bench-audit <html|-> task-cycle-input <task-id>
```

The output JSON includes:

- `requirements`: Senior SWE-Bench task context, no-GitHub/public-solution-search policy, and an explicit unified-diff candidate-patch deliverable.
- `design`: local checkout/local tests only; no public solution search.
- `plan`: inspect checkout, implement patch, run local tests, return only a unified diff.
- `benchmark_context`: task id, repo, variant, difficulty, and no-solution-search flag.
- `evaluation`: `status: not_evaluated`, `fitness: null`.

The seed coder prompt now defaults to Rust source for normal A²D challenges, but may follow an explicit alternate deliverable such as a unified diff candidate patch.

## CLI smoke

```bash
cargo run -q -p a2d -- senior-swe-bench-audit \
  runs/20260703-senior-swe-bench-cycle-input-evidence/task-cycle-input/sample-next-payload.txt \
  task-cycle-input firezone-fix-connlib-align-device-hard \
  > runs/20260703-senior-swe-bench-cycle-input-evidence/task-cycle-input/firezone-fix-connlib-align-device-hard-cycle-input.json
```

Smoke artifact:

- `runs/20260703-senior-swe-bench-cycle-input-evidence/task-cycle-input/firezone-fix-connlib-align-device-hard-cycle-input.json`

Inspection confirmed it carries `requirements`, `design`, `plan`, `benchmark_context`, and `evaluation`; the requirements include `unified diff candidate patch` and `Do not search GitHub`; evaluation remains `not_evaluated`.

## Validation

```bash
cargo fmt --check
cargo test -p a2d senior_swe_bench -- --nocapture
cargo test -p a2d senior_swe_bench_cycle_input -- --nocapture
cargo test -p a2d seed_germline_coder_consumes_design_plan_and_requirements -- --nocapture
cargo test
rg -n "senior_swe_bench|SeniorSweBench|Senior SWE-Bench|senior-swe-bench" crates/a2d-core
```

No `a2d-core` matches.

## Fresh actual-test evidence

```bash
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260703-senior-swe-bench-cycle-input-evidence/actual-test-score-artifact \
  cargo run -q -p a2d -- score-artifact sudoku \
  runs/20260703-senior-swe-bench-cycle-input-evidence/good-sudoku-artifact.rs
```

Artifact:

- `runs/20260703-senior-swe-bench-cycle-input-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`

Evidence inspection:

- `schema_version: a2d.fitness-evidence.v1`
- `actual_tests_evaluated: true`
- `non_regressing: true`
- `fitness: 1.0`
- `failed_cases: []`
- result labels include `all_tests_pass`
- `source_diff_hash: 419a182e7af9bd8a6780bbf8fd84e2764ecfa9f1` (matches `git diff -- crates | git hash-object --stdin`)

## Status

This is cycle-input plumbing only. It does not run a provider, does not produce a Senior SWE-Bench candidate patch, and does not prove official Senior SWE-Bench task mastery. The next gap remains wiring provider-produced candidate diffs to benchmark-provided checkouts and a real evaluator/holdout command without exposing public solution search.
