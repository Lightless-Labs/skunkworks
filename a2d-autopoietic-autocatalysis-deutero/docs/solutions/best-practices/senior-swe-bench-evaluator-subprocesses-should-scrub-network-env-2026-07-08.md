---
module: a2d-cli
tags: [senior-swe-bench, evaluator-boundary, no-search-policy, defense-in-depth]
problem_type: best-practice
---

# Senior SWE-Bench Evaluator Subprocesses Should Scrub Network Env

## Problem

Provider CLI subprocesses already scrub common inherited proxy/package-manager network configuration while preserving explicit no-public-solution-search policy flags. Senior SWE-Bench evaluator subprocesses had the policy flags but did not reuse that scrubber, so a local or official evaluator wrapper could still inherit ambient proxy/Rust/Cargo network configuration from the parent process.

This is not the same as allowing public solution search, but it weakens the benchmark boundary and makes no-search evidence labels easier to overread.

## Practice

Before spawning the Senior SWE-Bench evaluator command, apply the same shared network-env scrubber used for provider CLI subprocesses:

- Set explicit evaluator policy/context env first.
- Remove inherited network/proxy/package-manager configuration with `remove_network_configuration_env` before `spawn`.
- Keep the scope explicit: this is defense-in-depth and policy-boundary hygiene, not OS/network no-egress enforcement.

## Regression

`retry_attempt_evaluate_scrubs_network_env_while_preserving_no_search_policy_env` injects the shared scrub list into the parent `a2d` command via per-command `Command::env`, not global process env. The evaluator script asserts:

- `A2D_SENIOR_SWE_BENCH_GITHUB_SOLUTION_SEARCH_ALLOWED=false`
- `A2D_SENIOR_SWE_BENCH_PUBLIC_SOLUTION_SEARCH_FORBIDDEN=true`
- every shared network/proxy/package-manager env key is absent

The test imports the public scrub list from `a2d_providers::cli::network_configuration_env_vars` so the regression cannot silently drift from the implementation list.

## Evidence

Fresh source-patch gate:

- `runs/20260708-senior-swe-bench-evaluator-network-env-scrub-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`
- `schema_version: a2d.fitness-evidence.v1`
- `actual_tests_evaluated: true`
- `non_regressing: true`
- `fitness: 1.0`
- `all_tests_pass: true`
- `source_diff_hash: bcaaa373faa64dea9850b5c9b52bd1e96324cdaf`

Behavior-specific evaluator evidence:

- `runs/20260708-senior-swe-bench-evaluator-network-env-scrub-evidence/local-evaluator/fitness/senior-swe-bench-env-scrub-hard-cycle-0-fitness-evidence.json`
- `runs/20260708-senior-swe-bench-evaluator-network-env-scrub-evidence/local-evaluator/local-evaluation.json`
- `evidence_command: senior-swe-bench-evaluate ... -- evaluator.sh`
- `source_diff_hash: bcaaa373faa64dea9850b5c9b52bd1e96324cdaf`
- local evaluator script exits 42 if any shared scrub-list env key leaks while requiring the no-search policy env flags to remain present

Validation included `cargo fmt --check`, focused evaluator/provider env-scrub tests, full `CARGO_BUILD_JOBS=2 cargo test`, reviewer with no blockers, and `fitness-evidence-inspect --require-all-tests-pass` on both the source-patch gate and behavior-specific evaluator evidence.

This is not official Senior SWE-Bench mastery, hidden official holdout proof, live provider-loop success, or no-egress proof.
