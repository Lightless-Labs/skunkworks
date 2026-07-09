# 2026-07-09 — Retry next-cycle child env hardening

**Scope:** Senior SWE-Bench retry next-cycle child subprocess environment defense-in-depth.

## Lineage

Prior provider, evaluator, sandbox, and self-sandbox hardening made no-public-solution-search policy env observable and stripped inherited proxy/package-manager env at those subprocess boundaries. Scout recon found the next provider-adjacent gap in `crates/a2d-cli/src/main.rs`: `run_retry_next_cycle_command` spawned the current `a2d` executable for a persisted `cycle-input` next-cycle command without applying the shared env hardening.

## Change

- `run_retry_next_cycle_command` now builds a mutable `Command`, applies `provider_no_public_solution_search_env()`, strips the provider shared network/proxy/package-manager env list, then spawns the child.
- Added recursive focused regression coverage for the real retry next-cycle child path.
- The regression uses the provider public scrub-list wrapper and sets a short retry-next-cycle timeout to avoid a long CI stall on fixture regressions.

## Validation

```bash
cargo fmt --check
CARGO_BUILD_JOBS=2 cargo test -p a2d retry_next_cycle_child_process_receives_policy_env_and_scrubbed_network_env -- --nocapture --test-threads=1
CARGO_BUILD_JOBS=2 cargo test --manifest-path Cargo.toml
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260708-retry-next-cycle-child-env-hardening-evidence/actual-test-score-artifact \
  cargo run -q -p a2d -- score-artifact sudoku runs/20260701-score-artifact-fitness-evidence/good-sudoku-artifact.rs
cargo run -q -p a2d -- fitness-evidence-inspect \
  runs/20260708-retry-next-cycle-child-env-hardening-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json \
  --require-all-tests-pass
git diff --binary HEAD -- crates | git hash-object --stdin
```

Evidence:

- `runs/20260708-retry-next-cycle-child-env-hardening-evidence/tdd-baseline-failures.txt`
- `runs/20260708-retry-next-cycle-child-env-hardening-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`
- `runs/20260708-retry-next-cycle-child-env-hardening-evidence/validation-summary.json`

Source-patch evidence summary: `a2d.fitness-evidence.v1`, `actual_tests_evaluated: true`, `non_regressing: true`, `fitness: 1.0`, `all_tests_pass: true`, `hidden_acceptance: not_present`, `source_diff_hash: 39addaa048d4a6576a70d543efcbccd0d371146d`, matching the scoped crates diff.

## Non-claims

This is not official Senior SWE-Bench mastery, not hidden official holdout proof, not OS/network no-egress proof, and not live provider-loop success evidence. The evidence hash is intentionally scoped to `crates/`; docs and run artifacts are metadata documenting the source-bound gate.
