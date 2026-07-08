---
module: sandbox
problem_type: subprocess-boundary-hardening
tags: [sandbox, process-env, no-public-solution-search, defense-in-depth]
---

# Sandbox child processes should scrub network env

A²D's candidate-code sandbox compiles and runs provider-generated Rust. That boundary is as sensitive as provider/evaluator subprocesses: inherited proxy/package-manager network configuration should not leak into `rustc`, `rustc --test`, or the generated test binary, and the child process should see explicit no-public-solution-search policy env.

The sandbox now applies a generic `a2d-core` process-env helper to those child commands. `a2d-providers` reuses the same network scrub helper to avoid list drift. This is policy observability and defense-in-depth only; it is not OS/network no-egress enforcement.

Regression coverage:

- `sandbox_evaluated_code_receives_policy_env_and_scrubbed_network_env` recursively runs a generated Rust test through the sandbox and asserts the generated test process sees policy env while every shared network-env key is absent.
- Provider scrub regressions still pass after delegating the shared scrub list to `a2d-core`.

Fresh source-patch evidence: `runs/20260708-sandbox-child-env-hardening-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, full-passing `a2d.fitness-evidence.v1` with `source_diff_hash: c3b0190ed4982b7989a8e43eba7f7ceb6307a08a`.
