# 2026-07-08 — Sandbox child env hardening

**Scope:** candidate-code sandbox subprocess environment defense-in-depth.

## Lineage

Recent provider/evaluator hardening made no-public-solution-search policy env observable and stripped inherited proxy/package-manager env from CLI provider and Senior SWE-Bench evaluator subprocesses. Scout recon found the next highest-risk uncovered boundary in `crates/a2d-core/src/sandbox.rs`: provider-generated candidate code is compiled with `rustc`, compiled again with `rustc --test`, and executed as a generated test binary.

## Change

- Added generic process-env helpers in `crates/a2d-core/src/process_env.rs`.
- Applied policy env + network env scrubbing to sandbox `rustc`, `rustc --test`, and generated test-binary subprocesses.
- Made `a2d-providers` call the core network scrub helper to avoid duplicated scrub-list drift.

## Validation

```bash
cargo fmt --check
CARGO_BUILD_JOBS=2 cargo test -p a2d-core sandbox_evaluated_code_receives_policy_env_and_scrubbed_network_env -- --nocapture --test-threads=1
CARGO_BUILD_JOBS=2 cargo test -p a2d-providers remove_network_configuration_env_drops_explicit_network_env -- --nocapture
CARGO_BUILD_JOBS=2 cargo test -p a2d-providers network_env_scrub_preserves_no_public_solution_search_policy_env -- --nocapture
CARGO_BUILD_JOBS=2 cargo test --manifest-path Cargo.toml
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260708-sandbox-child-env-hardening-evidence/actual-test-score-artifact \
  target/debug/a2d score-artifact sudoku runs/20260701-score-artifact-fitness-evidence/good-sudoku-artifact.rs
target/debug/a2d fitness-evidence-inspect \
  runs/20260708-sandbox-child-env-hardening-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json \
  --require-all-tests-pass
```

Evidence:

- `runs/20260708-sandbox-child-env-hardening-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`
- `runs/20260708-sandbox-child-env-hardening-evidence/validation-summary.json`

Evidence summary: `a2d.fitness-evidence.v1`, `actual_tests_evaluated: true`, `non_regressing: true`, `fitness: 1.0`, `all_tests_pass: true`, `source_diff_hash: c3b0190ed4982b7989a8e43eba7f7ceb6307a08a`, matching the scoped crates diff.

## Non-claims

This is not official Senior SWE-Bench mastery, not hidden official holdout proof, not OS/network no-egress proof, and not live provider-loop success evidence.
