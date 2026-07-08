---
module: challenge-boundary
tags:
  - hidden-holdouts
  - core-boundary
  - evidence-gates
  - self-modification
problem_type: boundary-hardening
---

# Challenge catalog boundary sentinels must cover hidden oracle names

## Problem

After the 2026-07-04 challenge-catalog move, `a2d-core` no longer owned the built-in Sudoku/Chess/Rubik's challenge catalogs. The remaining boundary test checked broad challenge names (`sudoku_solver`, `chess_engine`, etc.), but it did not explicitly cover the newer hidden acceptance/oracle identifiers added for medium/hard Sudoku, chess castling/en-passant/Fool's mate, and Rubik's inverse/solver checks.

That left a maintenance gap: a future refactor could accidentally copy a domain-specific hidden holdout name into `a2d-core` while still passing the old boundary scan.

## Decision

Keep `a2d-core` generic and make the CLI-side boundary test scan core source for concrete domain oracle identifiers. The sentinel list intentionally lives in `crates/a2d-cli/src/main.rs` because the challenge catalog itself is also CLI/evaluation-layer code.

Avoid generic phrases such as `a2d_acceptance`: they can appear in core benchmark/evidence mechanics without violating the boundary. The sentinel terms should be concrete domain challenge or hidden-holdout names.

## False-positive handling

Two TDD baseline findings guided the final shape:

- `a2d_acceptance` was too generic and matched legitimate core benchmark helper code, so it is not forbidden.
- `fools_mate_leaves_checked_side_with_no_legal_moves` appeared in an `a2d-core` redaction unit fixture. The fixture was changed to synthetic `private_acceptance_case_42` so redaction coverage remains generic and does not couple core tests to chess catalog names.

Independent review found no blockers, but warned that hard-coded sentinel names can become stale. The test now includes a comment requiring updates when `crates/a2d-cli/src/challenges.rs` adds or renames domain oracles.

## Evidence

Fresh source-bound evidence for the boundary hardening:

- `runs/20260708-challenge-catalog-boundary-terms-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`
- `schema_version: a2d.fitness-evidence.v1`
- `actual_tests_evaluated: true`
- `non_regressing: true`
- `fitness: 1.0`
- `all_tests_pass: true`
- `source_tree_dirty: true`
- `source_diff_scope: crates`
- `source_diff_hash: 65d232d8f8f405098ddb59fe7f7363a79078c1fe`

Validation included focused boundary/redaction tests, full `CARGO_BUILD_JOBS=2 cargo test`, and `fitness-evidence-inspect --require-all-tests-pass`.

## Scope

This is challenge-boundary hardening only. It is not official Senior SWE-Bench mastery, not OS/network no-egress proof, and not new repeated autonomous benchmark-solving evidence.
