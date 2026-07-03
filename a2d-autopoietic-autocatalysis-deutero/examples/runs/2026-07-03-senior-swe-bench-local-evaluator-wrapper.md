# Senior SWE-Bench Local Evaluator Wrapper — 2026-07-03

## Purpose

Move one step beyond catalog/task packaging by adding a CLI-only wrapper that can run a caller-provided local Senior SWE-Bench evaluator command against a benchmark-provided checkout and candidate patch. Senior SWE-Bench-specific code remains outside `a2d-core`.

## Lineage constraints

- The previous task-package slice was explicitly `not_evaluated`; it did not prove task fitness.
- Coding agents must not search GitHub/public web for benchmark solutions.
- Source patches require fresh non-regressing `a2d.fitness-evidence.v1` actual-test evidence with holdout status before persistence.

## Change

Added:

```bash
a2d senior-swe-bench-evaluate \
  --task-package <json> \
  --candidate-patch <diff> \
  --checkout <dir> \
  [--output <json>] \
  -- <local-evaluator> [args...]
```

Behavior:

- Parses `a2d.senior-swe-bench-task-package.v1`.
- Refuses packages with `github_solution_search_allowed: true`.
- Runs the provided evaluator command in the supplied checkout.
- Passes task metadata via `A2D_SENIOR_SWE_BENCH_TASK_ID`, `A2D_SENIOR_SWE_BENCH_REPO`, and `A2D_SENIOR_SWE_BENCH_CANDIDATE_PATCH`.
- Captures stdout/stderr to files while waiting, avoiding pipe-buffer deadlocks.
- Emits `a2d.senior-swe-bench-local-evaluation.v1`.
- Records `candidate_patch_hash` using `git hash-object -- <candidate-patch>` so the local evaluation is bound to the exact candidate diff bytes.
- Exports `a2d.fitness-evidence.v1` only for full-passing evaluator outcomes. Failed evaluator outcomes still emit evaluation JSON and exit nonzero, but do not produce non-regressing evidence.
- Includes `candidate_patch_hash` in exported fitness evidence and rejects malformed/non-string candidate patch hash fields during export validation.
- Includes `evaluator_kind: provided_local_command` in newly exported local-wrapper fitness evidence; validation accepts only reviewed evaluator kinds while staying backward-compatible for older generic evidence without that optional field.
- Re-reads the emitted fitness evidence and verifies both its `candidate_patch_hash` and evaluator-kind provenance against the current candidate patch file before reporting the evidence path.

## Validation

```bash
cargo fmt --check
cargo test -p a2d fitness_evidence -- --nocapture
cargo test -p a2d senior_swe_bench -- --nocapture
cargo test
```

Full suite result after evaluator-kind provenance validation: 274 passed, 2 ignored.

Architecture boundary:

```bash
rg -n "senior_swe_bench|SeniorSweBench|Senior SWE-Bench|senior-swe-bench" crates/a2d-core
```

No `a2d-core` matches.

## Fresh fitness evidence

Passing local evaluator smoke:

```bash
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260703-senior-swe-bench-local-evaluator-evidence/local-evaluator/fitness \
  cargo run -q -p a2d -- senior-swe-bench-evaluate \
  --task-package runs/20260703-senior-swe-bench-local-evaluator-evidence/task-package/firezone-fix-connlib-align-device-hard-package.json \
  --candidate-patch runs/20260703-senior-swe-bench-local-evaluator-evidence/local-evaluator/candidate.diff \
  --checkout runs/20260703-senior-swe-bench-local-evaluator-evidence/checkout \
  --output runs/20260703-senior-swe-bench-local-evaluator-evidence/local-evaluator/firezone-fix-connlib-align-device-hard-local-evaluation.json \
  -- $PWD/runs/20260703-senior-swe-bench-local-evaluator-evidence/local-evaluator/mock-official-evaluator.sh
```

Artifacts:

- `runs/20260703-senior-swe-bench-local-evaluator-evidence/local-evaluator/firezone-fix-connlib-align-device-hard-local-evaluation.json`
- `runs/20260703-senior-swe-bench-local-evaluator-evidence/local-evaluator/fitness/senior-swe-bench-firezone-fix-connlib-align-device-hard-cycle-0-fitness-evidence.json`

Evidence inspection:

- `schema_version: a2d.fitness-evidence.v1`
- `actual_tests_evaluated: true`
- `non_regressing: true`
- `fitness: 1.0`
- `failed_cases: []`
- result labels include `all_tests_pass`, `hidden_acceptance`, and `has_no_solution_search`
- `source_revision: 6a840b6`
- `source_diff_hash: 7e82eecf604c76426d6a998138980ccdd8791f85`

Candidate-patch hash binding follow-up evidence:

```bash
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260703-senior-swe-bench-candidate-patch-hash-evidence/local-evaluator/fitness \
  cargo run -q -p a2d -- senior-swe-bench-evaluate \
  --task-package runs/20260703-senior-swe-bench-candidate-patch-hash-evidence/task-package/firezone-fix-connlib-align-device-hard-package.json \
  --candidate-patch runs/20260703-senior-swe-bench-candidate-patch-hash-evidence/local-evaluator/candidate.diff \
  --checkout runs/20260703-senior-swe-bench-candidate-patch-hash-evidence/checkout \
  --output runs/20260703-senior-swe-bench-candidate-patch-hash-evidence/local-evaluator/firezone-fix-connlib-align-device-hard-local-evaluation.json \
  -- $PWD/runs/20260703-senior-swe-bench-candidate-patch-hash-evidence/local-evaluator/mock-official-evaluator.sh
```

Artifacts:

- `runs/20260703-senior-swe-bench-candidate-patch-hash-evidence/local-evaluator/firezone-fix-connlib-align-device-hard-local-evaluation.json`
- `runs/20260703-senior-swe-bench-candidate-patch-hash-evidence/local-evaluator/fitness/senior-swe-bench-firezone-fix-connlib-align-device-hard-cycle-0-fitness-evidence.json`

Evidence inspection:

- `schema_version: a2d.fitness-evidence.v1`
- `actual_tests_evaluated: true`
- `non_regressing: true`
- `fitness: 1.0`
- `failed_cases: []`
- result labels include `all_tests_pass`, `hidden_acceptance`, and `has_no_solution_search`
- `source_diff_hash: cbd48c21654b9afd5ad97cab3711cd082e3dfc1b` (matches `git diff -- crates | git hash-object --stdin`)
- `candidate_patch_hash: 134b5415022cbd286abfd60e064dcf9a817d89a0` (matches `git hash-object -- candidate.diff`)

Candidate-patch binding consumption follow-up evidence:

```bash
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260703-senior-swe-bench-binding-validation-evidence/local-evaluator/fitness \
  cargo run -q -p a2d -- senior-swe-bench-evaluate \
  --task-package runs/20260703-senior-swe-bench-binding-validation-evidence/task-package/firezone-fix-connlib-align-device-hard-package.json \
  --candidate-patch runs/20260703-senior-swe-bench-binding-validation-evidence/local-evaluator/candidate.diff \
  --checkout runs/20260703-senior-swe-bench-binding-validation-evidence/checkout \
  --output runs/20260703-senior-swe-bench-binding-validation-evidence/local-evaluator/firezone-fix-connlib-align-device-hard-local-evaluation.json \
  -- $PWD/runs/20260703-senior-swe-bench-binding-validation-evidence/local-evaluator/mock-official-evaluator.sh
```

Artifacts:

- `runs/20260703-senior-swe-bench-binding-validation-evidence/local-evaluator/firezone-fix-connlib-align-device-hard-local-evaluation.json`
- `runs/20260703-senior-swe-bench-binding-validation-evidence/local-evaluator/fitness/senior-swe-bench-firezone-fix-connlib-align-device-hard-cycle-0-fitness-evidence.json`

Evidence inspection:

- `schema_version: a2d.fitness-evidence.v1`
- `actual_tests_evaluated: true`
- `non_regressing: true`
- `fitness: 1.0`
- `failed_cases: []`
- result labels include `all_tests_pass`, `hidden_acceptance`, and `has_no_solution_search`
- `source_diff_hash: 4b5efdd058f6e934736699ee9bb1a3947277086a` (matches `git diff -- crates | git hash-object --stdin`)
- `candidate_patch_hash: 134b5415022cbd286abfd60e064dcf9a817d89a0` (matches `git hash-object -- candidate.diff`)

Actual-test source-patch gate for evaluator-kind validation:

```bash
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260703-senior-swe-bench-evaluator-kind-evidence/actual-test-score-artifact \
  cargo run -q -p a2d -- score-artifact sudoku runs/20260703-senior-swe-bench-evaluator-kind-evidence/good-sudoku-artifact.rs
```

Artifact:

- `runs/20260703-senior-swe-bench-evaluator-kind-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`

Evidence inspection:

- `schema_version: a2d.fitness-evidence.v1`
- `actual_tests_evaluated: true`
- `non_regressing: true`
- `fitness: 1.0`
- `failed_cases: []`
- result labels include `all_tests_pass`
- `source_diff_hash: 8558575b0bd1b99f3197c1a9c91c07639f55836c` (matches `git diff -- crates | git hash-object --stdin`)

This Sudoku hidden-holdout replay is the fresh actual-test source-patch gate for the evaluator-kind validation change. The local-wrapper smoke below proves the new Senior SWE-Bench-specific `evaluator_kind` provenance path, but remains local-wrapper evidence rather than official benchmark mastery.

Evaluator-kind provenance follow-up evidence:

```bash
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260703-senior-swe-bench-evaluator-kind-evidence/local-evaluator/fitness \
  cargo run -q -p a2d -- senior-swe-bench-evaluate \
  --task-package runs/20260703-senior-swe-bench-evaluator-kind-evidence/task-package/firezone-fix-connlib-align-device-hard-package.json \
  --candidate-patch runs/20260703-senior-swe-bench-evaluator-kind-evidence/local-evaluator/candidate.diff \
  --checkout runs/20260703-senior-swe-bench-evaluator-kind-evidence/checkout \
  --output runs/20260703-senior-swe-bench-evaluator-kind-evidence/local-evaluator/firezone-fix-connlib-align-device-hard-local-evaluation.json \
  -- $PWD/runs/20260703-senior-swe-bench-evaluator-kind-evidence/local-evaluator/mock-official-evaluator.sh
```

Artifacts:

- `runs/20260703-senior-swe-bench-evaluator-kind-evidence/local-evaluator/firezone-fix-connlib-align-device-hard-local-evaluation.json`
- `runs/20260703-senior-swe-bench-evaluator-kind-evidence/local-evaluator/fitness/senior-swe-bench-firezone-fix-connlib-align-device-hard-cycle-0-fitness-evidence.json`

Evidence inspection:

- `schema_version: a2d.fitness-evidence.v1`
- `actual_tests_evaluated: true`
- `non_regressing: true`
- `fitness: 1.0`
- `failed_cases: []`
- result labels include `all_tests_pass`, `hidden_acceptance`, and `has_no_solution_search`
- `source_diff_hash: 8558575b0bd1b99f3197c1a9c91c07639f55836c` (matches `git diff -- crates | git hash-object --stdin`)
- `candidate_patch_hash: 134b5415022cbd286abfd60e064dcf9a817d89a0` (matches `git hash-object -- candidate.diff`)
- `evaluator_kind: provided_local_command`

The new evaluator-kind field is optional for historical/generic `a2d.fitness-evidence.v1` artifacts, but required by the Senior SWE-Bench post-export candidate-patch binding verifier before it reports a newly emitted evidence path. This keeps local-wrapper smoke evidence distinct from future official Senior SWE-Bench evaluator evidence.

The evidence source scope is `crates`; documentation updates in this run record the evidence and are not part of the source diff hash.

Negative smokes:

- `runs/20260703-senior-swe-bench-local-evaluator-evidence/bad-package/` proves packages allowing GitHub solution search are refused before evaluator execution.
- `runs/20260703-senior-swe-bench-local-evaluator-evidence/failed-evaluator/` proves failed evaluator outcomes emit local evaluation JSON with no `fitness_evidence_path` and no non-regressing evidence export.

## Status

This is still a local evaluator wrapper smoke, not proof of official Senior SWE-Bench task mastery. The remaining gap is to point this wrapper at a real benchmark-provided official evaluator/hidden-holdout command and then wire a challenge/cycle path that uses it without exposing hidden tests or public solution search.

## Isolated candidate-patch application follow-up

The evaluator wrapper now supports explicit opt-in patch application:

```bash
A2D_SENIOR_SWE_BENCH_PATCHED_CHECKOUT_DIR=runs/20260703-senior-swe-bench-apply-patch-evidence/patched-temp \
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260703-senior-swe-bench-apply-patch-evidence/local-evaluator/fitness \
  cargo run -q -p a2d -- senior-swe-bench-evaluate \
  --task-cycle-input runs/20260703-senior-swe-bench-apply-patch-evidence/task-cycle-input/firezone-fix-connlib-align-device-hard-cycle-input.json \
  --candidate-patch runs/20260703-senior-swe-bench-apply-patch-evidence/local-evaluator/candidate.diff \
  --checkout runs/20260703-senior-swe-bench-apply-patch-evidence/checkout \
  --apply-candidate-patch \
  --output runs/20260703-senior-swe-bench-apply-patch-evidence/local-evaluator/firezone-fix-connlib-align-device-hard-apply-patch-local-evaluation.json \
  -- "$PWD/runs/20260703-senior-swe-bench-apply-patch-evidence/local-evaluator/mock-official-evaluator.sh"
```

Artifacts:

- `runs/20260703-senior-swe-bench-apply-patch-evidence/local-evaluator/firezone-fix-connlib-align-device-hard-apply-patch-local-evaluation.json`
- `runs/20260703-senior-swe-bench-apply-patch-evidence/local-evaluator/fitness/senior-swe-bench-firezone-fix-connlib-align-device-hard-cycle-0-fitness-evidence.json`

Evidence inspection:

- `schema_version: a2d.fitness-evidence.v1`
- `actual_tests_evaluated: true`
- `non_regressing: true`
- `fitness: 1.0`
- `failed_cases: []`
- result labels: `all_tests_pass`, `has_no_solution_search` (policy-declared), `hidden_acceptance`
- `source_diff_hash: 77239a6992caaf3f39525d36242febec8c6dab73`
- `candidate_patch_hash: 91d0dab9c9091c6f3a7634f547601ff36285b218`
- `candidate_patch_applied: true`
- `evaluator_checkout_mode: isolated_copy`
- `evaluator_checkout` points at `runs/20260703-senior-swe-bench-apply-patch-evidence/patched-temp/...` and is distinct from `checkout`
- `original_checkout_mutated: false`
- `evaluator_kind: provided_local_command`
- full validation: `cargo test` (284 passed, 2 ignored)

Smoke inspection also confirmed the evaluator saw the patched file contents, the original checkout remained unmodified, the recorded candidate patch hash matched `git hash-object`, both the fitness evidence and local evaluation `source_diff_hash` matched `git diff -- crates | git hash-object --stdin`, the recorded evaluator checkout was distinct from the original checkout, and the patched temp checkout child was removed after evaluation. Focused regression tests cover rejected temp roots inside the original checkout, symlink/new-file mutation detection, and symlink escape mutation detection. The flag is opt-in so benchmark harnesses that expect an unmodified checkout plus patch path can continue using the default behavior.

## Candidate-patch applicability preflight follow-up

The evaluator wrapper now runs `git apply --check --whitespace=nowarn -- <candidate-patch>` against the supplied checkout before evaluator preparation. This preflight is intentionally separate from `--apply-candidate-patch`: the check proves the candidate diff is applicable to the benchmark checkout without mutating it, while the opt-in apply path still applies the diff in an isolated copy before running the evaluator. Passing local evaluation JSON and exported `a2d.fitness-evidence.v1` now record:

- `candidate_patch_preflight_checked: true`
- `candidate_patch_preflight_status: passed`
- `candidate_patch_preflight_command: git apply --check --whitespace=nowarn -- ...`

Smoke command:

```bash
A2D_SENIOR_SWE_BENCH_PATCHED_CHECKOUT_DIR=runs/20260703-senior-swe-bench-patch-preflight-evidence/patched-temp \
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260703-senior-swe-bench-patch-preflight-evidence/local-evaluator/fitness \
  cargo run -q -p a2d -- senior-swe-bench-evaluate \
  --task-cycle-input runs/20260703-senior-swe-bench-patch-preflight-evidence/task-cycle-input/firezone-fix-connlib-align-device-hard-cycle-input.json \
  --candidate-patch runs/20260703-senior-swe-bench-patch-preflight-evidence/local-evaluator/candidate.diff \
  --checkout runs/20260703-senior-swe-bench-patch-preflight-evidence/checkout \
  --apply-candidate-patch \
  --output runs/20260703-senior-swe-bench-patch-preflight-evidence/local-evaluator/firezone-fix-connlib-align-device-hard-patch-preflight-local-evaluation.json \
  -- "$PWD/runs/20260703-senior-swe-bench-patch-preflight-evidence/local-evaluator/mock-official-evaluator.sh"
```

Artifacts:

- `runs/20260703-senior-swe-bench-patch-preflight-evidence/local-evaluator/firezone-fix-connlib-align-device-hard-patch-preflight-local-evaluation.json`
- `runs/20260703-senior-swe-bench-patch-preflight-evidence/local-evaluator/fitness/senior-swe-bench-firezone-fix-connlib-align-device-hard-cycle-0-fitness-evidence.json`
- source-patch gate support: `runs/20260703-senior-swe-bench-patch-preflight-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`

Evidence inspection:

- `schema_version: a2d.fitness-evidence.v1`
- `actual_tests_evaluated: true`
- `non_regressing: true`
- `fitness: 1.0`
- `failed_cases: []`
- local-wrapper result labels: `all_tests_pass`, `has_no_solution_search` (policy-declared), `hidden_acceptance`
- source-patch gate support result labels include `all_tests_pass`
- `source_diff_hash: 7f38f1a1abacf394ea92a0a90bd4c80022400a4d` in both fresh evidence artifacts, matching `git diff -- crates | git hash-object --stdin`
- `candidate_patch_hash: 91d0dab9c9091c6f3a7634f547601ff36285b218`
- `candidate_patch_applied: true`, `evaluator_checkout_mode: isolated_copy`, `original_checkout_mutated: false`
- `candidate_patch_preflight_checked: true`, `candidate_patch_preflight_status: passed`, and a `candidate_patch_preflight_command` containing `git apply --check`
- full validation: `cargo test` (289 passed, 2 ignored)

This strengthens local-wrapper evidence binding and prevents evaluator/evidence claims for malformed or non-applicable candidate patch bytes. It still does not prove official Senior SWE-Bench mastery unless the provided evaluator command is the benchmark-provided official evaluator/holdouts.

## Official evaluator manifest gate follow-up

The evaluator wrapper now requires an explicit `--official-evaluator-manifest <json>` before it can emit `evaluator_kind: official_senior_swe_bench`. The manifest schema is `a2d.senior-swe-bench-official-evaluator-manifest.v1` and must declare a Senior SWE-Bench URL, matching task/repo, `hidden_holdouts: true`, `github_solution_search_allowed: false`, and a `benchmark_provided_command` exactly equal to the invoked evaluator command. Without the manifest, evidence remains `provided_local_command`.

The exported `a2d.fitness-evidence.v1` official path is unit-covered to serialize and validate:

- `official_evaluator_manifest_path`
- `official_evaluator_manifest_hash`
- `official_benchmark_url`
- `official_task_id`
- `official_repo`
- `official_hidden_holdouts: true`
- `official_github_solution_search_allowed: false`
- `official_benchmark_provided_command`

Validation rejects missing official manifest provenance on `official_senior_swe_bench` evidence and rejects `official_*` fields on `provided_local_command` evidence. No mock local evaluator evidence is relabeled as official in this run.

Fresh source-patch gate support:

```bash
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260703-senior-swe-bench-official-manifest-evidence/actual-test-score-artifact \
  cargo run -q -p a2d -- score-artifact sudoku runs/20260701-score-artifact-fitness-evidence/good-sudoku-artifact.rs
```

Evidence:

- `runs/20260703-senior-swe-bench-official-manifest-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`
- `schema_version: a2d.fitness-evidence.v1`
- `actual_tests_evaluated: true`
- `non_regressing: true`
- `fitness: 1.0`
- `failed_cases: []`
- result labels include `all_tests_pass`
- `source_diff_hash: 46f40e5f02427ac48092f17a9bb9d5e1e573c344`, matching `git diff HEAD -- crates | git hash-object --stdin`
- full validation: `cargo test` (291 passed, 2 ignored)

This is an official-claim gating slice, not official Senior SWE-Bench mastery. A future run must use a real benchmark-provided manifest and evaluator/holdouts before claiming `official_senior_swe_bench` fitness.
