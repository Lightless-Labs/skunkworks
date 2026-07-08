# 2026-07-08 — Senior SWE-Bench Evaluator Network Env Scrub

**Scope:** Senior SWE-Bench evaluator subprocess boundary hardening; defense-in-depth only.

## Change

`senior-swe-bench-evaluate` now applies the shared CLI provider network-env scrubber to the evaluator subprocess before spawn. The evaluator still receives explicit A²D task/no-public-solution-search policy env flags, but no longer inherits the shared scrub list of proxy/package-manager network configuration.

The scrub list is intentionally bounded and shared with provider CLI subprocesses. It is not OS/network namespace isolation or no-egress proof.

## Validation

Commands run:

```bash
cargo fmt --check
CARGO_BUILD_JOBS=2 cargo test -p a2d --test senior_swe_bench_retry_attempt_evaluate retry_attempt_evaluate_scrubs_network_env_while_preserving_no_search_policy_env -- --nocapture
CARGO_BUILD_JOBS=2 cargo test -p a2d-providers network_env_scrub_preserves_no_public_solution_search_policy_env -- --nocapture
CARGO_BUILD_JOBS=2 cargo test
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260708-senior-swe-bench-evaluator-network-env-scrub-evidence/actual-test-score-artifact \
  cargo run -q -p a2d -- score-artifact sudoku runs/20260701-score-artifact-fitness-evidence/good-sudoku-artifact.rs
cargo run -q -p a2d -- fitness-evidence-inspect \
  runs/20260708-senior-swe-bench-evaluator-network-env-scrub-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json \
  --require-all-tests-pass
# Behavior-specific local-wrapper evidence: run senior-swe-bench-evaluate with proxy/package-manager env injected.
# The evaluator script exits 42 if any shared scrub-list key leaks and also requires no-search policy env to remain present.
cargo run -q -p a2d -- fitness-evidence-inspect \
  runs/20260708-senior-swe-bench-evaluator-network-env-scrub-evidence/local-evaluator/fitness/senior-swe-bench-env-scrub-hard-cycle-0-fitness-evidence.json \
  --require-all-tests-pass
```

The focused evaluator regression injects network env values using per-command `Command::env`; it does not mutate global process env, which keeps full parallel `cargo test` stable. The committed local-wrapper behavior evidence exercises the changed `senior-swe-bench-evaluate` evaluator subprocess path directly; it is still `provided_local_command`, not official Senior SWE-Bench evidence.

## Evidence

- `runs/20260708-senior-swe-bench-evaluator-network-env-scrub-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`
- `runs/20260708-senior-swe-bench-evaluator-network-env-scrub-evidence/local-evaluator/fitness/senior-swe-bench-env-scrub-hard-cycle-0-fitness-evidence.json`
- `runs/20260708-senior-swe-bench-evaluator-network-env-scrub-evidence/local-evaluator/local-evaluation.json`
- `runs/20260708-senior-swe-bench-evaluator-network-env-scrub-evidence/validation-summary.json`
- `source_diff_hash: bcaaa373faa64dea9850b5c9b52bd1e96324cdaf`

This gates the source patch and documents the evaluator subprocess boundary. It is not network-forensics evidence, official Senior SWE-Bench evidence, hidden official holdout proof, or repeated autonomous benchmark mastery.
