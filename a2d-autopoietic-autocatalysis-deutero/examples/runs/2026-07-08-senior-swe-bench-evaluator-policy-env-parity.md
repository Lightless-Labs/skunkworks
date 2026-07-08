# 2026-07-08 — Senior SWE-Bench Evaluator Policy Env Parity

**Scope:** Senior SWE-Bench evaluator subprocess policy-env observability; defense-in-depth only.

## Lineage

The provider subprocess boundary already exports five no-public-solution-search policy env flags, while the previous evaluator network-env scrub slice only verified the two Senior SWE-Bench-specific flags. A read-only scout identified the asymmetry: evaluator wrappers and their nested test runners did not receive the general A²D policy source marker or generic no-public-solution-search flags that provider subprocesses receive.

## Change

`senior-swe-bench-evaluate` now applies `provider_no_public_solution_search_env()` to the evaluator subprocess before the shared network-env scrubber runs. The evaluator receives:

- `A2D_PROVIDER_POLICY_ENV_SOURCE=a2d-cli-provider`
- `A2D_GITHUB_SOLUTION_SEARCH_ALLOWED=false`
- `A2D_PUBLIC_SOLUTION_SEARCH_FORBIDDEN=true`
- `A2D_SENIOR_SWE_BENCH_GITHUB_SOLUTION_SEARCH_ALLOWED=false`
- `A2D_SENIOR_SWE_BENCH_PUBLIC_SOLUTION_SEARCH_FORBIDDEN=true`

Senior SWE-Bench task/cycle-input parsers already reject `github_solution_search_allowed=true` before evaluator execution, so the shared provider policy env does not contradict an accepted allowed-search path. No such path is accepted.

## Validation

Commands run:

```bash
cargo fmt --check
CARGO_BUILD_JOBS=2 cargo test -p a2d --test senior_swe_bench_retry_attempt_evaluate retry_attempt_evaluate_scrubs_network_env_while_preserving_no_search_policy_env -- --nocapture
CARGO_BUILD_JOBS=2 cargo test -p a2d-providers network_env_scrub_preserves_no_public_solution_search_policy_env -- --nocapture
CARGO_BUILD_JOBS=2 cargo test
cargo run -q -p a2d -- fitness-evidence-inspect \
  runs/20260708-senior-swe-bench-evaluator-policy-env-parity-evidence/local-evaluator/fitness/senior-swe-bench-policy-env-parity-hard-cycle-0-fitness-evidence.json \
  --require-all-tests-pass
cargo run -q -p a2d -- fitness-evidence-inspect \
  runs/20260708-senior-swe-bench-evaluator-policy-env-parity-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json \
  --require-all-tests-pass
```

The behavior-specific evaluator wrapper exits if any of the five policy env flags are missing or if any shared scrub-list proxy/package-manager env key leaks. The same wrapper was run directly without A²D-provided env as a negative smoke and failed before any success/evidence claim.

## Evidence

- `runs/20260708-senior-swe-bench-evaluator-policy-env-parity-evidence/local-evaluator/fitness/senior-swe-bench-policy-env-parity-hard-cycle-0-fitness-evidence.json`
- `runs/20260708-senior-swe-bench-evaluator-policy-env-parity-evidence/local-evaluator/local-evaluation.json`
- `runs/20260708-senior-swe-bench-evaluator-policy-env-parity-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`
- `runs/20260708-senior-swe-bench-evaluator-policy-env-parity-evidence/validation-summary.json`
- `source_diff_hash: f564d504708fded164d2d1832d812accbb84bd3d`

This is policy-observability and evaluator-boundary evidence only. It is not OS/network no-egress proof, official Senior SWE-Bench evidence, hidden official holdout proof, or repeated autonomous benchmark mastery.
