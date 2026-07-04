# Senior SWE-Bench Artifact Diagnosis — 2026-07-04

## Purpose

Distinguish the last live cycle-output finding before adding more Senior SWE-Bench checkout context. The captured provider artifact was not a patch; this slice classifies whether the artifact looks like missing checkout/tool context versus a generic output-contract failure, while preserving fail-closed extraction/evaluator gates.

## Lineage constraints

- Prior cycle-output artifact capture produced exact provider bytes and a manifest, but extraction rejected prose-only output.
- OpenCode artifact roles intentionally run pure/no-tools in isolated cwd, so a request to "inspect the local checkout" can be impossible unless checkout context is explicitly supplied through the allowed artifact path.
- Captured provider output is not fitness evidence; downstream extractor/evaluator gates must validate it before any Senior SWE-Bench claim.

## Change

Added `a2d senior-swe-bench-diagnose-artifact <artifact|->`, emitting `a2d.senior-swe-bench-artifact-diagnosis.v1` JSON. It reports whether a unified diff is extractable, whether public GitHub solution references are present, a bounded/redacted preview, and a diagnostic-only `failure_kind` such as `checkout_context_not_exercised`, `output_contract_not_followed`, `public_solution_reference`, or `candidate_patch_extractable`.

Extraction remains fail-closed. Public GitHub references are detected case-insensitively, and diagnostic previews are redacted when such references are present.

## Validation

- `cargo fmt --check`
- `cargo test -p a2d --test senior_swe_bench_diagnose_artifact -- --nocapture` — 4 passed
- `cargo test -p a2d senior_swe_bench_candidate_patch_extractor -- --nocapture`
- `cargo test` — 306 passed, 2 ignored
- Reviewer found two blockers (case-sensitive GitHub detection and preview leakage); both were fixed before persistence.

## Evidence

Diagnostic output for the previous live prose artifact:

- `runs/20260704-senior-swe-bench-artifact-diagnosis-evidence/diagnosis/prose-artifact-diagnosis.json`
- `failure_kind: checkout_context_not_exercised`
- `contains_unified_diff_candidate_patch: false`
- `note`: diagnostic only, not fitness evidence

Negative smoke:

- `runs/20260704-senior-swe-bench-artifact-diagnosis-evidence/negative-smoke/mixed-case-github.err`
- `runs/20260704-senior-swe-bench-artifact-diagnosis-evidence/negative-smoke/mixed-case-github.status` (`1`)

Fresh source-patch gate:

- `runs/20260704-senior-swe-bench-artifact-diagnosis-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`
- `schema_version: a2d.fitness-evidence.v1`
- `actual_tests_evaluated: true`
- `non_regressing: true`
- `fitness: 1.0`
- `failed_cases: []`
- `source_diff_hash: 1d526318a6426ba61cc79bda7c6c5c04a5867397`, matching `git diff --binary HEAD -- crates | git hash-object --stdin` before commit.

This gates the diagnostic CLI source patch only. It is not official Senior SWE-Bench mastery.
