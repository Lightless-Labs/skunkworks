# 2026-07-08 — CLI Provider Network Configuration Env Scrub

**Scope:** CLI provider subprocess defense-in-depth; not OS/network no-egress proof and not official Senior SWE-Bench mastery.

## Change

`CliProvider::invoke` now removes common inherited network/proxy/package-manager environment variables from provider subprocesses before spawning the CLI, while preserving explicit A²D no-public-solution-search policy env flags.

Scrubbed examples include `HTTP_PROXY`, `HTTPS_PROXY`, `ALL_PROXY`, `NO_PROXY`, `GIT_PROXY_COMMAND`, `CARGO_HTTP_PROXY`, and Rustup mirror roots. The implementation is documented as environment-level defense-in-depth only.

## Validation

Commands run:

```bash
cargo fmt --check
CARGO_BUILD_JOBS=2 cargo test -p a2d-providers remove_network_configuration_env_drops_explicit_network_env -- --nocapture
CARGO_BUILD_JOBS=2 cargo test -p a2d-providers
CARGO_BUILD_JOBS=2 cargo test
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260708-provider-network-env-scrub-evidence/actual-test-score-artifact \
  cargo run -q -p a2d -- score-artifact sudoku runs/20260701-score-artifact-fitness-evidence/good-sudoku-artifact.rs
cargo run -q -p a2d -- fitness-evidence-inspect \
  runs/20260708-provider-network-env-scrub-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json \
  --require-all-tests-pass
```

Reviewer found the first process-global env-mutating regression unsafe; the final regression uses explicit `Command::env` values and does not mutate parent process env.

## Evidence

Fresh source-patch evidence:

- `runs/20260708-provider-network-env-scrub-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`
- `schema_version: a2d.fitness-evidence.v1`
- `actual_tests_evaluated: true`
- `non_regressing: true`
- `fitness: 1.0`
- `all_tests_pass: true`
- `source_diff_hash: 5e0c1967b4edc6cbf67b44edb2d8bc2cf49227d7`

This gates the provider-boundary source patch. It is not network-forensics evidence, official Senior SWE-Bench evidence, or repeated autonomous benchmark mastery.
