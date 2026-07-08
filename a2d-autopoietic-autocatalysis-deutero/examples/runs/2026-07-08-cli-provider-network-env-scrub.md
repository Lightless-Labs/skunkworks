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
CARGO_BUILD_JOBS=2 cargo test -p a2d-providers network_env_scrub_preserves_no_public_solution_search_policy_env -- --nocapture
CARGO_BUILD_JOBS=2 cargo test -p a2d-providers
CARGO_BUILD_JOBS=2 cargo test
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260708-provider-network-env-scrub-evidence/actual-test-score-artifact \
  cargo run -q -p a2d -- score-artifact sudoku runs/20260701-score-artifact-fitness-evidence/good-sudoku-artifact.rs
cargo run -q -p a2d -- fitness-evidence-inspect \
  runs/20260708-provider-network-env-scrub-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json \
  --require-all-tests-pass
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260708-postcommit-fitness-evidence-provider-env-scrub/actual-test-score-artifact \
  cargo run -q -p a2d -- score-artifact sudoku runs/20260701-score-artifact-fitness-evidence/good-sudoku-artifact.rs
cargo run -q -p a2d -- fitness-evidence-inspect \
  runs/20260708-postcommit-fitness-evidence-provider-env-scrub/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json \
  --require-all-tests-pass
HTTP_PROXY=http://example.invalid:9 HTTPS_PROXY=http://example.invalid:9 \
  ALL_PROXY=socks5://example.invalid:9 NO_PROXY=example.invalid \
  GIT_PROXY_COMMAND=blocked CARGO_HTTP_PROXY=http://example.invalid:9 \
  RUSTUP_DIST_SERVER=https://example.invalid \
  A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260708-provider-network-env-policy-preservation-evidence/actual-test-score-artifact \
  cargo run -q -p a2d -- score-artifact sudoku runs/20260701-score-artifact-fitness-evidence/good-sudoku-artifact.rs
cargo run -q -p a2d -- fitness-evidence-inspect \
  runs/20260708-provider-network-env-policy-preservation-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json \
  --require-all-tests-pass
```

Reviewer found the first process-global env-mutating regression unsafe; the final regression uses explicit `Command::env` values and does not mutate parent process env. Follow-up coverage applies the scrubber and no-search policy env to the same `Command`, proving the scrub removes ambient network configuration without deleting explicit A²D benchmark-integrity policy flags.

## Evidence

Fresh source-patch evidence:

- `runs/20260708-provider-network-env-scrub-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`
- `schema_version: a2d.fitness-evidence.v1`
- `actual_tests_evaluated: true`
- `non_regressing: true`
- `fitness: 1.0`
- `all_tests_pass: true`
- `source_diff_hash: 5e0c1967b4edc6cbf67b44edb2d8bc2cf49227d7`

Postcommit clean-head replay evidence:

- `runs/20260708-postcommit-fitness-evidence-provider-env-scrub/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`
- `source_tree_dirty: false`
- `source_diff_hash: e69de29bb2d1d6434b8b29ae775ad8c2e48c5391`

Follow-up policy-preservation evidence:

- `runs/20260708-provider-network-env-policy-preservation-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`
- `source_tree_dirty: true`
- `source_diff_hash: 686dd13acbb8d7b168ab9a29b8aef54f6653a479`

This gates the provider-boundary source patch and verifies a clean-head replay after commit. The follow-up pins policy-preservation behavior for the exact source diff. It is not network-forensics evidence, official Senior SWE-Bench evidence, or repeated autonomous benchmark mastery.
