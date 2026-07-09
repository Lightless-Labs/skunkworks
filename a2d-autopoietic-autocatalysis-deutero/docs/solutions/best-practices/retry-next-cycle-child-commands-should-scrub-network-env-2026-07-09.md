---
module: a2d-cli
problem_type: subprocess-boundary-hardening
tags: [senior-swe-bench, retry-loop, process-env, no-public-solution-search, defense-in-depth]
---

# Retry next-cycle child commands should scrub network env

The Senior SWE-Bench retry loop can bridge from deterministic retry gates back into a live `cycle-input` provider run through `senior-swe-bench-retry-run-next-cycle`. That boundary spawns the current `a2d` executable to run the persisted next-cycle command, so it is a provider-adjacent subprocess boundary: it should not inherit ambient proxy/package-manager network configuration, and it should receive explicit no-public-solution-search policy env.

`run_retry_next_cycle_command` now constructs the child `Command`, applies `provider_no_public_solution_search_env()`, and removes the shared provider network/proxy/package-manager env list before spawning. This is policy observability and environment defense-in-depth only; it is not OS/network no-egress enforcement.

Regression coverage:

- `retry_next_cycle_child_process_receives_policy_env_and_scrubbed_network_env` runs a recursive test-binary fixture through the real `run_retry_next_cycle_command` path.
- The parent fixture injects every provider scrub-list key with sentinel values and sets a short retry-next-cycle timeout for test reliability.
- The child fixture asserts all provider no-public-solution-search policy env values are present and every provider scrub-list key is absent.

Fresh source-patch evidence: `runs/20260708-retry-next-cycle-child-env-hardening-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, full-passing `a2d.fitness-evidence.v1` with `source_diff_hash: 39addaa048d4a6576a70d543efcbccd0d371146d`, matching the scoped crates diff.
