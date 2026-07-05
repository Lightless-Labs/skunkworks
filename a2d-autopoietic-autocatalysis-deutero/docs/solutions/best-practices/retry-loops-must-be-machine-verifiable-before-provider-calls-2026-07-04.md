---
module: a2d-cli
tags: [senior-swe-bench, retry-loop, evidence-gates, information-barriers]
problem_type: best-practice
---

# Retry loops must be machine-verifiable before provider calls

## Problem

After a failed Senior SWE-Bench candidate evaluation, it is tempting to wire an automatic retry loop immediately. That risks creating cosmetic autonomy: providers may be invoked repeatedly without bounded attempts, clear stop criteria, or a fresh `a2d.fitness-evidence.v1` gate for any success claim.

## Pattern

Before starting providers or evaluators, emit a machine-readable retry plan that states:

- maximum attempts and explicit exhaustion behavior;
- the required per-attempt gates (`cycle-input` output capture, patch extraction, evaluator run, evidence inspection);
- fail-closed stops for extraction failure and policy/evidence-binding mismatch;
- success only after non-regressing actual-test `a2d.fitness-evidence.v1`;
- information barriers for no public GitHub solution search and hidden-holdout redaction.

Then add a deterministic retry-step gate that consumes the plan plus a single evaluation result and chooses only the next safe action. The step should validate schema-complete plan fields (including stop criteria, barriers, and per-attempt transitions), produce feedback-enriched next cycle input only for failed non-final attempts, and treat passed evaluations as instructions to inspect evidence rather than as fitness claims.

Before wiring a loop executor, add a retry-attempt planner that composes existing gates into command arguments only. It should select the exact coder artifact by manifest provenance, verify unified-diff extractability, emit planned extraction/evaluation/retry-step args, and stop with no evaluator args when extraction fails.

Then execute only one planned command at a time. The extraction executor should re-verify selected artifact bytes/hash, re-run patch extraction/public-solution rejection, require the extracted patch hash to match the plan, and write the planned patch path idempotently. It should emit the next evaluation/retry-step args but still start no providers or evaluators.

The evaluator executor may run exactly one planned `senior-swe-bench-evaluate` command, but only after re-validating the extraction artifact, candidate patch, selected artifact, exact evaluator argv, and downstream retry-step argv. It must validate the local evaluation JSON independently of wrapper exit code and must not run retry-step or evidence inspection; a passed evaluation only points at evidence for the next gate.

The step executor may then run exactly one planned `senior-swe-bench-retry-step` command, but only after re-validating the retry-attempt evaluation, patch/artifact provenance, local-evaluation source provenance, local-vs-official evaluator provenance, and retry-step argv. It must not run providers, evaluators, or `fitness-evidence-inspect`; it should record prior evaluator execution separately from evaluator execution started by the step command itself.

Finally, the evidence-inspection executor may run exactly one planned `fitness-evidence-inspect --require-all-tests-pass` command, but only for a completed step-execution artifact whose embedded retry-step decision is `inspect_fitness_evidence`. It should re-check selected artifact and candidate patch hashes, require no provider/evaluator invocations for the current command, re-read the inspected `a2d.fitness-evidence.v1`, and bind `candidate_patch_hash` plus reviewed evaluator provenance before allowing downstream success claims.

The plan/step/attempt/extraction/evaluation/step-execution artifacts are not evidence and must not claim fitness. Even the evidence-inspection execution artifact is only a pointer to an inspected `a2d.fitness-evidence.v1`; claims must still cite the actual evidence path and evaluator kind.

If a terminal retry-run summary is needed, build it as a thin verifier over the passed step-evidence execution rather than as new evidence. It must re-read the underlying `a2d.fitness-evidence.v1`, run the same all-tests-pass inspection, rebuild summary fields from the inspected evidence, reject forged summaries, and keep `provided_local_command` distinct from `official_senior_swe_bench` so local-wrapper success cannot become an official benchmark claim.

A bounded executor can then compose these gates, but it should not jump directly to unbounded provider/cycle recursion. The first safe executor shape consumes precomputed cycle-output manifests, validates the existing retry plan and cycle input for each attempt, then calls the existing deterministic gate builders in order. It must report whether it started providers (false), whether evaluator execution occurred, why execution stopped, and whether `fitness-evidence-inspect` passed. If callers provide fewer precomputed manifests than `max_attempts`, stop with a distinct precomputed-manifest-exhausted reason instead of pretending the retry budget was fully spent.

## Evidence

Implemented by `a2d senior-swe-bench-retry-plan`, `a2d senior-swe-bench-retry-step`, `a2d senior-swe-bench-retry-attempt-plan`, `a2d senior-swe-bench-retry-attempt-extract-patch`, `a2d senior-swe-bench-retry-attempt-evaluate`, `a2d senior-swe-bench-retry-attempt-step`, `a2d senior-swe-bench-retry-attempt-step-evidence`, `a2d senior-swe-bench-retry-run-result`, and `a2d senior-swe-bench-retry-execute` in `crates/a2d-cli/src/main.rs` / `crates/a2d-cli/src/senior_swe_bench.rs`, with CLI coverage in `crates/a2d-cli/tests/senior_swe_bench_retry_plan.rs`, `crates/a2d-cli/tests/senior_swe_bench_retry_step.rs`, `crates/a2d-cli/tests/senior_swe_bench_retry_attempt_plan.rs`, `crates/a2d-cli/tests/senior_swe_bench_retry_attempt_extract_patch.rs`, `crates/a2d-cli/tests/senior_swe_bench_retry_attempt_evaluate.rs`, `crates/a2d-cli/tests/senior_swe_bench_retry_attempt_step.rs`, `crates/a2d-cli/tests/senior_swe_bench_retry_attempt_step_evidence.rs`, `crates/a2d-cli/tests/senior_swe_bench_retry_run_result.rs`, and `crates/a2d-cli/tests/senior_swe_bench_retry_execute.rs`.

Fresh retry-plan source-patch gate evidence: `runs/20260704-senior-swe-bench-retry-plan-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, full-passing with `source_diff_hash: fa652ee3ca175bb5cb37d15ab106840d17c37f84`.

Fresh retry-step source-patch gate evidence: `runs/20260704-senior-swe-bench-retry-step-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, full-passing with `source_diff_hash: f2073d167fcddc55903fa19d8e459d8b25d29e0c`.

Fresh retry-attempt-plan source-patch gate evidence: `runs/20260704-senior-swe-bench-retry-attempt-plan-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, full-passing with `source_diff_hash: 95e5f526834ef5bce322ab1f474e9f9ef5fcba0b`.

Fresh retry-attempt-extract-patch source-patch gate evidence: pre-commit dirty-tree evidence `runs/20260704-senior-swe-bench-retry-attempt-extract-patch-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, full-passing with `source_diff_hash: aa8a4df265620b7ef92923a8e18ea693fd54b7b0`; post-commit clean-HEAD evidence `runs/20260704-postcommit-fitness-evidence-b0abcf6/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, full-passing with `source_revision: c7cd22e`, `source_tree_dirty: false`, and `source_diff_hash: e69de29bb2d1d6434b8b29ae775ad8c2e48c5391`.

Fresh retry-attempt-evaluate source-patch gate evidence: `runs/20260704-senior-swe-bench-retry-attempt-evaluate-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, full-passing with `source_revision: c7cd22e`, `source_tree_dirty: true`, `source_diff_scope: crates`, and `source_diff_hash: 24663b72742e27d26a149b2d7c7330bf195471ad`.

Fresh retry-attempt-step source-patch gate evidence: `runs/20260704-senior-swe-bench-retry-attempt-step-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, full-passing with `source_revision: dd6543b`, `source_tree_dirty: true`, `source_diff_scope: crates`, and `source_diff_hash: 405790bbe8085004f63156b9267997c29e34cc9a`. Step smoke `runs/20260704-senior-swe-bench-retry-attempt-step-evidence/step-smoke/step-execution.json` records `evaluator_invocations_started: false`, `prior_evaluator_invocations_started: true`, `retry_step_started: true`, and `fitness_evidence_inspection_started: false`.

Fresh retry-attempt-step-evidence source-patch gate evidence: `runs/20260704-senior-swe-bench-retry-attempt-step-evidence-inspect-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, full-passing with `source_diff_scope: crates` and `source_diff_hash: 51733d777b8c4356279f7b970281bbd330ad0709`. Step-evidence smoke `runs/20260704-senior-swe-bench-retry-attempt-step-evidence-inspect-evidence/step-evidence-smoke/step-evidence-execution.json` records `fitness_evidence_inspection_started: true`, `fitness_evidence_inspection_passed: true`, `provider_invocations_started: false`, `evaluator_invocations_started: false`, and a full-passing inspected local-wrapper evidence summary with `evaluator_kind: provided_local_command`.

Fresh retry-run-result source-patch gate evidence: `runs/20260705-senior-swe-bench-retry-run-result-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, full-passing with `source_diff_scope: crates` and `source_diff_hash: a510ff65b833c3b5589c32d0b37a6e8394b2d611`. The command emits a run-result summary only; the underlying inspected `a2d.fitness-evidence.v1` remains the authoritative evidence and local-wrapper results remain non-official.

Fresh retry-execute source-patch gate evidence: `runs/20260705-senior-swe-bench-retry-execute-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, full-passing with `source_diff_scope: crates` and `source_diff_hash: fee83715a54f10f0e73e8c8bbf73591e6cb921e8`; post-commit clean-HEAD evidence for `ada37b2` lives at `runs/20260705-postcommit-fitness-evidence-ada37b2/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json` and passes `fitness-evidence-inspect --require-all-tests-pass` with clean `source_diff_hash: e69de29bb2d1d6434b8b29ae775ad8c2e48c5391`.

Run docs: `examples/runs/2026-07-04-senior-swe-bench-retry-plan.md`, `examples/runs/2026-07-04-senior-swe-bench-retry-step.md`, `examples/runs/2026-07-04-senior-swe-bench-retry-attempt-plan.md`, `examples/runs/2026-07-04-senior-swe-bench-retry-attempt-extract-patch.md`, `examples/runs/2026-07-04-senior-swe-bench-retry-attempt-evaluate.md`, `examples/runs/2026-07-04-senior-swe-bench-retry-attempt-step.md`, `examples/runs/2026-07-04-senior-swe-bench-retry-attempt-step-evidence.md`, `examples/runs/2026-07-05-senior-swe-bench-retry-run-result.md`, and `examples/runs/2026-07-05-senior-swe-bench-retry-execute.md`.
