# Senior SWE-Bench Candidate Artifact Selection — 2026-07-04

## Purpose

Close the deterministic handoff gap between `cycle-input --output-artifacts` and `senior-swe-bench-extract-patch`: a retry executor needs to select the exact coder artifact bytes from a cycle-output manifest before extraction/evaluation, without guessing or starting providers/evaluators.

## Change

Added CLI command:

```bash
a2d senior-swe-bench-select-candidate-artifact <cycle-output-manifest.json|->
```

The command reads `a2d.cycle-output-artifacts.v1`, requires exactly one `enzyme_id: coder` / `artifact_type: code` record, reads the referenced artifact bytes, verifies the manifest byte count and `git hash-object` hash, rejects public GitHub solution references, and emits `a2d.senior-swe-bench-candidate-artifact-selection.v1`.

The output records the selected artifact path/hash/provider/cycle/workcell, diagnosis fields, and `extract_patch_args`. It sets `provider_invocations_started: false`, `evaluator_invocations_started: false`, and `fitness_claim_allowed_before_evidence: false`.

## Validation

Focused checks:

```bash
cargo fmt --check
cargo test -p a2d --test senior_swe_bench_select_candidate_artifact -- --nocapture
cargo test -p a2d senior_swe_bench -- --nocapture
cargo test -p a2d cycle_output_artifact -- --nocapture
```

Full suite:

```bash
cargo test
```

Result: 327 passed, 2 ignored.

## Smokes

Run directory: `runs/20260704-senior-swe-bench-candidate-artifact-selection-evidence/`.

Positive smokes:

- `selection-smoke/selection.json` — valid single coder/code artifact is selected, rehashed, diagnosed as `candidate_patch_extractable`, and points to `senior-swe-bench-extract-patch`.
- `selection-smoke/prose-selection.json` — exact non-diff prose artifact is selected but diagnosed as `checkout_context_not_exercised`, not as evidence.

Negative smokes:

- `negative-smoke/hash-mismatch.err` — manifest hash mismatch fails closed.
- `negative-smoke/multiple.err` — multiple coder/code candidates fail closed instead of guessing.
- `negative-smoke/public.err` — valid diff plus trailing public GitHub URL fails closed before extraction/evaluation.

## Source-patch gate evidence

Artifact: `runs/20260704-senior-swe-bench-candidate-artifact-selection-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`

Inspected fields:

- `schema_version: a2d.fitness-evidence.v1`
- `actual_tests_evaluated: true`
- `non_regressing: true`
- `fitness: 1.0` (6/6)
- `failed_cases: []`
- `source_diff_scope: crates`
- `source_tree_dirty: true`
- `source_diff_hash: 5c973a79c75109b52d0b6e1349e9ceba84c3b6a9`

Verified hash:

```bash
git diff --binary HEAD -- crates | git hash-object --stdin
# 5c973a79c75109b52d0b6e1349e9ceba84c3b6a9
```

## Claim boundary

This is deterministic artifact-selection/handoff plumbing and source-patch evidence. It starts no providers/evaluators and is not official Senior SWE-Bench mastery.
