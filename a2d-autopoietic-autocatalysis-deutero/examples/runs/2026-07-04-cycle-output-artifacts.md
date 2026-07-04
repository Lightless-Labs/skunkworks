# Cycle Output Artifacts — 2026-07-04

## Purpose

Close the gap discovered after the explicit `a2d cycle-input` bridge: a live provider could materialize a `code` artifact, but operators had no first-class file/manifest path for handing that artifact to `senior-swe-bench-extract-patch` and then to the evaluator wrapper.

## Lineage constraints

- Senior SWE-Bench task-cycle-input remains CLI/evaluation-layer plumbing; `a2d-core` stays benchmark-generic.
- Coding agents are still forbidden from public GitHub/web solution search.
- Captured provider output is not fitness evidence. It must pass extractor/evaluator gates before any Senior SWE-Bench fitness claim.
- Source persistence still requires fresh `a2d.fitness-evidence.v1` actual-test evidence.

## Change

`a2d cycle-input` now supports:

```bash
a2d cycle-input <artifact-bundle.json|-> [cycles] [--output-artifacts <dir>]
```

When enabled, materialized outputs are written as `.artifact` files and indexed by a cumulative `a2d.cycle-output-artifacts.v1` manifest. The manifest records cycle/workcell/enzyme/provider/artifact type, byte count, output path, and `git hash-object` content hash. Export fails closed on path collisions or an existing manifest rather than overwriting prior artifacts.

## Validation

Focused tests:

```bash
cargo fmt --check
cargo test -p a2d cycle_output_artifact -- --nocapture
cargo test -p a2d cycle_input -- --nocapture
```

Full suite:

```bash
cargo test
```

Final result: 302 passed, 2 ignored.

Reviewer found two critical issues in the first draft: output files could overwrite/collide, and multi-cycle manifests would be rewritten per cycle. Both were fixed before persistence by rejecting collisions/pre-existing manifests and accumulating manifest records across the whole run.

## Live cycle-input smoke

```bash
A2D_PROVIDER_TIMEOUT_SECS=120 A2D_MAX_CYCLE_SECS=180 \
  cargo run -q -p a2d -- cycle-input \
  runs/20260703-senior-swe-bench-cycle-input-evidence/task-cycle-input/firezone-fix-connlib-align-device-hard-cycle-input.json \
  1 --output-artifacts runs/20260704-cycle-output-artifacts-evidence/live-cycle/artifacts
```

Artifacts:

- `runs/20260704-cycle-output-artifacts-evidence/live-cycle/artifacts/manifest.json`
- `runs/20260704-cycle-output-artifacts-evidence/live-cycle/artifacts/cycle-0-wc-0001-coder-code.artifact`

The manifest captured one provider-produced `code` artifact from `opencode/kimi-for-coding/k2p6`, content hash `1c7b18fff2fe71619e93ba3aa8b0ed8146909538`.

## Downstream extraction gate

```bash
cargo run -q -p a2d -- senior-swe-bench-extract-patch \
  runs/20260704-cycle-output-artifacts-evidence/live-cycle/artifacts/cycle-0-wc-0001-coder-code.artifact
```

Result: fail-closed, because the provider output was prose (`I'll inspect...`) rather than a unified diff. Evidence:

- `runs/20260704-cycle-output-artifacts-evidence/extract/extract.err`
- `runs/20260704-cycle-output-artifacts-evidence/extract/extract.status` (`1`)

This is an integration finding, not a Senior SWE-Bench solve: the artifact can now be captured mechanically, and the extractor prevents non-patch output from becoming evaluator/fitness evidence.

## Fresh fitness evidence

```bash
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260704-cycle-output-artifacts-evidence/actual-test-score-artifact \
  cargo run -q -p a2d -- score-artifact sudoku runs/20260701-score-artifact-fitness-evidence/good-sudoku-artifact.rs

cargo run -q -p a2d -- fitness-evidence-inspect \
  runs/20260704-cycle-output-artifacts-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json \
  --require-all-tests-pass
```

Artifact: `runs/20260704-cycle-output-artifacts-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`

Inspected fields:

- `schema_version: a2d.fitness-evidence.v1`
- `actual_tests_evaluated: true`
- `non_regressing: true`
- `fitness: 1.0`
- `failed_cases: []`
- `source_diff_hash: ed02b5802a540a7800696e8c8110277afd471cda`, matching `git diff --binary HEAD -- crates | git hash-object --stdin`

This gates the CLI source patch. It is not official Senior SWE-Bench mastery.
