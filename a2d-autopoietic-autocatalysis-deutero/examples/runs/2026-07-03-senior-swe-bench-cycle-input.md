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

The initial bridge was cycle-input plumbing only. The follow-up replay slice now proves that a cycle-input artifact can drive the gated local evaluator/evidence path, but it still does not run a provider and does not prove official Senior SWE-Bench task mastery. The next gap remains wiring provider-produced candidate diffs to benchmark-provided checkouts and a real evaluator/holdout command without exposing public solution search.

## Cycle-input local evaluator replay

The follow-up slice wires the `not_evaluated` cycle-input artifact into the existing gated local evaluator wrapper without adding Senior SWE-Bench logic to `a2d-core`:

```bash
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260703-senior-swe-bench-cycle-input-replay-evidence/local-evaluator/fitness \
  cargo run -q -p a2d -- senior-swe-bench-evaluate \
  --task-cycle-input runs/20260703-senior-swe-bench-cycle-input-replay-evidence/task-cycle-input/firezone-fix-connlib-align-device-hard-cycle-input.json \
  --candidate-patch runs/20260703-senior-swe-bench-cycle-input-replay-evidence/local-evaluator/candidate.diff \
  --checkout runs/20260703-senior-swe-bench-cycle-input-replay-evidence/checkout \
  --output runs/20260703-senior-swe-bench-cycle-input-replay-evidence/local-evaluator/firezone-fix-connlib-align-device-hard-cycle-input-local-evaluation.json \
  -- "$PWD/runs/20260703-senior-swe-bench-cycle-input-replay-evidence/local-evaluator/mock-official-evaluator.sh"
```

Artifacts:

- `runs/20260703-senior-swe-bench-cycle-input-replay-evidence/local-evaluator/firezone-fix-connlib-align-device-hard-cycle-input-local-evaluation.json`
- `runs/20260703-senior-swe-bench-cycle-input-replay-evidence/local-evaluator/fitness/senior-swe-bench-firezone-fix-connlib-align-device-hard-cycle-0-fitness-evidence.json`

Evidence inspection:

- `schema_version: a2d.fitness-evidence.v1`
- `actual_tests_evaluated: true`
- `non_regressing: true`
- `fitness: 1.0`
- `failed_cases: []`
- result labels: `all_tests_pass`, `has_no_solution_search` (policy-declared from accepted no-search metadata, not network-forensics proof), `hidden_acceptance`
- `source_diff_hash: 65506a4c371a1751089be88cd0eb98501bb31649`
- `candidate_patch_hash: 8ecc93527321bf316172ef06469260421bc701db`
- `evaluator_kind: provided_local_command`

This validates cycle-input replay through the gated evaluator/evidence path. It is still provided-local-command evidence; only a benchmark-provided official evaluator/holdout command should be described as official Senior SWE-Bench fitness.

## Negative cycle-input replay smoke

A mutated cycle-input artifact with `benchmark_context.github_solution_search_allowed: true` was rejected before evaluator execution:

```bash
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260703-senior-swe-bench-cycle-input-replay-evidence/negative-smoke/fitness \
  cargo run -q -p a2d -- senior-swe-bench-evaluate \
  --task-cycle-input runs/20260703-senior-swe-bench-cycle-input-replay-evidence/task-cycle-input/firezone-fix-connlib-align-device-hard-cycle-input-allows-search.json \
  --candidate-patch runs/20260703-senior-swe-bench-cycle-input-replay-evidence/local-evaluator/candidate.diff \
  --checkout runs/20260703-senior-swe-bench-cycle-input-replay-evidence/checkout \
  --output runs/20260703-senior-swe-bench-cycle-input-replay-evidence/negative-smoke/should-not-exist.json \
  -- "$PWD/runs/20260703-senior-swe-bench-cycle-input-replay-evidence/local-evaluator/mock-official-evaluator.sh"
```

Artifacts:

- `runs/20260703-senior-swe-bench-cycle-input-replay-evidence/negative-smoke/solution-search-rejection.err`
- `runs/20260703-senior-swe-bench-cycle-input-replay-evidence/negative-smoke/solution-search-rejection.status` (`negative_status=1`)

No evaluation JSON and no fitness evidence were produced for the rejected unsafe cycle input.
