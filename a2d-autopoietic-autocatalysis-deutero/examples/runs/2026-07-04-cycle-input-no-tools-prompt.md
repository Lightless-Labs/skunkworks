# Cycle Input No-Tools Prompt Hardening — 2026-07-04

## Purpose

Lineage search showed a gap after `cycle-input --checkout`: A²D correctly supplied bounded checkout context as artifact text, but a live Senior SWE-Bench smoke still produced a provider artifact that tried to inspect the isolated provider temp directory with shell commands. Because providers are no-tools/artifact-only, the checkout-context prompt must say explicitly that filesystem inspection is unavailable and that the model must solve from the supplied snapshot text.

## Change

`enrich_cycle_input_with_checkout` now injects a critical no-tools rule into the coder-visible `design` artifact:

- provider invocations are no-tools/artifact-only in isolated temporary working directories;
- the coder cannot run `ls`, `cat`, `find`, `grep`, shell commands, or filesystem inspection tools against the benchmark checkout;
- the coder must use only the supplied checkout snapshot text;
- the coder must return only a unified diff candidate patch.

Senior SWE-Bench/check-out orchestration remains in `a2d-cli`; no `a2d-core` boundary change was made.

## Validation

Focused checks:

```bash
cargo fmt --check
cargo test -p a2d cycle_input_checkout_context_enriches_design_without_trusting_bundle -- --nocapture
```

Full suite:

```bash
cargo test
```

Result: 309 passed, 2 ignored.

Independent reviewer found no blockers or warnings.

## Live diagnostic smoke

Run directory: `runs/20260704-cycle-input-no-tools-prompt-evidence/`.

A bounded local Senior SWE-Bench-style smoke ran:

```bash
A2D_GERMLINE=seed A2D_PROVIDER_TIMEOUT_SECS=90 A2D_MAX_CYCLE_SECS=120 \
  cargo run -q -p a2d -- cycle-input \
  runs/20260703-senior-swe-bench-cycle-input-replay-evidence/task-cycle-input/firezone-fix-connlib-align-device-hard-cycle-input.json \
  1 \
  --checkout runs/20260704-senior-swe-bench-artifact-evaluate-evidence/local-evaluator/checkout \
  --output-artifacts runs/20260704-cycle-input-no-tools-prompt-evidence/live-cycle/artifacts
```

Captured provider artifact:

- `runs/20260704-cycle-input-no-tools-prompt-evidence/live-cycle/artifacts/cycle-0-wc-0001-coder-code.artifact`
- `senior-swe-bench-diagnose-artifact` classified it as `candidate_patch_extractable`.
- `senior-swe-bench-extract-patch` exited 0 and produced a non-empty 1204-byte diff at `live-cycle/extracted.diff`.

The generated patch is only a toy local-checkout patch, not a benchmark-quality solution.

## Local-wrapper evaluator evidence

Artifact: `runs/20260704-cycle-input-no-tools-prompt-evidence/local-evaluator/fitness/senior-swe-bench-firezone-fix-connlib-align-device-hard-cycle-0-fitness-evidence.json`

Inspected fields:

- `schema_version: a2d.fitness-evidence.v1`
- `actual_tests_evaluated: true`
- `non_regressing: true`
- `fitness: 1.0` (3/3)
- `failed_cases: []`
- result labels: `all_tests_pass`, `has_no_solution_search`, `hidden_acceptance`
- `candidate_patch_artifact_hash: 21c8d1356ddeee1072d18f44726ba5f40e4d3099`
- `candidate_patch_hash: 10a74cdccff79e476762ca8b0a4669b321ba1de5`
- `candidate_patch_applied: true`
- `evaluator_checkout_mode: isolated_copy`
- `original_checkout_mutated: false`
- `candidate_patch_preflight_status: passed`
- `evaluator_kind: provided_local_command`
- `source_diff_scope: crates`
- `source_diff_hash: 49431e677d4f4338b5cc71a125d5602e3c1eb3ca`

This proves the captured artifact can flow through extraction, isolated patch application, no-solution-search policy checks, and local-wrapper evidence binding. It is not official Senior SWE-Bench mastery because the evaluator kind is `provided_local_command`, not `official_senior_swe_bench` with a benchmark-provided manifest/holdouts.

## Source-patch gate evidence

Artifact: `runs/20260704-cycle-input-no-tools-prompt-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`

Inspected fields:

- `schema_version: a2d.fitness-evidence.v1`
- `actual_tests_evaluated: true`
- `non_regressing: true`
- `fitness: 1.0` (6/6)
- `failed_cases: []`
- result label includes `all_tests_pass`
- `source_diff_scope: crates`
- `source_diff_hash: 49431e677d4f4338b5cc71a125d5602e3c1eb3ca`

Verified hash:

```bash
git diff --binary HEAD -- crates | git hash-object --stdin
# 49431e677d4f4338b5cc71a125d5602e3c1eb3ca
```

This gates the source prompt hardening. Docs and run artifacts are supporting persistence outside the `crates` source-hash scope.
