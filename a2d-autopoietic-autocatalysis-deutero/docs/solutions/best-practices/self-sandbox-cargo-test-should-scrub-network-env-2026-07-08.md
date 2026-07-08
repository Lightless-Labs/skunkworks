---
module: self_sandbox
problem_type: subprocess-boundary-hardening
tags: [self-sandbox, process-env, no-public-solution-search, defense-in-depth]
---

# Self-sandbox cargo test should scrub network env

A²D's self-sandbox validates automated `SystemPatch` batches by copying the source tree and running `cargo test` in the isolated copy. That child process is part of the self-modification acceptance boundary: it should not inherit parent proxy/package-manager network configuration, and it should receive explicit no-public-solution-search policy env.

The self-sandbox now applies the shared `a2d-core` process-env helpers to its `cargo test` command before spawn. This is policy observability and environment defense-in-depth only; it is not OS/network no-egress enforcement.

Regression coverage:

- `self_sandbox_cargo_test_receives_policy_env_and_scrubbed_network_env` injects every shared scrub-list key into a recursive fixture process.
- `self_sandbox_cargo_test_child_process_env_fixture` patches a minimal fixture workspace test to assert the nested self-sandbox `cargo test` sees all generic policy env values while every shared network/proxy/package-manager env key is absent.
- The TDD baseline failed before the implementation because nested Cargo inherited `CARGO_HTTP_CHECK_REVOKE` from the parent sentinel env.

Fresh source-patch evidence: `runs/20260708-self-sandbox-env-hardening-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, full-passing `a2d.fitness-evidence.v1` with `source_diff_hash: d252e9a7f9596fdf3a21faf020b97a7693049c07`, matching the scoped crates diff.
