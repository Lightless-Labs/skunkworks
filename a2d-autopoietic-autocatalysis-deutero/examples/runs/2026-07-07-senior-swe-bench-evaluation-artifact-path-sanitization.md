# Senior SWE-Bench evaluation artifact path sanitization — 2026-07-07

## Purpose

Make successful Senior SWE-Bench local-wrapper evaluation artifacts portable after retry/local-evaluation `fitness_evidence_path` hardening exposed a remaining host-local leakage class.

A live retry-chain smoke produced passing local-wrapper evidence, but it embedded host-local paths in `candidate_patch_preflight_command` and the isolated temp `evaluator_checkout`, so it was useful as a smoke but not commit-ready evidence.

## Change

- In-project Senior SWE-Bench candidate patch, checkout, candidate artifact, evaluator command, and evidence-command path fields serialize through retry artifact path semantics.
- Isolated patched evaluator checkouts outside the project root serialize as `isolated_temp_checkout` instead of `/tmp`, `/var`, or machine-local project paths.
- Binding validators resolve repo-relative candidate patch/artifact paths before comparison.
- The isolated checkout marker is accepted only for `evaluator_checkout_mode: isolated_copy`.

## Validation

Commands passed:

```bash
cargo fmt --check
CARGO_BUILD_JOBS=2 cargo test -p a2d --test senior_swe_bench_retry_attempt_evaluate -- --nocapture
CARGO_BUILD_JOBS=2 cargo test -p a2d --test senior_swe_bench_retry_execute -- --nocapture
CARGO_BUILD_JOBS=2 cargo test
cargo run -q -p a2d -- fitness-evidence-inspect \
  runs/20260707-senior-swe-bench-evaluation-artifact-path-sanitization-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json \
  --require-all-tests-pass
```

Fresh source-patch gate:

- `runs/20260707-senior-swe-bench-evaluation-artifact-path-sanitization-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`
- `schema_version: a2d.fitness-evidence.v1`
- `actual_tests_evaluated: true`
- `non_regressing: true`
- `fitness: 1.0`
- `failed_cases: []`
- `source_diff_hash: d45429fdb7effe5b37436743bdb2442885a51fea`

`validation-summary.json` records that the evidence hash matches the current scoped crates diff and the evidence contains no host-local absolute path prefixes.

## Scope

This is artifact-portability/source-patch evidence only. It is not official Senior SWE-Bench mastery, hidden official holdout evidence, OS/network no-egress proof, or top-level goal completion.
