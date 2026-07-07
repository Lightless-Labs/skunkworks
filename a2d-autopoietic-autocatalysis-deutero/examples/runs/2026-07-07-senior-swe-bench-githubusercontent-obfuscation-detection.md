# Senior SWE-Bench Raw/Obfuscated GitHub Reference Detection

**Date:** 2026-07-07
**Scope:** Artifact diagnosis/selection no-public-solution-reference hardening; not official Senior SWE-Bench mastery.

## Lineage

The previous Senior SWE-Bench artifact detector slice widened public GitHub solution-reference detection from browser-style pull/commit URLs to SSH remotes (`git@github.com:...`) and raw pull refs (`refs/pull/...`). Follow-up recon and review showed another narrow artifact-safety gap: provider artifacts can cite public GitHub material via raw content hosts or simple obfuscated host spellings while still containing an otherwise valid unified diff.

This remains CLI/evaluation-layer hardening only. It does not change `a2d-core`, does not prove network egress prevention, and does not claim official benchmark mastery.

## Change

`contains_public_github_solution_reference` now also treats these as public solution-reference indicators:

- `githubusercontent.com` (for example `raw.githubusercontent.com/...`);
- `github[.]com`;
- `github dot com`;
- `github . com`.

Coverage was extended in:

- `crates/a2d-cli/src/main.rs` extractor unit test;
- `crates/a2d-cli/tests/senior_swe_bench_diagnose_artifact.rs`;
- `crates/a2d-cli/tests/senior_swe_bench_select_candidate_artifact.rs`.

The tests prove extraction rejects these forms before candidate patch materialization, diagnostic previews are redacted, and candidate selection rejects valid-looking diffs containing them before extraction/evaluation.

## TDD Baseline

With only the new fixtures added, before widening the detector, the focused checks failed as expected:

- extractor accepted a fenced diff preceded by `https://raw.githubusercontent.com/...`;
- diagnosis reported `contains_public_github_solution_reference == false`;
- candidate selection exited `0` for a valid-looking diff containing a raw/obfuscated GitHub reference.

Sanitized baseline notes: `runs/20260707-senior-swe-bench-githubusercontent-obfuscation-evidence/tdd-baseline-failures.txt`.

## Validation

```bash
cargo fmt --check
CARGO_BUILD_JOBS=2 cargo test -p a2d senior_swe_bench_candidate_patch_extractor_accepts_diff_and_fenced_diff_only -- --nocapture
CARGO_BUILD_JOBS=2 cargo test -p a2d --test senior_swe_bench_diagnose_artifact diagnose_artifact_redacts_mixed_case_public_github_references -- --nocapture
CARGO_BUILD_JOBS=2 cargo test -p a2d --test senior_swe_bench_select_candidate_artifact senior_swe_bench_select_candidate_artifact_fails_closed_on_unsafe_manifests -- --nocapture
CARGO_BUILD_JOBS=2 cargo test -p a2d --test senior_swe_bench_diagnose_artifact -- --nocapture
CARGO_BUILD_JOBS=2 cargo test -p a2d --test senior_swe_bench_select_candidate_artifact -- --nocapture
CARGO_BUILD_JOBS=2 cargo test
```

Fresh source-patch evidence:

```bash
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260707-senior-swe-bench-githubusercontent-obfuscation-evidence/actual-test-score-artifact \
  cargo run -q -p a2d -- score-artifact sudoku runs/20260701-score-artifact-fitness-evidence/good-sudoku-artifact.rs

cargo run -q -p a2d -- fitness-evidence-inspect \
  runs/20260707-senior-swe-bench-githubusercontent-obfuscation-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json \
  --require-all-tests-pass
```

Evidence summary: `a2d.fitness-evidence.v1`, `actual_tests_evaluated: true`, `non_regressing: true`, `fitness: 1.0`, `passed: 6`, `total: 6`, `failed_cases: []`, aggregate `all_tests_pass: true`, `hidden_acceptance: not_present`, `source_diff_scope: crates`, `source_diff_hash: 175282b7c7368b956557c63567b86f98a57ad79a`, matching the scoped crates diff.

The evidence command scored the tracked good Sudoku artifact at `runs/20260701-score-artifact-fitness-evidence/good-sudoku-artifact.rs`; this is a fresh source-bound actual-test gate for the A²D source change, not a new Senior SWE-Bench task run. `hidden_acceptance: not_present` is expected for this local score-artifact gate and must not be cited as official Senior SWE-Bench hidden-holdout evidence.

## Interpretation

This is artifact safety hardening for the Senior SWE-Bench no-public-solution-search policy. It is not OS/network no-egress proof, not official Senior SWE-Bench mastery, and not live provider-loop success evidence.
