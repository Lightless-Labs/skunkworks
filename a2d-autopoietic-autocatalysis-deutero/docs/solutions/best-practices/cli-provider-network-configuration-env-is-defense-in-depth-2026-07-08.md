---
module: a2d-providers
tags: [provider-boundary, senior-swe-bench, no-search-policy, defense-in-depth]
problem_type: best-practice
---

# CLI Provider Network Configuration Env Is Defense-in-Depth

## Problem

A²D already passes no-public-solution-search policy flags to CLI provider subprocesses, but provider CLIs still inherited parent proxy/package-manager network configuration such as `HTTP_PROXY`, `HTTPS_PROXY`, `ALL_PROXY`, `NO_PROXY`, `GIT_PROXY_COMMAND`, `CARGO_HTTP_PROXY`, and Rustup mirror variables.

That inheritance is not itself a public-solution-search permission, but it weakens the provider boundary by allowing ambient host network/proxy configuration to shape artifact-generation subprocesses.

## Practice

Before spawning CLI providers, remove common network/proxy/package-manager configuration variables from the child `Command` environment while still passing explicit A²D no-public-solution-search policy flags.

This is **defense-in-depth only**. It reduces ambient network configuration inheritance; it does **not** prove OS/network no-egress, syscall filtering, or benchmark-official isolation.

## Regression

`remove_network_configuration_env_drops_explicit_network_env` constructs a child command with explicit network configuration variables, applies the scrubber, and verifies the child no longer sees any scrubbed variable while unrelated sentinel env survives. The test avoids mutating process-global parent environment.

`network_env_scrub_preserves_no_public_solution_search_policy_env` applies both layers to the same child `Command`: all scrubbed network/proxy/package-manager keys are injected, A²D's explicit no-public-solution-search policy env is injected, then the scrubber runs. The child must see none of the network configuration keys and must still see every A²D policy flag. This pins the intended boundary: remove ambient network configuration without deleting explicit benchmark-integrity observability.

## Evidence

Fresh source-patch gate:

- `runs/20260708-provider-network-env-scrub-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`
- `schema_version: a2d.fitness-evidence.v1`
- `actual_tests_evaluated: true`
- `non_regressing: true`
- `fitness: 1.0`
- `source_diff_hash: 5e0c1967b4edc6cbf67b44edb2d8bc2cf49227d7`

Postcommit clean-head replay evidence:

- `runs/20260708-postcommit-fitness-evidence-provider-env-scrub/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`
- `source_tree_dirty: false`
- `source_diff_hash: e69de29bb2d1d6434b8b29ae775ad8c2e48c5391`

Follow-up policy-preservation coverage:

- `runs/20260708-provider-network-env-policy-preservation-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`
- `schema_version: a2d.fitness-evidence.v1`
- `actual_tests_evaluated: true`
- `non_regressing: true`
- `fitness: 1.0`
- `source_diff_hash: 686dd13acbb8d7b168ab9a29b8aef54f6653a479`

Senior SWE-Bench evaluator subprocesses reuse the same scrubber as of the follow-up evaluator-boundary slice:

- `runs/20260708-senior-swe-bench-evaluator-network-env-scrub-evidence/local-evaluator/fitness/senior-swe-bench-env-scrub-hard-cycle-0-fitness-evidence.json`
- `runs/20260708-senior-swe-bench-evaluator-network-env-scrub-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`
- `source_diff_hash: bcaaa373faa64dea9850b5c9b52bd1e96324cdaf`

Senior SWE-Bench evaluator subprocesses also reuse `provider_no_public_solution_search_env()` as of the policy-env parity follow-up, so the same generic and Senior-specific no-public-solution-search policy flags are visible to evaluator wrappers and their nested test runners:

- `runs/20260708-senior-swe-bench-evaluator-policy-env-parity-evidence/local-evaluator/fitness/senior-swe-bench-policy-env-parity-hard-cycle-0-fitness-evidence.json`
- `runs/20260708-senior-swe-bench-evaluator-policy-env-parity-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`
- `source_diff_hash: f564d504708fded164d2d1832d812accbb84bd3d`

Validation included `cargo fmt --check`, focused and full `a2d-providers` tests, focused evaluator-boundary tests, full `CARGO_BUILD_JOBS=2 cargo test`, reviewer re-review, and `fitness-evidence-inspect --require-all-tests-pass`.
