# 2026-07-08 — Self-sandbox cargo-test env hardening

**Scope:** automated self-modification sandbox subprocess environment defense-in-depth.

## Lineage

After provider, evaluator, and candidate-code sandbox subprocesses received no-public-solution-search policy env plus shared proxy/package-manager env scrubbing, scout recon found the adjacent uncovered self-modification boundary in `crates/a2d-core/src/self_sandbox.rs`: `validate_patches` copied the source tree and ran `cargo test` in the temp tree while inheriting parent process env.

Prior findings constrained this slice:

- Fitness evidence gates remain authoritative; passing `cargo test` alone is not persistence evidence.
- No-search env propagation is observability/defense-in-depth, not OS/network no-egress proof.
- Env tests should use per-command `Command::env` rather than mutating global process env.
- Senior SWE-Bench/domain-specific logic stays out of `a2d-core`; the helper used here is generic.

## Change

- Added `harden_self_sandbox_child_command` in `crates/a2d-core/src/self_sandbox.rs`.
- Applied shared `a2d_core::process_env` no-public-solution-search policy env and network/proxy/package-manager env scrubbing to the self-sandbox `cargo test` child command.
- Added recursive focused regression coverage proving the nested self-sandbox `cargo test` process receives policy env and does not inherit scrub-list env.

## Validation

```bash
# TDD baseline: failed before implementation; see tdd-baseline-failures.txt
CARGO_BUILD_JOBS=2 cargo test -p a2d-core \
  self_sandbox_cargo_test_receives_policy_env_and_scrubbed_network_env \
  -- --nocapture --test-threads=1

cargo fmt --check
CARGO_BUILD_JOBS=2 cargo test -p a2d-core \
  self_sandbox_cargo_test_receives_policy_env_and_scrubbed_network_env \
  -- --nocapture --test-threads=1
CARGO_BUILD_JOBS=2 cargo test -p a2d-core \
  self_sandbox::tests::validate_patches_accepts_combined_production_and_test_change_atomically \
  -- --nocapture
CARGO_BUILD_JOBS=2 cargo test --manifest-path Cargo.toml
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260708-self-sandbox-env-hardening-evidence/actual-test-score-artifact \
  cargo run -q -p a2d -- score-artifact sudoku runs/20260701-score-artifact-fitness-evidence/good-sudoku-artifact.rs
cargo run -q -p a2d -- fitness-evidence-inspect \
  runs/20260708-self-sandbox-env-hardening-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json \
  --require-all-tests-pass
```

Evidence:

- `runs/20260708-self-sandbox-env-hardening-evidence/tdd-baseline-failures.txt`
- `runs/20260708-self-sandbox-env-hardening-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`
- `runs/20260708-self-sandbox-env-hardening-evidence/validation-summary.json`

Source-patch evidence summary: `a2d.fitness-evidence.v1`, `actual_tests_evaluated: true`, `non_regressing: true`, `fitness: 1.0`, `all_tests_pass: true`, `source_diff_hash: d252e9a7f9596fdf3a21faf020b97a7693049c07`, matching the scoped crates diff. The evidence inspection reports `hidden_acceptance: not_present`; this is source-patch gate evidence, not an official hidden-holdout benchmark proof.

## Non-claims

This is not official Senior SWE-Bench mastery, not hidden official holdout proof, not OS/network no-egress proof, and not live provider-loop success evidence.
