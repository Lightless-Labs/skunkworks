# Senior SWE-Bench base64 public-reference detection — 2026-07-07

## Purpose

Harden Senior SWE-Bench artifact diagnosis/selection against public GitHub solution references hidden in base64/base64url tokens.

Prior slices rejected raw, obfuscated, percent-encoded, nested percent-encoded, and GitHub CLI references. A remaining bypass was a provider artifact that embedded `github.com`, `raw.githubusercontent.com`, or `refs/pull/...` inside base64 text rather than plain text.

## Change

- `contains_public_github_solution_reference` now also scans bounded base64/base64url candidate tokens.
- Decoded token text is checked through the same normalized/percent-decoding detector as plain artifact text.
- Scanning is bounded by token size and does not recursively decode base64.
- Candidate selection/diagnosis regressions cover:
  - plain base64 GitHub URLs;
  - assignment-style metadata such as `source=<base64>`;
  - unpadded URL-safe base64;
  - base64-encoded `refs/pull/...`;
  - base64-encoded `raw.githubusercontent.com` URLs;
  - benign long base64-looking local metadata.

## Validation

Commands passed:

```bash
cargo fmt --check
CARGO_BUILD_JOBS=2 cargo test -p a2d --test senior_swe_bench_select_candidate_artifact senior_swe_bench_select_candidate_artifact_fails_closed_on_unsafe_manifests -- --nocapture
CARGO_BUILD_JOBS=2 cargo test -p a2d --test senior_swe_bench_diagnose_artifact diagnose_artifact_redacts_mixed_case_public_github_references -- --nocapture
CARGO_BUILD_JOBS=2 cargo test -p a2d --test senior_swe_bench_select_candidate_artifact -- --nocapture
CARGO_BUILD_JOBS=2 cargo test -p a2d --test senior_swe_bench_diagnose_artifact -- --nocapture
CARGO_BUILD_JOBS=2 cargo test
```

Manual detector smokes rejected both assignment and padded forms:

```bash
printf 'source=aHR0cHM6Ly9naXRodWIuY29tL29yZy9yZXBvL3B1bGwvMTIz\n' \
  | target/debug/a2d senior-swe-bench-diagnose-artifact -

printf 'source: aHR0cHM6Ly9yYXcuZ2l0aHVidXNlcmNvbnRlbnQuY29tL29yZy9yZXBvL21haW4vZml4LmRpZmY=\n' \
  | target/debug/a2d senior-swe-bench-diagnose-artifact -
```

Fresh source-patch evidence:

```bash
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260707-senior-swe-bench-base64-reference-detection-evidence/actual-test-score-artifact \
  cargo run -q -p a2d -- score-artifact sudoku runs/20260701-score-artifact-fitness-evidence/good-sudoku-artifact.rs

cargo run -q -p a2d -- fitness-evidence-inspect \
  runs/20260707-senior-swe-bench-base64-reference-detection-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json \
  --require-all-tests-pass
```

Evidence summary: `a2d.fitness-evidence.v1`, `actual_tests_evaluated: true`, `non_regressing: true`, `fitness: 1.0`, `failed_cases: []`, aggregate `all_tests_pass: true`, `hidden_acceptance: not_present`, `source_diff_scope: crates`, and `source_diff_hash: b2979a790b103b0b0c4255db939e0f3012f2cbdc`, matching `git diff --binary HEAD -- crates | git hash-object --stdin` for the source patch.

## Scope

This is CLI/evaluation-layer artifact safety hardening. It is not official Senior SWE-Bench mastery, hidden official holdout evidence, OS/network no-egress proof, or live provider-loop success evidence.

TDD note: regression fixtures were added before the production detector change, but clean pre-implementation failing-test output was not retained because concurrent Cargo package-cache locks caused repeated timeout/no-output runs. The persistence gate is therefore the passing regression suite plus fresh source-bound `a2d.fitness-evidence.v1` actual-test evidence, not a retained red-test artifact.
