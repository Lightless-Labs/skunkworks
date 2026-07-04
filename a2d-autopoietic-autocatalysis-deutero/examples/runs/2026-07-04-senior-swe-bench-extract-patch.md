# Senior SWE-Bench Candidate Patch Extraction — 2026-07-04

## Purpose

Bridge A²D cycle/coder output toward the Senior SWE-Bench evaluator by extracting a unified diff candidate patch from a raw artifact before calling `senior-swe-bench-evaluate`.

This is a narrow orchestration slice: it does not run a provider-generated Senior SWE-Bench solution yet, but it removes a manual step between “coder returns a diff-like artifact” and “evaluator receives a patch file”.

## Lineage constraints

- Senior SWE-Bench adapters stay in `crates/a2d-cli`; `a2d-core` remains generic.
- Prior cycle-input work asks coders to produce unified diff candidate patches, but evaluator replay requires a patch file.
- Foundry-style barriers require rejecting prose-only artifacts and artifacts that appear to cite public GitHub solution references.
- Evidence remains local-wrapper evidence unless the evaluator command is benchmark-provided and manifest-gated.

## Change

Added CLI command:

```bash
a2d senior-swe-bench-extract-patch <artifact|->
```

The command:

- accepts raw unified diffs;
- accepts fenced unified diffs embedded in prose;
- writes only the extracted patch to stdout;
- rejects artifacts without a unified diff;
- rejects artifacts containing obvious public GitHub solution references such as `github.com/`, `/pull/`, or `/commit/`.

## Validation

Focused checks:

```bash
cargo fmt --check
cargo test -p a2d senior_swe_bench_candidate_patch_extractor -- --nocapture
cargo test -p a2d senior_swe_bench -- --nocapture
```

Full suite:

```bash
cargo test
```

Result: 294 passed, 2 ignored.

## Senior SWE-Bench local-wrapper smoke

A prose-prefaced coder artifact was written to `runs/20260704-senior-swe-bench-extract-patch-evidence/local-evaluator/coder-output.md`; `a2d senior-swe-bench-extract-patch` extracted the fenced diff to `candidate.diff`; then `a2d senior-swe-bench-evaluate --apply-candidate-patch` evaluated that extracted patch in an isolated checkout.

Evidence artifact: `runs/20260704-senior-swe-bench-extract-patch-evidence/local-evaluator/fitness/senior-swe-bench-firezone-fix-connlib-align-device-hard-cycle-0-fitness-evidence.json`

Inspected fields:

- `schema_version: a2d.fitness-evidence.v1`
- `actual_tests_evaluated: true`
- `non_regressing: true`
- `fitness: 1.0` (3/3)
- `failed_cases: []`
- result labels: `all_tests_pass`, `has_no_solution_search`, `hidden_acceptance`
- `candidate_patch_hash: e61dbc5efcae0fda0dca699dae5c782d69e9e5e3`
- `candidate_patch_applied: true`
- `evaluator_checkout_mode: isolated_copy`
- `original_checkout_mutated: false`
- `candidate_patch_preflight_status: passed`
- `evaluator_kind: provided_local_command`
- `source_diff_hash: a32c3577c381cd056a13aa6de4d7d982fd75454e`

Local evaluation artifact: `runs/20260704-senior-swe-bench-extract-patch-evidence/local-evaluator/firezone-fix-connlib-align-device-hard-extracted-patch-local-evaluation.json` recorded `github_solution_search_allowed: false` and `stdout_preview: senior-swe-bench-extracted-patch-ok`.

## Source-patch gate evidence

Command:

```bash
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260704-senior-swe-bench-extract-patch-evidence/actual-test-score-artifact \
  cargo run -q -p a2d -- score-artifact sudoku runs/20260701-score-artifact-fitness-evidence/good-sudoku-artifact.rs
```

Artifact: `runs/20260704-senior-swe-bench-extract-patch-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`

Inspected fields:

- `schema_version: a2d.fitness-evidence.v1`
- `actual_tests_evaluated: true`
- `non_regressing: true`
- `fitness: 1.0` (6/6)
- `failed_cases: []`
- result labels include `all_tests_pass`
- `source_diff_scope: crates`
- `source_diff_hash: a32c3577c381cd056a13aa6de4d7d982fd75454e`

This gates the CLI extraction source patch and proves the extracted diff can feed the existing local evaluator wrapper. It is not an official Senior SWE-Bench mastery claim.
