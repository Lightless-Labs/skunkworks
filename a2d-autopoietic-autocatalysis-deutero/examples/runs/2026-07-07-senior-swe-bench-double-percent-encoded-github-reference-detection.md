# Senior SWE-Bench Double Percent-Encoded GitHub Reference Detection

**Date:** 2026-07-07
**Scope:** Senior SWE-Bench public-solution-reference detector hardening
**Evidence:** `runs/20260707-senior-swe-bench-double-percent-encoded-reference-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`

## Lineage

Recent Senior SWE-Bench hardening rejected raw GitHub URLs, obfuscated hosts, GitHub CLI commands, percent-encoded hosts/refs, and then reused the shared detector for coder-visible evaluator feedback. That left a documented detector gap: the percent decoder ran once, so `github%252ecom` decoded only to `github%2ecom` and could still bypass artifact diagnosis, candidate selection, and feedback gates.

## Change

The shared CLI/evaluation-layer detector now applies bounded iterative ASCII percent decoding with a cap of eight passes. It checks after each decode pass, lowercases decoded text each time so fully encoded mixed-case hosts are still detected, and treats still-changing over-depth encodings as suspicious when the current or next decoded form carries public GitHub/ref structure. This avoids the reviewer-identified false positive on harmless deeply encoded local `issue` metadata.

## Regression coverage

- double/triple/over-depth encoded `github%...2ecom` hosts;
- double/triple/over-depth encoded `refs%...2fpull%...` raw refs;
- fully percent-encoded mixed-case `GitHub.com` references;
- visible feedback using the shared detector;
- harmless over-depth local metadata containing the word `issue` remains accepted;
- exact boundary coverage in candidate selection pins depth-8 public host/ref forms as detected at the decode cap, depth-9 public host/ref forms as rejected by the bounded fallback (including fully percent-encoded host/ref forms), and depth-9 local metadata as accepted.

## Validation

- TDD baseline: `runs/20260707-senior-swe-bench-double-percent-encoded-reference-evidence/tdd-baseline-failures.txt`
- `cargo fmt --check`
- focused diagnosis/selection/feedback regressions
- full diagnosis and selection integration suites
- full `CARGO_BUILD_JOBS=2 cargo test`
- `cargo run -q -p a2d -- fitness-evidence-inspect runs/20260707-senior-swe-bench-double-percent-encoded-reference-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json --require-all-tests-pass`

The source-patch evidence is full-passing `a2d.fitness-evidence.v1` actual-test evidence with `source_diff_hash: d68df156ee6531f78c71fe7eb78db107f77c1249` matching the implementation scoped crates diff. Postcommit clean-HEAD evidence is `runs/20260707-postcommit-fitness-evidence-d88d0e2-nested-percent/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, passes `fitness-evidence-inspect --require-all-tests-pass`, records `source_tree_dirty: false`, and carries clean crates diff hash `e69de29bb2d1d6434b8b29ae775ad8c2e48c5391`. `hidden_acceptance: not_present` is expected for this local score-artifact source-patch gate.

## Non-claims

This is public-solution-reference hardening only. It is not official Senior SWE-Bench mastery, not Senior SWE-Bench hidden-holdout evidence, not network no-egress enforcement, and not live provider-loop success evidence.
