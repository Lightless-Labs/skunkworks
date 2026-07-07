# Senior SWE-Bench GitHub CLI Reference Detection

**Date:** 2026-07-07
**Scope:** artifact no-public-solution-reference hardening
**Evidence:** `runs/20260707-senior-swe-bench-github-cli-reference-detection-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`

## Lineage

After the SSH/raw-ref and raw/obfuscated-host slices, artifact diagnosis and selection still only rejected URL/ref/host text. A provider artifact could cite a public solution lookup through GitHub CLI commands such as `gh pr view`, `gh api repos/.../pulls/...`, or `hub pr checkout` while avoiding the existing substrings.

## Change

`contains_public_github_solution_reference` now also rejects reviewed `gh`/`hub` solution-search subcommands (`api`, `pr`, `issue`, `repo`, `search`, `browse`, `clone`). The detector tokenizes text so ordinary prose fragments like `GH` and `PR` are not rejected unless they form a reviewed command shape.

## Validation

- TDD baseline failures recorded in `runs/20260707-senior-swe-bench-github-cli-reference-detection-evidence/tdd-baseline-failures.txt`.
- `cargo fmt --check`
- `CARGO_BUILD_JOBS=2 cargo test -p a2d --test senior_swe_bench_diagnose_artifact -- --nocapture`
- `CARGO_BUILD_JOBS=2 cargo test -p a2d --test senior_swe_bench_select_candidate_artifact -- --nocapture`
- `CARGO_BUILD_JOBS=2 cargo test`
- `cargo run -q -p a2d -- fitness-evidence-inspect runs/20260707-senior-swe-bench-github-cli-reference-detection-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json --require-all-tests-pass`

The evidence is full-passing `a2d.fitness-evidence.v1` actual-test evidence with `source_diff_hash: 1d45de32464f9e58a5fbd7dcdd4384ad594d01fa`, matching the scoped crates diff. `hidden_acceptance: not_present` is expected for this local score-artifact source-patch gate.

## Non-claims

This is not official Senior SWE-Bench mastery, not hidden-holdout Senior SWE-Bench evidence, not a network no-egress proof, and not live provider-loop success evidence.
