# Senior SWE-Bench Retry Next-Gate Real Spawn Fail-Closed Coverage

**Date:** 2026-07-07
**Scope:** Retry next-gate/current-exe spawn coverage; not official Senior SWE-Bench mastery.

## Lineage

The retry-chain smoke proved a controller/status chain through inspected evidence, but its next-cycle provider boundary used a fixture summary. The remaining gap was the `FromRetryExecution` controller branch that actually spawns the current `a2d cycle-input` executable from a persisted retry execution. That branch needed evidence that checkout validation failures fail closed before provider invocation and cannot be masked by provider stderr text that merely resembles checkout-validation output after the catalytic cycle starts.

## Change

Added real-spawn integration coverage in `crates/a2d-cli/tests/senior_swe_bench_retry_execute.rs`:

- `retry_run_next_gate_from_retry_execution_spawns_current_exe_cycle_input_and_fails_closed_before_provider_on_invalid_checkout` runs an initial failed retry execution, deletes the checkout source context, then invokes `senior-swe-bench-retry-run-next-gate --retry-execution <retry-execution.json>`. The controller spawns the current `a2d cycle-input` child, persists the child/controller summaries, and reports `cycle_input_failed_before_provider: true`, `provider_invocations_started_by_this_command: false`, no evaluator/evidence-inspection side effects, no output manifest, and no chained resume plan.
- Unit coverage in `crates/a2d-cli/src/main.rs` now also simulates a spawned `cycle-input` run whose stdout already contains `A²D Catalytic Cycle` while stderr contains checkout-like text from provider execution. That negative case remains `cycle_input_failed_before_provider: false` and reports provider activity as possible/started.

The classifier is deliberately narrow: it refuses pre-provider classification once the child stdout shows the catalytic-cycle phase marker, and otherwise accepts only known pre-provider `cycle-input` validation prefixes at the beginning of stderr.

## Validation

```bash
cargo fmt --check
CARGO_BUILD_JOBS=2 cargo test -p a2d retry_run_next_cycle -- --nocapture
CARGO_BUILD_JOBS=2 cargo test -p a2d --test senior_swe_bench_retry_execute retry_run_next_gate_from_retry_execution_spawns_current_exe_cycle_input_and_fails_closed_before_provider_on_invalid_checkout -- --nocapture
CARGO_BUILD_JOBS=2 cargo test -p a2d --test senior_swe_bench_retry_execute -- --nocapture
CARGO_BUILD_JOBS=2 cargo test
```

Fresh source-patch evidence:

```bash
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260707-retry-next-gate-real-spawn-evidence/actual-test-score-artifact \
  cargo run -q -p a2d -- score-artifact sudoku runs/20260701-score-artifact-fitness-evidence/good-sudoku-artifact.rs

cargo run -q -p a2d -- fitness-evidence-inspect \
  runs/20260707-retry-next-gate-real-spawn-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json \
  --require-all-tests-pass
```

Evidence summary: `a2d.fitness-evidence.v1`, `actual_tests_evaluated: true`, `non_regressing: true`, `fitness: 1.0`, `passed: 6`, `total: 6`, `source_diff_scope: crates`, `source_diff_hash: a2817d73e38676bcef969de6eff9e2e6c80cea7f`, matching the scoped crates diff.

## Interpretation

This is retry-controller fail-closed coverage for the real current-exe `cycle-input` boundary. It proves invalid checkout context stops before provider invocation and that provider stderr containing similar text after invocation is not misclassified. It is not official Senior SWE-Bench mastery, not OS/network no-search enforcement, and not a full live provider-loop success proof.
