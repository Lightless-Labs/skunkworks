# Senior SWE-Bench Artifact Evaluation — 2026-07-04

## Purpose

Close the next local orchestration gap after candidate-patch extraction: allow `senior-swe-bench-evaluate` to consume a coder/cycle artifact directly, materialize the extracted diff, and bind evaluator evidence to both the raw artifact and extracted patch.

## Lineage constraints

- Senior SWE-Bench remains CLI/evaluation-layer only; `a2d-core` stays generic.
- Candidate patch bytes were already bound to evidence, but raw coder artifacts were not.
- No-public-solution-search barriers require artifact extraction to reject public GitHub solution references and mismatched pre-existing extracted patch files before any evaluator runs.
- Evidence is still `provided_local_command` unless a real benchmark-provided official evaluator manifest/holdouts are used.

## Change

`a2d senior-swe-bench-evaluate` now accepts either the existing patch-file path or an artifact-to-patch path:

```bash
a2d senior-swe-bench-evaluate \
  --task-package <json> \
  --candidate-patch-artifact <coder-output.md> \
  --extracted-candidate-patch <candidate.diff> \
  --checkout <dir> \
  --apply-candidate-patch \
  -- <evaluator> [args...]
```

When artifact mode is used, the CLI extracts the unified diff with the existing policy gate, writes it to `--extracted-candidate-patch` when absent, rejects a pre-existing extracted file whose bytes do not match the artifact extraction, and records both:

- `candidate_patch_hash` / `candidate_patch_path`
- `candidate_patch_artifact_hash` / `candidate_patch_artifact_path`

in exported `a2d.fitness-evidence.v1` evidence.

## Validation

Focused checks:

```bash
cargo fmt --check
cargo test -p a2d senior_swe_bench_exported_fitness_evidence_binds_candidate_patch_hash -- --nocapture
cargo test -p a2d senior_swe_bench -- --nocapture
cargo test -p a2d exported_fitness_evidence_validation_requires_source_provenance -- --nocapture
```

Full suite:

```bash
cargo test
```

Result: 295 passed, 2 ignored.

The post-export binding verifier was also hardened to validate raw artifact path/hash when artifact mode is used; artifact hashing uses the exact on-disk/stdin bytes that are then decoded for diff extraction.

## Senior SWE-Bench local-wrapper smoke

Run directory: `runs/20260704-senior-swe-bench-artifact-evaluate-evidence/`.

The smoke wrote a prose-prefaced coder artifact to `local-evaluator/coder-output.md`, passed it directly to `senior-swe-bench-evaluate` via `--candidate-patch-artifact`, and let the evaluator wrapper materialize `local-evaluator/candidate.diff`. The local evaluator verified patched checkout content, preserved original checkout content, and the no-solution-search policy env before exiting successfully.

Evidence artifact: `runs/20260704-senior-swe-bench-artifact-evaluate-evidence/local-evaluator/fitness/senior-swe-bench-firezone-fix-connlib-align-device-hard-cycle-0-fitness-evidence.json`

Inspected fields:

- `schema_version: a2d.fitness-evidence.v1`
- `actual_tests_evaluated: true`
- `non_regressing: true`
- `fitness: 1.0` (3/3)
- `failed_cases: []`
- result labels: `all_tests_pass`, `has_no_solution_search`, `hidden_acceptance`
- `candidate_patch_hash: e61dbc5efcae0fda0dca699dae5c782d69e9e5e3`
- `candidate_patch_artifact_hash: 5f7013d405095ecf479f626c1ce7a38adf9a5cc4`
- `candidate_patch_applied: true`
- `evaluator_checkout_mode: isolated_copy`
- `original_checkout_mutated: false`
- `candidate_patch_preflight_status: passed`
- `evaluator_kind: provided_local_command`
- `source_revision: be662c6` (scoped `HEAD:crates` tree id)
- `source_diff_hash: 70b17bc9bbc8667c8ee3f0b2dfe94a3310a7991e`

Negative smoke: `runs/20260704-senior-swe-bench-artifact-evaluate-evidence/negative-smoke/mismatch.err` proves a pre-existing extracted diff that does not match the raw artifact is rejected before evaluator execution and no fitness evidence is emitted.

## Source-patch gate evidence

Artifact: `runs/20260704-senior-swe-bench-artifact-evaluate-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`

Inspected fields:

- `schema_version: a2d.fitness-evidence.v1`
- `actual_tests_evaluated: true`
- `non_regressing: true`
- `fitness: 1.0` (6/6)
- `failed_cases: []`
- result labels include `all_tests_pass`
- `source_diff_scope: crates`
- `source_diff_hash: 70b17bc9bbc8667c8ee3f0b2dfe94a3310a7991e`

This gates the artifact-evaluation source patch and proves the artifact-to-evaluator path locally. It is not an official Senior SWE-Bench mastery claim.
