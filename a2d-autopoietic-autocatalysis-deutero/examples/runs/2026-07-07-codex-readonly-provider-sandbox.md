# Codex Read-Only Provider Sandbox

**Date:** 2026-07-07
**Scope:** CLI provider artifact-role hardening; not OS/network no-egress enforcement.

## Lineage

Senior SWE-Bench work has repeatedly separated policy observability from enforcement: provider subprocesses receive no-public-solution-search environment flags, OpenCode uses pure/tool-denied artifact mode, Pi runs no-tools/no-context artifact mode, and A²D applies any source changes only through structured patch/evidence gates. The remaining Codex-specific gap was that `CliProvider::codex` still requested `--full-auto`, which is broader than an artifact-only provider role needs and is inconsistent with the no-direct-repo-mutation boundary.

## Change

`crates/a2d-providers/src/cli.rs` now builds Codex provider invocations with explicit read-only, ephemeral artifact settings:

- `--sandbox read-only`
- `--ephemeral`
- `--skip-git-repo-check`
- `--ignore-user-config`
- `--ignore-rules`

The Codex provider no longer passes `--full-auto`, and regression coverage asserts it also does not request dangerous sandbox or hook bypass flags.

## Validation

```bash
codex exec --help
CARGO_BUILD_JOBS=2 cargo test -p a2d-providers codex_provider_ -- --nocapture
cargo fmt --check
CARGO_BUILD_JOBS=2 cargo test -p a2d-providers -- --nocapture
CARGO_BUILD_JOBS=2 cargo test
```

Fresh source-patch evidence:

```bash
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260707-codex-readonly-provider-sandbox-evidence/actual-test-score-artifact \
  cargo run -q -p a2d -- score-artifact sudoku runs/20260701-score-artifact-fitness-evidence/good-sudoku-artifact.rs

cargo run -q -p a2d -- fitness-evidence-inspect \
  runs/20260707-codex-readonly-provider-sandbox-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json \
  --require-all-tests-pass

git diff --binary HEAD -- crates | git hash-object --stdin
```

Evidence summary: `a2d.fitness-evidence.v1`, `actual_tests_evaluated: true`, `non_regressing: true`, `fitness: 1.0`, `passed: 6`, `total: 6`, `failed_cases: []`, aggregate `all_tests_pass: true`, `hidden_acceptance: not_present` for this local source-patch score-artifact gate, `source_diff_scope: crates`, and `source_diff_hash: 1c0bd351606a4cd352d5314b16417002bad99023`, matching the scoped crates diff.

## Interpretation

This is Codex provider CLI sandbox hardening for artifact roles. It reduces direct filesystem/tool risk for Codex-backed A²D providers, but it is not OS/network no-egress enforcement, not official Senior SWE-Bench mastery, and not live provider-loop success evidence.
