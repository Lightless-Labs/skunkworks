# Senior SWE-Bench Feedback Solution-Reference Normalization

**Date:** 2026-07-07
**Scope:** retry/cycle-input feedback no-public-solution-reference hardening
**Evidence:** `runs/20260707-senior-swe-bench-feedback-solution-reference-normalization-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`

## Lineage

The artifact diagnosis/selection detector had been hardened for raw GitHub refs, obfuscated hosts, GitHub CLI commands, and percent encoding. The feedback path (`senior-swe-bench-cycle-input-feedback`) still used a narrower local checker for evaluator feedback before making public local-test output coder-visible. That made feedback an inconsistent ingress for public solution references, despite the same no-GitHub/public-solution-search policy.

## Change

Feedback public-solution detection now reuses the shared artifact detector, so visible evaluator feedback and preserved cycle-input text reject the same reviewed public GitHub host/ref/CLI/percent-encoded forms as artifact diagnosis and candidate selection. Regression coverage covers:

- `github[.]com` without relying on `/issues` path markers;
- `github dot com` host spelling;
- `github%2ecom` percent-encoded host without relying on `/pull` path markers;
- `gh pr` and `hub search` command references;
- retained `gist.github.com` artifact diagnosis/selection coverage as public GitHub-hosted source leakage.

## Validation

- TDD baseline failure: `runs/20260707-senior-swe-bench-feedback-solution-reference-normalization-evidence/tdd-baseline-failures.txt`
- `cargo fmt --check`
- `CARGO_BUILD_JOBS=2 cargo test -p a2d cycle_input_feedback_rejects_public_solution_references_in_visible_feedback -- --nocapture`
- `CARGO_BUILD_JOBS=2 cargo test -p a2d cycle_input_feedback -- --nocapture`
- `CARGO_BUILD_JOBS=2 cargo test -p a2d --test senior_swe_bench_diagnose_artifact -- --nocapture`
- `CARGO_BUILD_JOBS=2 cargo test -p a2d --test senior_swe_bench_select_candidate_artifact -- --nocapture`
- `CARGO_BUILD_JOBS=2 cargo test`
- `cargo run -q -p a2d -- fitness-evidence-inspect runs/20260707-senior-swe-bench-feedback-solution-reference-normalization-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json --require-all-tests-pass`

The evidence is full-passing `a2d.fitness-evidence.v1` actual-test evidence with `source_diff_hash: b8d552350c3a5d24f46e3ba7b6a0e299f80c120e`, matching the scoped crates diff. `hidden_acceptance: not_present` is expected for this local score-artifact source-patch gate.

## Non-claims

This is feedback/artifact safety hardening only. It is not official Senior SWE-Bench mastery, not Senior SWE-Bench hidden-holdout evidence, not network no-egress enforcement, and not live provider-loop success evidence.

Postcommit clean-HEAD reconciliation after implementation commit `8a5a7c7`:

- `runs/20260707-postcommit-fitness-evidence-8a5a7c7-feedback/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`
- `runs/20260707-postcommit-fitness-evidence-8a5a7c7-feedback/fitness-evidence-inspect.txt`
- clean crates diff hash `e69de29bb2d1d6434b8b29ae775ad8c2e48c5391` with `source_tree_dirty: false`
