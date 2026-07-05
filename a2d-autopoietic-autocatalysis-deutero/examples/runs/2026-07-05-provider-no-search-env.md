# 2026-07-05 — CLI provider no-search policy env propagation

## Scope

Propagate the Senior SWE-Bench no-public-solution-search policy into CLI provider subprocess environments for observability.

This does **not** enforce OS/network isolation and is **not** official Senior SWE-Bench mastery evidence.

## Validation

- `cargo fmt --check`
- `cargo test -p a2d-providers cli_providers_export_no_public_solution_search_policy_env -- --nocapture`
- `cargo test -p a2d-providers cli_provider_subprocess_receives_no_public_solution_search_policy_env -- --nocapture`
- `cargo test -p a2d-providers`
- `cargo test`
- `target/debug/a2d fitness-evidence-inspect runs/20260705-provider-no-search-env-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json --require-all-tests-pass`

## Evidence

- Evidence artifact: `runs/20260705-provider-no-search-env-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`
- Validation summary: `runs/20260705-provider-no-search-env-evidence/validation-summary.json`
- Staged crates diff hash / evidence `source_diff_hash`: `1729e8cd7cf8f73cbe399a5dca2ba88cc254c63a`

The focused fake-CLI regression proves `CliProvider::invoke` applies the env map to the actual child process. The fresh `a2d.fitness-evidence.v1` artifact is source-patch gating evidence for the crates diff.
