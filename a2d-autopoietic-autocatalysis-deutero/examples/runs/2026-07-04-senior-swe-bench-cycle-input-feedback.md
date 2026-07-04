# Senior SWE-Bench Cycle-Input Feedback — 2026-07-04

## Purpose

Close the next feedback-loop gap for Senior SWE-Bench cycle inputs: after a provider produces a candidate patch and a local/provided evaluator fails it, A²D needs a safe way to build the next `cycle-input` artifact from the previous task context plus evaluator feedback.

Prior findings constrained the design:

- `docs/solutions/architectural-insights/broken-feedback-loop-coder-never-sees-failure-output-2026-04-04.md` — the component that can fix the problem must see actionable failure feedback.
- `docs/solutions/best-practices/cycle-input-must-not-seed-runtime-evidence-2026-07-04.md` — task input must not impersonate `fitness_report` / `failure_report` / other runtime evidence.
- Foundry hidden-holdout patterns — official hidden-holdout output must not be fed back to coding agents.
- Senior SWE-Bench policy — no public GitHub/web solution-search references can enter coder context.

## Change

Added CLI command:

```bash
a2d senior-swe-bench-cycle-input-feedback <task-cycle-input.json|-> <local-evaluation.json|->
```

The command:

- parses a Senior SWE-Bench `task-cycle-input` artifact and a matching `a2d.senior-swe-bench-local-evaluation.v1` record;
- rejects task/repo mismatches and evaluations where GitHub solution search is allowed;
- injects previous evaluator feedback into coder-visible `design` and `plan` only;
- resets `evaluation.status` to `not_evaluated` and `evaluation.fitness` to `null` before the next cycle;
- rejects or redacts unsafe feedback:
  - official/hidden-holdout evaluator output is redacted by default;
  - local stdout/stderr is shown only with explicit `feedback_visibility: public_local_test_output`;
  - public GitHub solution references in preserved cycle input or visible evaluator output fail closed;
  - `status`/`evaluator` are enum-validated, `task_id`/`repo` are safe identifier validated, and `fitness_evidence_path` is omitted from coder-visible text;
  - top-level preserved cycle input fields are allowlisted to task context and recursively reject reserved runtime artifacts (`fitness_report`, `failure_report`, `system_patch`, `test_results`, `enzyme_defs`, `code`, etc.).

Senior SWE-Bench logic remains in `a2d-cli`; no `a2d-core` domain boundary was added.

## Validation

Focused checks:

```bash
cargo fmt --check
cargo test -p a2d cycle_input_feedback -- --nocapture
cargo test -p a2d --test senior_swe_bench_cycle_input_feedback -- --nocapture
```

Full suite:

```bash
cargo test
```

Result: 309 passed, 2 ignored.

Independent reviewer final pass found no blockers or actionable warnings.

## Feedback smokes

Run directory: `runs/20260704-senior-swe-bench-cycle-input-feedback-evidence/`.

Positive smoke:

- input: `feedback-smoke/task-cycle-input.json`
- evaluation: `feedback-smoke/failed-local-evaluation.json`
- output: `feedback-smoke/feedback-cycle-input.json`

The output keeps evaluation `not_evaluated` / `fitness: null`, injects public local-test feedback into `design`, omits the private evidence path, and does not seed `fitness_report` or `failure_report`.

Negative smokes:

- `negative-smoke/public-solution.err` — visible evaluator output containing a GitHub PR URL is rejected.
- `negative-smoke/reserved.err` — nested reserved runtime artifact (`system_patch`) in cycle input is rejected.

## Source-patch gate evidence

Artifact: `runs/20260704-senior-swe-bench-cycle-input-feedback-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`

Inspected fields:

- `schema_version: a2d.fitness-evidence.v1`
- `actual_tests_evaluated: true`
- `non_regressing: true`
- `fitness: 1.0` (6/6)
- `failed_cases: []`
- `source_diff_scope: crates`
- `source_diff_hash: dd390ea40414bb9c16a7aded24adae854c094d09`

Verified hash:

```bash
git diff --binary HEAD -- crates | git hash-object --stdin
# dd390ea40414bb9c16a7aded24adae854c094d09
```

This gates the CLI feedback source patch. Docs and run artifacts are supporting persistence outside the `crates` source-hash scope.

## Local-wrapper evidence

Artifact: `runs/20260704-senior-swe-bench-cycle-input-feedback-evidence/local-evaluator/fitness/senior-swe-bench-firezone-fix-connlib-align-device-hard-cycle-0-fitness-evidence.json`

Inspected fields:

- `schema_version: a2d.fitness-evidence.v1`
- `actual_tests_evaluated: true`
- `non_regressing: true`
- `fitness: 1.0`
- `failed_cases: []`
- result labels: `all_tests_pass`, `has_no_solution_search`, `hidden_acceptance`
- `evaluator_kind: provided_local_command`
- `source_diff_scope: crates`
- `source_diff_hash: dd390ea40414bb9c16a7aded24adae854c094d09`

This proves the source patch still supports the Senior SWE-Bench local-wrapper evidence path with hidden-acceptance status while preserving the no-solution-search policy. It remains `provided_local_command` evidence.

## Claim boundary

This is evaluator-feedback plumbing plus source-patch hidden-holdout replay evidence. It is not official Senior SWE-Bench mastery because no official Senior SWE-Bench evaluator/manifest/holdout run is claimed.
