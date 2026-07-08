# Senior SWE-Bench Official Evaluator Manifest Inspect

**Date:** 2026-07-05
**Status:** source-patch evidence passed; not official Senior SWE-Bench mastery

## What changed

Added CLI-only `a2d senior-swe-bench-official-evaluator-manifest-inspect` to validate an official evaluator manifest before evaluator execution. The command accepts either `--task-package <json>` or `--task-cycle-input <json>`, requires `--official-evaluator-manifest <json>`, and checks the exact benchmark-provided evaluator argv after `--`.

The inspection output is `a2d.senior-swe-bench-official-evaluator-manifest-inspection.v1` and records the manifest path/hash, benchmark URL, hidden-holdout/no-search fields, exact evaluator command, and explicit false flags for evaluator invocation, evidence inspection, pre-evidence fitness claims, and public GitHub solution search.

## Validation

```bash
cargo fmt --check
cargo test -p a2d --test senior_swe_bench_official_evaluator_manifest -- --nocapture
cargo test

target/debug/a2d fitness-evidence-inspect \
  runs/20260705-official-evaluator-manifest-inspect-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json \
  --require-all-tests-pass
```

Focused integration tests passed (3 tests). Full `cargo test` passed. Reviewer found no blockers or warnings.

2026-07-08 follow-up coverage focused on the official benchmark URL origin trust boundary:

```bash
cargo fmt --check
CARGO_BUILD_JOBS=2 cargo test -p a2d --test senior_swe_bench_official_evaluator_manifest -- --nocapture
CARGO_BUILD_JOBS=2 cargo test
```

The manifest-inspection suite now rejects untrusted `benchmark_url` origins before evaluator execution, covering HTTP scheme downgrade, suffix-host spoofing, userinfo authority spoofing, and paths that merely contain the trusted host string.

## Evidence

Source-patch gate: `runs/20260705-official-evaluator-manifest-inspect-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, full-passing `a2d.fitness-evidence.v1` actual-test evidence with `source_diff_hash: ea715543a40f05f1f0ae918a855f6a5da470224a`, matching `git diff --cached --binary HEAD -- crates | git hash-object --stdin` before implementation commit `774827e`.

Postcommit clean-HEAD evidence for `774827e`: `runs/20260705-postcommit-fitness-evidence-774827e/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, passing `fitness-evidence-inspect --require-all-tests-pass` with `source_tree_dirty: false` and clean `source_diff_hash: e69de29bb2d1d6434b8b29ae775ad8c2e48c5391`.

Follow-up source-patch gate: `runs/20260708-official-manifest-url-origin-coverage-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, full-passing `a2d.fitness-evidence.v1` actual-test evidence with `source_diff_hash: 8ce20ffba8915071eee6e5162720cd0d66d1a2ca`, matching the scoped crates diff for the URL-origin coverage slice.

This is manifest-inspection and source-patch evidence only. It proves the CLI gate is validated without running the evaluator and that URL trust is origin/authority based; it does not prove official Senior SWE-Bench task success or A²D benchmark mastery.
