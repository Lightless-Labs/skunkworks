# Plan: Stage 2 — Verified Self-Improvement

**Created:** 2026-04-01
**Status:** In progress
**Enhanced:** 2026-06-29 — structured `a2d.fitness-evidence.v1` artifacts now gate durability; live export/inspection path added for challenge runs
**Enhanced:** 2026-06-30 — comparison modes export labeled canonical fitness evidence; provenance tightened to reject provider-produced evidence
**Enhanced:** 2026-07-01 — exported evidence now records and validates source revision/diff provenance for the `crates` source scope
**Enhanced:** 2026-07-03 — Senior SWE-Bench catalog/evaluator/cycle-input integration is CLI-only external benchmark audit plumbing, not `a2d-core` challenge physics; coder prompt now carries generic benchmark no-solution-search integrity rule; local evaluator evidence binds and verifies the exact candidate patch hash and evaluator provenance
**Enhanced:** 2026-07-04 — built-in domain challenge catalogs moved out of `a2d-core` into the CLI/evaluation layer
**Hardened:** 2026-07-04 — seed coder prompt now names the public `has_tests` fitness gate and exact Rust test module shape after repeated Sudoku evidence showed a generated-solution test-module omission
**Enhanced:** 2026-07-04 — added a first-class `a2d fitness-evidence-inspect` CLI path for source-bound evidence review before persistence decisions
**Enhanced:** 2026-07-04 — added explicit `a2d cycle-input` file/stdin bridge for JSON artifact bundles while rejecting reserved runtime evidence artifacts
**Depends on:** Stage 1 (complete)

## Problem

Stage 1 mutations are structurally valid (RAF closure maintained) but
semantically unverified. The evolver can produce enzyme definitions that
parse as JSON and preserve graph topology while being functionally useless.
"Mutation accepted" currently means "didn't break the plumbing" — not
"made the system better."

## Goal

Every accepted mutation must be accompanied by a mechanical fitness delta.
The system should demonstrably improve at its task across generations,
measured by something other than its own self-report.

## Approach: Holdout Scenarios (StrongDM Pattern)

The evolver and coder cannot see the test scenarios. The tester runs them
and reports pass/fail counts only. This is Foundry's adversarial pattern
applied to the metabolic cycle.

### Components

1. **Holdout benchmark suite** — a set of coding tasks with known-correct
   solutions, stored outside the germline where enzymes can't see them.
   The tester runs the coder's output against these.

2. **Fitness signal** — `pass_count / total_count` on the holdout suite.
   Mechanical. Binary per test, ratio overall. No LLM judgment.

3. **Mutation gate upgrade** — germline accepts a mutation only if:
   - RAF closure is maintained (existing gate)
   - Fitness on holdout suite >= previous generation's fitness (new gate)

4. **Regression detection** — if fitness drops, mutation is rejected even
   if RAF closure holds. Performance monotonicity (Constitution Invariant,
   currently implicit).

### Information Barriers

| Entity | Sees | Never sees |
|--------|------|------------|
| Coder | Requirements, enzyme_defs | Holdout test cases |
| Tester | Code, holdout suite | Enzyme_defs internals |
| Evolver | Test results (pass/fail counts) | Holdout test code, coder's code |

## Implementation Order

1. Define holdout benchmark format (input/expected_output pairs)
2. Create initial benchmark suite (5-10 coding tasks with solutions)
3. Add fitness measurement to the tester enzyme
4. Add fitness-gated mutation acceptance to the germline
5. Wire into metabolism cycle reporting
6. Run multi-generation evolution and measure fitness trajectory

## Success Criteria

- Fitness measurably increases across 5+ generations
- At least one mutation is rejected due to fitness regression (gate works)
- The system cannot game the holdout suite (information barrier holds)

## 2026-06-29 Update: Auditable Fitness Evidence

Implemented structured `a2d.fitness-evidence.v1` artifacts and durability checks so mutation/provider-policy/patch persistence requires non-regressing actual-test evidence, not just RAF closure or internal `cargo test` success.

Added an opt-in challenge-run export path:

```bash
A2D_FITNESS_EVIDENCE_EXPORT_DIR=<dir> cargo run -p a2d -- challenge <challenge> <cycles>
```

When export is requested, the CLI fails closed if a cycle produces no actual-test fitness evidence or if the evidence is stale, regressing, incomplete, contains unreviewed fields, or leaks non-public hidden-holdout case names. Live evidence artifact: `runs/20260629-fitness-evidence/sudoku-solver-cycle-0-fitness-evidence.json` from a seed `sudoku 1` run. It reached 67% (4/6) and exposed `all_tests_pass: false`, so it validates the evidence path and hidden-holdout status reporting, not benchmark mastery.

## 2026-06-30 Update: Comparison Evidence Export

The same export path now covers non-persistent comparison modes:

```bash
A2D_FITNESS_EVIDENCE_EXPORT_DIR=<dir> cargo run -p a2d -- compare-topologies <challenge> <cycles>
A2D_FITNESS_EVIDENCE_EXPORT_DIR=<dir> cargo run -p a2d -- compare-provider-policy <challenge> <cycles>
```

Exports are label-prefixed (`seed-`, `evolved-`, `current-`, `proposed-`) but otherwise use the canonical `fitness_report` artifact created by the benchmark path. Provenance was tightened so current artifact-store evidence is exportable only when the `CycleReport` has current benchmark fitness, while prior-cycle evidence is accepted only from lineage inputs consumed by a later cycle. Provider-produced `fitness_report` outputs are rejected for both export and durability gating.

Live topology evidence: `runs/20260630-topology-fitness-evidence/{seed,evolved}-sudoku-solver-cycle-0-fitness-evidence.json`, both `a2d.fitness-evidence.v1`, `actual_tests_evaluated: true`, `non_regressing: true`, `all_tests_pass: true`, fitness 100% (6/6), SHA-256 `6aa4f715aaa5dd155371519737ff569c3deb0233a01a18cc263e9ec0e2c62abe`. This validates comparison export plumbing with full-passing Sudoku evidence, not repeated benchmark mastery. The provider-policy smoke had no assignment delta, so it is not evidence for a durable policy change.

## 2026-07-01 Update: Source-Bound Evidence Provenance

Exported `a2d.fitness-evidence.v1` now includes source provenance fields for the source scope under test: `source_revision`, `source_tree_dirty`, `source_diff_scope`, `source_diff_hash`, and `evidence_command`. Export-time validation rejects missing provenance, forged diff hashes, revision mismatches, dirty-status mismatches, untracked files under `crates`, and stale current source diffs before writing evidence. The diff hash is computed from a repo-root pathspec for `a2d-autopoietic-autocatalysis-deutero/crates`, so invoking export from a crate subdirectory cannot silently hash an empty scope.

Fresh source-patch gating smoke: `runs/20260701-fitness-evidence-provenance/challenge-smoke/sudoku-solver-cycle-0-fitness-evidence.json`, `a2d.fitness-evidence.v1`, `actual_tests_evaluated: true`, `non_regressing: true`, `all_tests_pass: true`, fitness 100% (6/6), `source_revision: ecdc3dc`, `source_diff_hash: db406660a8259a29169a6d72be4af2c62418703c`. Saved-artifact replay support evidence `runs/20260701-fitness-evidence-provenance/baseline-good/baseline-sudoku-solver-cycle-0-fitness-evidence.json` validates the same provenance/export plumbing with full-passing evidence and the same source diff hash, but is support evidence rather than the source-patch gate. This slice validates provenance/export plumbing, not a durable provider-policy or repeated benchmark-reliability improvement.

## 2026-07-04 Update: Cycle Input Bridge

Added `a2d cycle-input <artifact-bundle.json|-> [cycles]` as an explicit file/stdin entry for JSON artifact bundles such as Senior SWE-Bench `task-cycle-input` outputs. The command validates that input is a JSON object, rejects reserved runtime/mechanical artifacts (`fitness_report`, `failure_report`, `provider_health_report`, `provider_policy`, `system_code`) before provider invocation, and then reuses the existing artifact seeding path for task-context artifacts (`requirements`, `design`, `plan`, `benchmark_context`, `evaluation`). This keeps Senior SWE-Bench orchestration in the CLI/evaluation layer and prevents task input from impersonating benchmark/runtime evidence.

Focused validation: `cargo fmt --check`; `cargo test -p a2d cycle_input -- --nocapture`; `cargo test -p a2d --test cycle_input -- --nocapture`. Full `cargo test` passed (300 passed, 2 ignored). Binary smokes under `runs/20260704-cycle-input-bridge-evidence/negative-smoke/` prove non-JSON stdin and reserved `fitness_report` inputs fail before printing `A²D Catalytic Cycle`. Fresh source-patch gate: `runs/20260704-cycle-input-bridge-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, full-passing `a2d.fitness-evidence.v1` actual-test evidence with `source_diff_hash: 918971a1f1be72793a3fb2f6c68dfa9e300c0825`, matching `git diff --binary HEAD -- crates | git hash-object --stdin`. This is cycle-input bridge evidence, not official Senior SWE-Bench task mastery.

## 2026-07-04 Update: First-Class Fitness Evidence Inspection

Added `a2d fitness-evidence-inspect <evidence.json> [--require-all-tests-pass]` so source-bound `a2d.fitness-evidence.v1` records can be reviewed through the same exported-evidence validator used by persistence gates instead of ad hoc JSON checks. The command requires current source provenance, `actual_tests_evaluated: true`, and `non_regressing: true`; with `--require-all-tests-pass`, it also requires `all_tests_pass: true`, zero failed cases by totals, and no failed result entries. Hidden holdout details remain redacted; absent `hidden_acceptance` is printed as `not_present`, not as a pass.

Fresh source-patch gate: `runs/20260704-fitness-evidence-inspect-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, full-passing `a2d.fitness-evidence.v1` actual-test evidence with `source_diff_hash: 5370681c12650e4236e4fb1bcc2cc4600ebb4794`, matching `git diff --binary HEAD -- crates | git hash-object --stdin`. Full `cargo test` passed (296 passed, 2 ignored). This is evidence for the inspection CLI/source patch, not repeated benchmark mastery.

## 2026-07-02 Update: Directed Routine Evolver Mutations

Routine benchmark-driven evolver `enzyme_defs` mutations are now scoped to existing-enzyme `prompt_template` changes once `fitness_report` exists. Adding enzymes or changing reactants/products/catalysts is rejected in that routine feedback path as structural architecture work, while non-routine structural add/replace paths remain available. The evolver prompt also extracts structured `failed_cases` and per-case pass/fail status from `a2d.fitness-evidence.v1`, so prompt updates can target public/aggregate failure labels without inferring hidden holdout specifics.

Fresh source-patch evidence: `runs/20260702-directed-evolver-fitness-evidence/challenge-sudoku2/sudoku-solver-cycle-0-fitness-evidence.json` and `runs/20260702-directed-evolver-fitness-evidence/challenge-sudoku2/sudoku-solver-cycle-0-consumed-by-cycle-1-fitness-evidence.json`, both `a2d.fitness-evidence.v1`, `actual_tests_evaluated: true`, `non_regressing: true`, fitness 83% (5/6), `all_tests_pass: false`, `source_revision: f155d39`, `source_diff_hash: 7cc9a1c73e7f78fe74953c0d1e986b60ede18ea3`. This is non-regressing source-patch evidence for the directed-evolver gate, not a benchmark-mastery or repeated-reliability claim.

## 2026-07-03 Update: Senior SWE-Bench External Audit Adapter

Senior SWE-Bench bootstrap is deliberately outside `a2d-core`: the public task-list parser, catalog audit, and no-GitHub-solution-search task-context renderer live in the private CLI module `crates/a2d-cli/src/senior_swe_bench.rs` behind `a2d senior-swe-bench-audit`. `a2d-core` has no `senior_swe_bench` exports or references, preserving the boundary between generic A²D evidence/gating primitives and external benchmark adapters.

The audit command parses the public Next.js/RSC task listing from `https://senior-swe-bench.snorkel.ai/tasks`, emits `a2d.senior-swe-bench-audit.v1`, and can render a task prompt preamble that forbids coding agents from searching GitHub/issues/PRs/commits/forks for benchmark solutions. Live audit artifact: `runs/20260703-senior-swe-bench-audit-evidence/audit/senior-swe-bench-audit.json` (50 benchmark tasks, 12 repos, `github_solution_search_allowed: false`).

Fresh source-patch evidence: `runs/20260703-senior-swe-bench-audit-evidence/challenge-smoke/sudoku-solver-cycle-0-fitness-evidence.json`, `a2d.fitness-evidence.v1`, `actual_tests_evaluated: true`, `non_regressing: true`, fitness 67% (4/6), failed cases `all_tests_pass` and `has_tests`, `source_revision: 05aaf3f`, `source_diff_hash: f2e2282d52631f75747a3ae69ba7b46bf75b8720`. Saved-artifact support evidence `runs/20260703-senior-swe-bench-audit-evidence/baseline-good/baseline-sudoku-solver-cycle-0-fitness-evidence.json` is full-passing with the same source diff hash. This is non-regressing source-patch/catalog-adapter evidence, not evidence that A²D solves Senior SWE-Bench tasks yet.

## 2026-07-03 Update: Senior SWE-Bench Task Package Boundary

The audit adapter now has a task-package mode: `a2d senior-swe-bench-audit <html|-> task-package <task-id>`. It emits `a2d.senior-swe-bench-task-package.v1` JSON with the selected hard/guided variant, catalog provenance (`in_benchmark`, `in_sample`, tags), no-GitHub solution-search restrictions, a rendered coding-agent context, and structured evaluation state: `status: not_evaluated`, `evaluator: official_senior_swe_bench`, `fitness: null`. This makes the next evaluator integration step explicit without pretending a catalog package is task fitness.

The architecture boundary is now covered mechanically by `a2d_core_does_not_contain_senior_swe_bench_adapter_code`, which scans `crates/a2d-core/src` for `senior_swe_bench` / `Senior SWE-Bench` adapter text. Explicit grep validation (`rg -n "senior_swe_bench|SeniorSweBench|Senior SWE-Bench|senior-swe-bench" crates/a2d-core crates/a2d-cli Cargo.toml`) showed all Senior SWE-Bench identifiers remain in `crates/a2d-cli`; `a2d-core` stays benchmark-generic.

Fresh source-patch evidence: `runs/20260703-senior-swe-bench-task-package-evidence/challenge-smoke/sudoku-solver-cycle-0-fitness-evidence.json`, `a2d.fitness-evidence.v1`, `actual_tests_evaluated: true`, `non_regressing: true`, fitness 83% (5/6), failed case `all_tests_pass`, `source_revision: 65d25fc`, `source_diff_hash: 51d0e6f5fd1a74c05d827f69a55900d4b3aeea9b`. Saved-artifact support evidence `runs/20260703-senior-swe-bench-task-package-evidence/baseline-good/baseline-sudoku-solver-cycle-0-fitness-evidence.json` is full-passing with the same source diff hash. Package smoke artifact: `runs/20260703-senior-swe-bench-task-package-evidence/task-package/firezone-fix-connlib-align-device-hard-package.json`. This remains `not_evaluated`; the next evidence-backed gap is an official Senior SWE-Bench evaluator/hidden-holdout runner over benchmark-provided checkouts that enforces the no-public-solution-search policy.

## 2026-07-03 Update: Senior SWE-Bench Local Evaluator Wrapper

The CLI now has `a2d senior-swe-bench-evaluate --task-package <json> --candidate-patch <diff> --checkout <dir> [--output <json>] -- <local-evaluator> [args...]`. It stays outside `a2d-core`: the wrapper reads the task package, refuses packages where `github_solution_search_allowed` is true, runs a caller-provided local evaluator command in the supplied checkout, passes task metadata via environment variables, captures stdout/stderr to files to avoid pipe deadlock, emits `a2d.senior-swe-bench-local-evaluation.v1`, and exports `a2d.fitness-evidence.v1` only for full-passing evaluator outcomes. Failed evaluator outcomes still emit evaluation JSON and exit nonzero, but they do not produce non-regressing evidence.

Fresh source-patch evidence: `runs/20260703-senior-swe-bench-local-evaluator-evidence/local-evaluator/fitness/senior-swe-bench-firezone-fix-connlib-align-device-hard-cycle-0-fitness-evidence.json`, `a2d.fitness-evidence.v1`, `actual_tests_evaluated: true`, `non_regressing: true`, `fitness: 1.0`, `failed_cases: []`, result labels `all_tests_pass`, `hidden_acceptance`, and `has_no_solution_search`, `source_revision: 6a840b6`, `source_diff_hash: 7e82eecf604c76426d6a998138980ccdd8791f85`. Local evaluation artifact: `runs/20260703-senior-swe-bench-local-evaluator-evidence/local-evaluator/firezone-fix-connlib-align-device-hard-local-evaluation.json`. Negative smokes are tracked under `runs/20260703-senior-swe-bench-local-evaluator-evidence/bad-package/` and `failed-evaluator/`. Full `cargo test` passed after implementation (273 passed, 2 ignored). This validates the local wrapper and evidence gate, not official benchmark mastery; the remaining gap is wiring a real benchmark-provided official evaluator/holdout command and then a challenge/cycle path.

## 2026-07-03 Update: Senior SWE-Bench Candidate Patch Hash Binding

The local evaluator path now binds evaluator/evidence claims to the exact candidate patch bytes. `a2d senior-swe-bench-evaluate` computes `git hash-object -- <candidate-patch>`, records the resulting `candidate_patch_hash` in `a2d.senior-swe-bench-local-evaluation.v1`, and includes the same optional field in exported `a2d.fitness-evidence.v1`. Export validation rejects malformed or non-string candidate patch hashes, while retaining the existing source-diff provenance checks and keeping all Senior SWE-Bench-specific code in `crates/a2d-cli`.

Fresh source-patch evidence: `runs/20260703-senior-swe-bench-candidate-patch-hash-evidence/local-evaluator/fitness/senior-swe-bench-firezone-fix-connlib-align-device-hard-cycle-0-fitness-evidence.json`, `a2d.fitness-evidence.v1`, `actual_tests_evaluated: true`, `non_regressing: true`, `fitness: 1.0`, `failed_cases: []`, result labels `all_tests_pass`, `hidden_acceptance`, and `has_no_solution_search`, `source_diff_hash: cbd48c21654b9afd5ad97cab3711cd082e3dfc1b`, `candidate_patch_hash: 134b5415022cbd286abfd60e064dcf9a817d89a0`. The local evaluation artifact `runs/20260703-senior-swe-bench-candidate-patch-hash-evidence/local-evaluator/firezone-fix-connlib-align-device-hard-local-evaluation.json` carries the same candidate patch hash. Full `cargo test` passed after implementation (274 passed, 2 ignored). This remains local-wrapper evidence, not official Senior SWE-Bench mastery.

## 2026-07-03 Update: Senior SWE-Bench Candidate Patch Binding Consumption

The local evaluator now verifies the binding after export, not only while constructing evidence. After writing `a2d.fitness-evidence.v1`, `a2d senior-swe-bench-evaluate` re-reads the evidence file, runs the exported-evidence schema/provenance validator, requires `candidate_patch_hash`, recomputes `git hash-object` for the current candidate patch file, and rejects missing or mismatched hashes before printing the evidence path. Focused coverage exercises matching, missing, and mismatched candidate-patch hashes.

Fresh source-patch evidence: `runs/20260703-senior-swe-bench-binding-validation-evidence/local-evaluator/fitness/senior-swe-bench-firezone-fix-connlib-align-device-hard-cycle-0-fitness-evidence.json`, `a2d.fitness-evidence.v1`, `actual_tests_evaluated: true`, `non_regressing: true`, `fitness: 1.0`, `failed_cases: []`, result labels `all_tests_pass`, `hidden_acceptance`, and `has_no_solution_search`, `source_diff_hash: 4b5efdd058f6e934736699ee9bb1a3947277086a`, `candidate_patch_hash: 134b5415022cbd286abfd60e064dcf9a817d89a0`. The local evaluation artifact `runs/20260703-senior-swe-bench-binding-validation-evidence/local-evaluator/firezone-fix-connlib-align-device-hard-local-evaluation.json` carries the same candidate patch hash. Full `cargo test` passed after implementation (274 passed, 2 ignored). This still validates wrapper/evidence plumbing only, not official Senior SWE-Bench mastery.

## 2026-07-03 Update: Senior SWE-Bench Evaluator Kind Provenance

Senior SWE-Bench standalone fitness evidence now distinguishes the evaluator source. `a2d senior-swe-bench-evaluate` adds `evaluator_kind: provided_local_command` to newly exported `a2d.fitness-evidence.v1`; exported-evidence validation accepts only reviewed evaluator kinds (`provided_local_command` or future `official_senior_swe_bench`), while older generic evidence remains backward-compatible because the field is optional outside Senior SWE-Bench candidate-patch binding. The post-export binding verifier now requires `evaluator_kind` alongside `candidate_patch_hash` before reporting a Senior SWE-Bench evidence path, so local wrapper evidence cannot be mistaken for official benchmark mastery.

Fresh actual-test source-patch evidence: `runs/20260703-senior-swe-bench-evaluator-kind-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, `a2d.fitness-evidence.v1`, `actual_tests_evaluated: true`, `non_regressing: true`, `fitness: 1.0`, `failed_cases: []`, result labels include `all_tests_pass`, and `source_diff_hash: 8558575b0bd1b99f3197c1a9c91c07639f55836c`. Fresh local-wrapper provenance evidence: `runs/20260703-senior-swe-bench-evaluator-kind-evidence/local-evaluator/fitness/senior-swe-bench-firezone-fix-connlib-align-device-hard-cycle-0-fitness-evidence.json`, `a2d.fitness-evidence.v1`, `actual_tests_evaluated: true`, `non_regressing: true`, `fitness: 1.0`, `failed_cases: []`, result labels `all_tests_pass`, `hidden_acceptance`, and `has_no_solution_search`, `source_diff_hash: 8558575b0bd1b99f3197c1a9c91c07639f55836c`, `candidate_patch_hash: 134b5415022cbd286abfd60e064dcf9a817d89a0`, `evaluator_kind: provided_local_command`. The local evaluation artifact `runs/20260703-senior-swe-bench-evaluator-kind-evidence/local-evaluator/firezone-fix-connlib-align-device-hard-local-evaluation.json` carries the same candidate patch hash and still records `evaluator: provided_local_command`. Full `cargo test` passed after implementation (274 passed, 2 ignored). This remains local-wrapper evidence plus a Sudoku hidden-holdout replay gate for the source patch, not official Senior SWE-Bench mastery.

## 2026-07-03 Update: Senior SWE-Bench Cycle Input

The audit adapter now exposes `a2d senior-swe-bench-audit <html|-> task-cycle-input <task-id>`, emitting a JSON artifact bundle that can seed the existing A²D cycle inputs without adding Senior SWE-Bench logic to `a2d-core`. The bundle carries `requirements`, `design`, `plan`, `benchmark_context`, and `evaluation`; requirements include the no-GitHub/public-solution-search policy and an explicit unified-diff candidate-patch deliverable, while evaluation stays `status: not_evaluated` / `fitness: null`. The seed coder prompt still defaults to a single Rust source file for normal challenges, but now follows an explicitly requested alternate deliverable such as a unified diff candidate patch.

CLI smoke artifact: `runs/20260703-senior-swe-bench-cycle-input-evidence/task-cycle-input/firezone-fix-connlib-align-device-hard-cycle-input.json`, produced by `a2d senior-swe-bench-audit ... task-cycle-input firezone-fix-connlib-align-device-hard`; inspection confirmed `requirements`, `design`, `plan`, `benchmark_context`, and `evaluation`, with `unified diff candidate patch`, `Do not search GitHub`, and `not_evaluated`. Fresh actual-test source-patch evidence: `runs/20260703-senior-swe-bench-cycle-input-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, `a2d.fitness-evidence.v1`, `actual_tests_evaluated: true`, `non_regressing: true`, `fitness: 1.0`, `failed_cases: []`, result labels include `all_tests_pass`, and `source_diff_hash: 419a182e7af9bd8a6780bbf8fd84e2764ecfa9f1`. Full `cargo test` passed after implementation (276 passed, 2 ignored). This is cycle-input plumbing, not official Senior SWE-Bench mastery.

## 2026-07-03 Update: Senior SWE-Bench Cycle Input Replay

`a2d senior-swe-bench-evaluate` now accepts `--task-cycle-input <json>` as an alternative to `--task-package <json>`, so the `not_evaluated` cycle-input bundle can feed the existing gated evaluator wrapper directly. The new parser reuses the CLI-only Senior SWE-Bench boundary, converts valid cycle input into the existing task summary, and rejects unsafe or overclaiming inputs: GitHub/public solution search must be forbidden, `evaluation.status` must still be `not_evaluated`, and `evaluation.fitness` must be `null`. CLI parsing rejects missing task input and mutually exclusive `--task-package` plus `--task-cycle-input`.

Fresh actual-test local-replay evidence: `runs/20260703-senior-swe-bench-cycle-input-replay-evidence/local-evaluator/fitness/senior-swe-bench-firezone-fix-connlib-align-device-hard-cycle-0-fitness-evidence.json`, `a2d.fitness-evidence.v1`, `actual_tests_evaluated: true`, `non_regressing: true`, `fitness: 1.0`, `failed_cases: []`, result labels `all_tests_pass`, `has_no_solution_search` (policy-declared from accepted no-search metadata, not a network-forensics proof), and `hidden_acceptance`, `source_diff_hash: 65506a4c371a1751089be88cd0eb98501bb31649`, `candidate_patch_hash: 8ecc93527321bf316172ef06469260421bc701db`, `evaluator_kind: provided_local_command`. Local evaluation artifact: `runs/20260703-senior-swe-bench-cycle-input-replay-evidence/local-evaluator/firezone-fix-connlib-align-device-hard-cycle-input-local-evaluation.json`. Full `cargo test` passed after implementation (277 passed, 2 ignored). Negative cycle-input replay smoke `runs/20260703-senior-swe-bench-cycle-input-replay-evidence/negative-smoke/solution-search-rejection.err` proves a cycle input with `github_solution_search_allowed: true` is rejected before evaluator execution and emits no fitness evidence. This proves cycle-input artifacts can reach the gated evaluator/evidence path, but remains local-wrapper evidence unless the provided command is the official Senior SWE-Bench evaluator/holdouts.

## 2026-07-03 Update: Senior SWE-Bench Isolated Patch Application

`a2d senior-swe-bench-evaluate` now has explicit `--apply-candidate-patch` semantics. The default remains unchanged for benchmark/evaluator contracts that expect an unmodified checkout plus candidate patch path. When the flag is set, the CLI rejects patched temp roots inside the original checkout (including symlinked descendants), copies the supplied checkout to an isolated temp directory, applies the candidate unified diff there with `git apply --whitespace=nowarn`, fingerprints the original checkout before/after evaluator execution, runs the provided evaluator with `current_dir` set to the patched copy, passes original/evaluator checkout paths through environment variables, and removes the temp copy unless explicitly retained for debugging. Passing evidence requires `original_checkout_mutated: false` for the opt-in isolated path.

Evaluation JSON and exported `a2d.fitness-evidence.v1` now record patch-application provenance: `candidate_patch_path`, `candidate_patch_applied`, `evaluator_checkout_mode`, `evaluator_checkout`, and `original_checkout_mutated`; the local evaluation JSON also records `source_revision`, `source_tree_dirty`, `source_diff_scope`, `source_diff_hash`, and `evidence_command`. The Senior SWE-Bench post-export binding verifier now requires path/hash agreement and checks expected patch-application provenance before reporting evidence, so unrelated passing evidence cannot be silently reused for a different candidate patch or application mode.

Fresh actual-test apply-patch evidence: `runs/20260703-senior-swe-bench-apply-patch-evidence/local-evaluator/fitness/senior-swe-bench-firezone-fix-connlib-align-device-hard-cycle-0-fitness-evidence.json`, `a2d.fitness-evidence.v1`, `actual_tests_evaluated: true`, `non_regressing: true`, `fitness: 1.0`, `failed_cases: []`, result labels `all_tests_pass`, `has_no_solution_search` (policy-declared from accepted no-search metadata), and `hidden_acceptance`, `source_diff_hash: 77239a6992caaf3f39525d36242febec8c6dab73`, `candidate_patch_hash: 91d0dab9c9091c6f3a7634f547601ff36285b218`, `candidate_patch_applied: true`, `evaluator_checkout_mode: isolated_copy`, `original_checkout_mutated: false`, `evaluator_checkout` set to the isolated patched temp checkout, `evaluator_kind: provided_local_command`. Local evaluation artifact: `runs/20260703-senior-swe-bench-apply-patch-evidence/local-evaluator/firezone-fix-connlib-align-device-hard-apply-patch-local-evaluation.json`. Full `cargo test` passed after implementation (284 passed, 2 ignored). This proves the wrapper can evaluate applied candidate diffs while checking that the original checkout was not contaminated; it remains local-wrapper evidence unless the provided command is the official Senior SWE-Bench evaluator/holdouts.

## 2026-07-03 Update: Senior SWE-Bench Candidate Patch Preflight

`a2d senior-swe-bench-evaluate` now preflights candidate diffs with `git apply --check --whitespace=nowarn -- <candidate-patch>` against the supplied checkout before evaluator preparation. This is non-mutating and distinct from `--apply-candidate-patch`: preflight proves applicability to the benchmark checkout, while the opt-in apply mode still applies the patch only in an isolated copy. Newly emitted local evaluation JSON and Senior SWE-Bench fitness evidence record `candidate_patch_preflight_checked`, `candidate_patch_preflight_status`, and `candidate_patch_preflight_command`; the post-export Senior SWE-Bench binding verifier requires those fields before reporting evidence.

Fresh local-wrapper evidence: `runs/20260703-senior-swe-bench-patch-preflight-evidence/local-evaluator/fitness/senior-swe-bench-firezone-fix-connlib-align-device-hard-cycle-0-fitness-evidence.json`, `a2d.fitness-evidence.v1`, `actual_tests_evaluated: true`, `non_regressing: true`, `fitness: 1.0`, `failed_cases: []`, result labels `all_tests_pass`, `has_no_solution_search`, and `hidden_acceptance`, `source_diff_hash: 7f38f1a1abacf394ea92a0a90bd4c80022400a4d`, `candidate_patch_hash: 91d0dab9c9091c6f3a7634f547601ff36285b218`, `candidate_patch_applied: true`, `evaluator_checkout_mode: isolated_copy`, `original_checkout_mutated: false`, and preflight status `passed`. Fresh source-patch gate support evidence: `runs/20260703-senior-swe-bench-patch-preflight-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, full-passing with the same `source_diff_hash`. Full `cargo test` passed after implementation (289 passed, 2 ignored). This remains local-wrapper evidence, not official Senior SWE-Bench mastery.

## 2026-07-03 Update: Senior SWE-Bench Official Evaluator Manifest Gate

`a2d senior-swe-bench-evaluate` now has an explicit `--official-evaluator-manifest <json>` gate before any evidence may use `evaluator_kind: official_senior_swe_bench`. The manifest parser is CLI-only and requires schema `a2d.senior-swe-bench-official-evaluator-manifest.v1`, a Senior SWE-Bench URL, matching `task_id`/`repo`, `hidden_holdouts: true`, `github_solution_search_allowed: false`, and an exact match between `benchmark_provided_command` and the invoked evaluator command. Without a valid manifest, evaluator evidence remains `provided_local_command`.

Exported `a2d.fitness-evidence.v1` and local evaluation JSON now carry official manifest provenance only when the gate passes: manifest path/hash, benchmark URL, task/repo, hidden-holdout declaration, no-search declaration, and benchmark-provided command. Export validation rejects `official_senior_swe_bench` without complete manifest provenance and rejects `official_*` fields on non-official evidence to prevent local-wrapper smokes from masquerading as official benchmark mastery. Fresh source-patch gate support evidence: `runs/20260703-senior-swe-bench-official-manifest-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, `a2d.fitness-evidence.v1`, `actual_tests_evaluated: true`, `non_regressing: true`, `fitness: 1.0`, `failed_cases: []`, result label `all_tests_pass`, and `source_diff_hash: 46f40e5f02427ac48092f17a9bb9d5e1e573c344`. Full `cargo test` passed after implementation (291 passed, 2 ignored). This adds the official-evaluator claim gate; it still does not claim official Senior SWE-Bench task mastery until a real benchmark-provided evaluator/holdout command and manifest are used.

## 2026-07-03 Update: Coder Benchmark Integrity Prompt

The seed coder prompt now includes a generic benchmark-integrity rule: if the requirements, design, or plan say not to search GitHub, issues, pull requests, commits, forks, public web pages, or solution writeups for benchmark answers, the coder must obey that restriction and solve from provided context plus local tests only. This supports Senior SWE-Bench task contexts without adding benchmark-specific code to `a2d-core`.

Loaded coder prompt normalization now preserves evolved prompt text when it already contains `design` and `plan`; if such a prompt lacks the integrity rule, A²D appends the rule instead of replacing the whole prompt. Focused coverage includes `normalize_loaded_enzymes_preserves_evolved_coder_prompt_when_adding_integrity_rule`.

Fresh source-patch evidence: `runs/20260703-coder-benchmark-integrity-evidence/challenge-smoke/sudoku-solver-cycle-0-fitness-evidence.json`, `a2d.fitness-evidence.v1`, `actual_tests_evaluated: true`, `non_regressing: true`, fitness 67% (4/6), failed cases `all_tests_pass` and `has_tests`, `source_revision: 2f88a93`, `source_diff_hash: 9916603b8d352a3316b9e1964392693f33fa41ec`. Saved-artifact support evidence `runs/20260703-coder-benchmark-integrity-evidence/baseline-good/baseline-sudoku-solver-cycle-0-fitness-evidence.json` is full-passing with the same source diff hash. This is non-regressing source-patch/prompt-integrity evidence, not proof of Senior SWE-Bench task-solving.

## 2026-07-04 Update: Challenge Catalog Boundary Cleanup

Built-in domain challenge definitions and hidden acceptance-test catalogs (`sudoku`, `chess`, `rubiks`) now live in the CLI/evaluation layer at `crates/a2d-cli/src/challenges.rs` instead of `a2d-core`. `a2d-core` now exposes generic benchmark/sandbox/evidence/gating primitives only; it no longer exports `pub mod challenges`, and regression tests scan core for domain challenge catalog terms. The self-sandbox automated-modifiable allowlist also excludes both the old core challenge path and the new CLI challenge catalog path, preserving hidden-holdout oracle integrity from automated `SystemPatch` edits.

Validation: `cargo fmt --check`; focused `cargo test -p a2d-core challenge_catalogs_are_not_core_modifiable_surface -- --nocapture`; focused `cargo test -p a2d challenges:: -- --nocapture`; full `cargo test` passed (293 passed, 2 ignored). Independent review flagged hidden-holdout mutability in the first draft; the final slice removed `crates/a2d-cli/src/challenges.rs` from the self-sandbox allowlist.

Fresh source-patch evidence: `runs/20260704-challenge-catalog-boundary-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, `a2d.fitness-evidence.v1`, `actual_tests_evaluated: true`, `non_regressing: true`, fitness 100% (6/6), `failed_cases: []`, result labels include `all_tests_pass`, `source_revision: 3e3c34e`, `source_diff_hash: da45da61907809b691d825410e4d0fdf3b0a6f67`. This is source-patch boundary evidence and hidden-holdout replay evidence, not new benchmark mastery.

## 2026-07-04 Update: Senior SWE-Bench Evaluator Policy Environment

The Senior SWE-Bench evaluator wrapper now passes the accepted no-solution-search policy through to evaluator subprocesses as `A2D_SENIOR_SWE_BENCH_GITHUB_SOLUTION_SEARCH_ALLOWED=false` and `A2D_SENIOR_SWE_BENCH_PUBLIC_SOLUTION_SEARCH_FORBIDDEN=true`. The values come from the parsed task package or cycle input, which already fail closed when GitHub solution search is allowed. This preserves the CLI/evaluation boundary while making policy visible to local or official harness wrappers that need to assert the benchmark contract at process runtime.

Focused validation: `cargo fmt --check`; `cargo test -p a2d senior_swe_bench -- --nocapture` (27 passed). Full `cargo test` passed (293 passed, 2 ignored). Boundary search found no Senior SWE-Bench/domain challenge adapter text in `crates/a2d-core`.

Fresh local-wrapper evidence: `runs/20260704-senior-swe-bench-evaluator-policy-evidence/local-evaluator/fitness/senior-swe-bench-firezone-fix-connlib-align-device-hard-cycle-0-fitness-evidence.json`, `a2d.fitness-evidence.v1`, `actual_tests_evaluated: true`, `non_regressing: true`, `fitness: 1.0`, `failed_cases: []`, labels `all_tests_pass`, `has_no_solution_search`, `hidden_acceptance`, `candidate_patch_applied: true`, `evaluator_checkout_mode: isolated_copy`, `original_checkout_mutated: false`, `candidate_patch_preflight_status: passed`, `evaluator_kind: provided_local_command`, and `source_diff_hash: c18edad00627bf325fadb84ff65468289e7fe693`. Fresh source-patch gate support: `runs/20260704-senior-swe-bench-evaluator-policy-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, full-passing with the same `source_diff_hash`. This remains local-wrapper/evaluator-policy evidence, not official Senior SWE-Bench task mastery.

## 2026-07-04 Update: Senior SWE-Bench Candidate Patch Extraction

`a2d senior-swe-bench-extract-patch <artifact|->` now bridges coder/cycle artifacts toward the evaluator wrapper by extracting a unified diff candidate patch from raw or fenced output. It emits only the patch bytes to stdout and rejects prose-only artifacts plus obvious public GitHub solution references (`github.com/`, `/pull/`, `/commit/`). This keeps the extraction/adaptation logic in the CLI/evaluation layer and preserves the `a2d-core` boundary.

Focused validation: `cargo fmt --check`; `cargo test -p a2d senior_swe_bench_candidate_patch_extractor -- --nocapture`; `cargo test -p a2d senior_swe_bench -- --nocapture` (28 passed). Full `cargo test` passed (294 passed, 2 ignored). Boundary search found no Senior SWE-Bench/domain challenge adapter text in `crates/a2d-core`.

Fresh local-wrapper evidence: `runs/20260704-senior-swe-bench-extract-patch-evidence/local-evaluator/fitness/senior-swe-bench-firezone-fix-connlib-align-device-hard-cycle-0-fitness-evidence.json`, `a2d.fitness-evidence.v1`, `actual_tests_evaluated: true`, `non_regressing: true`, `fitness: 1.0`, `failed_cases: []`, labels `all_tests_pass`, `has_no_solution_search`, `hidden_acceptance`, extracted `candidate_patch_hash: e61dbc5efcae0fda0dca699dae5c782d69e9e5e3`, `candidate_patch_applied: true`, `evaluator_checkout_mode: isolated_copy`, `original_checkout_mutated: false`, `candidate_patch_preflight_status: passed`, `evaluator_kind: provided_local_command`, `source_revision: fa71b7b` (the scoped `HEAD:crates` tree id), and `source_diff_hash: a32c3577c381cd056a13aa6de4d7d982fd75454e`. Fresh source-patch gate support: `runs/20260704-senior-swe-bench-extract-patch-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, full-passing with the same source tree/diff provenance. This is local-wrapper extraction/evaluator evidence, not official Senior SWE-Bench mastery.

## 2026-07-04 Update: Senior SWE-Bench Artifact Evaluation Binding

`a2d senior-swe-bench-evaluate` now accepts `--candidate-patch-artifact <artifact>` together with `--extracted-candidate-patch <diff>` as an alternative to direct `--candidate-patch`. The evaluator command reads the exact raw artifact bytes, hashes those same bytes, extracts a unified diff, writes the extracted patch when absent, rejects a pre-existing extracted patch that does not match the raw artifact before any evaluator execution, and records raw-artifact provenance in exported evidence: `candidate_patch_artifact_path` plus `candidate_patch_artifact_hash`. Existing `candidate_patch_path`/`candidate_patch_hash` continue to bind the exact extracted diff bytes consumed by preflight/apply/evaluator. Export validation rejects one-sided artifact provenance, and the post-export Senior SWE-Bench binding verifier checks artifact path/hash when artifact mode is used.

Focused validation: `cargo fmt --check`; `cargo test -p a2d senior_swe_bench_exported_fitness_evidence_binds_candidate_patch_hash -- --nocapture`; `cargo test -p a2d senior_swe_bench -- --nocapture` (29 passed); `cargo test -p a2d exported_fitness_evidence_validation_requires_source_provenance -- --nocapture`. Full `cargo test` passed (295 passed, 2 ignored). Boundary search found no Senior SWE-Bench/domain challenge adapter text in `crates/a2d-core`.

Fresh local-wrapper evidence: `runs/20260704-senior-swe-bench-artifact-evaluate-evidence/local-evaluator/fitness/senior-swe-bench-firezone-fix-connlib-align-device-hard-cycle-0-fitness-evidence.json`, `a2d.fitness-evidence.v1`, `actual_tests_evaluated: true`, `non_regressing: true`, `fitness: 1.0`, `failed_cases: []`, labels `all_tests_pass`, `has_no_solution_search`, `hidden_acceptance`, `candidate_patch_hash: e61dbc5efcae0fda0dca699dae5c782d69e9e5e3`, `candidate_patch_artifact_hash: 5f7013d405095ecf479f626c1ce7a38adf9a5cc4`, `candidate_patch_applied: true`, `evaluator_checkout_mode: isolated_copy`, `original_checkout_mutated: false`, `candidate_patch_preflight_status: passed`, `evaluator_kind: provided_local_command`, `source_revision: be662c6` (the scoped `HEAD:crates` tree id), and `source_diff_hash: 70b17bc9bbc8667c8ee3f0b2dfe94a3310a7991e`. Negative smoke `runs/20260704-senior-swe-bench-artifact-evaluate-evidence/negative-smoke/mismatch.err` proves a mismatched pre-existing extracted diff is rejected before evaluator execution and emits no fitness evidence. Fresh source-patch gate support: `runs/20260704-senior-swe-bench-artifact-evaluate-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, full-passing with the same source tree/diff provenance. This remains local-wrapper artifact-evaluation evidence, not official Senior SWE-Bench task mastery.

## 2026-07-04 Update: Sudoku `has_tests` Prompt Hardening

Repeated seed Sudoku evidence showed one public prompt-compliance failure: `runs/20260630-sudoku-repeat-evidence/r1/sudoku-solver-cycle-0-fitness-evidence.json` compiled and exposed `parse`/`solve`/`validate`, but failed `has_tests`. The seed coder prompt now names the exact required Rust test module header (`#[cfg(test)] mod tests`), states that omitting it fails the mechanical `has_tests` fitness gate, and asks for normal-path, edge/invalid-input, and end-to-end behavior tests. This names a public aggregate/scaffolding gate and does not leak hidden holdouts.

Focused validation: `cargo fmt --check`; `cargo test -p a2d seed_germline_coder_consumes_design_plan_and_requirements -- --nocapture`. Full `cargo test` passed (295 passed, 2 ignored).

Fresh generated challenge evidence: `runs/20260704-sudoku-test-prompt-evidence/challenge-seed-r1/sudoku-solver-cycle-0-fitness-evidence.json`, `a2d.fitness-evidence.v1`, `actual_tests_evaluated: true`, `non_regressing: true`, `fitness: 0.8333333333333334`, `failed_cases: ["all_tests_pass"]`, result label `has_tests: true`, `source_revision: d273717`, and `source_diff_hash: d44224bb8edd9d79608bb2b2646b00867f4cf4f1`. Fresh full-passing source-patch replay gate: `runs/20260704-sudoku-test-prompt-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, `fitness: 1.0`, `failed_cases: []`, and the same source diff hash. This gates the prompt source patch and proves the generated solution satisfied `has_tests` in this run, but it is not a Sudoku-mastery claim because generated challenge `all_tests_pass` still failed.

## 2026-07-04 Update: Cycle Input Output Artifacts

`a2d cycle-input` now accepts `--output-artifacts <dir>` while preserving the existing `a2d cycle-input <artifact-bundle.json|-> [cycles]` positional cycle-count form. When enabled, materialized cycle outputs are written to files plus a cumulative `a2d.cycle-output-artifacts.v1` manifest with cycle/workcell/enzyme/provider/artifact type, byte count, and `git hash-object` content hashes. The export fails closed on destination collisions or pre-existing manifests instead of overwriting artifacts, so provider-produced outputs can be safely passed to downstream extraction/evaluation commands.

Focused validation: `cargo fmt --check`; `cargo test -p a2d cycle_output_artifact -- --nocapture`; `cargo test -p a2d cycle_input -- --nocapture`. Full `cargo test` passed (302 passed, 2 ignored in the final run: 113 a2d CLI tests, 2 cycle-input integration tests, 5 score-artifact integration tests, 160 core tests, 11 bootstrap tests, 10 provider tests, 1 doctest; 2 ignored self-sandbox tests).

Fresh live cycle-input smoke: `runs/20260704-cycle-output-artifacts-evidence/live-cycle/artifacts/manifest.json` captured one provider-produced `code` artifact from `firezone-fix-connlib-align-device-hard` via `opencode/kimi-for-coding/k2p6`, with hash `1c7b18fff2fe71619e93ba3aa8b0ed8146909538`. Downstream extraction intentionally failed closed (`runs/20260704-cycle-output-artifacts-evidence/extract/extract.err`) because the provider output was prose (`I'll inspect...`) rather than a unified diff. This is useful integration evidence: the bridge can now capture the artifact, and the extractor/evaluator path rejects non-patch output before any Senior SWE-Bench fitness claim.

Fresh source-patch gate evidence: `runs/20260704-cycle-output-artifacts-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, full-passing `a2d.fitness-evidence.v1`, `actual_tests_evaluated: true`, `non_regressing: true`, `fitness: 1.0`, `failed_cases: []`, `source_diff_hash: ed02b5802a540a7800696e8c8110277afd471cda`, matching `git diff --binary HEAD -- crates | git hash-object --stdin`. This gates the CLI source patch; it is not official Senior SWE-Bench mastery.

## 2026-07-04 Update: Senior SWE-Bench Artifact Diagnosis

`a2d senior-swe-bench-diagnose-artifact <artifact|->` now emits diagnostic-only `a2d.senior-swe-bench-artifact-diagnosis.v1` JSON for captured provider artifacts before operators add more task/checkout context. The diagnosis distinguishes extractable unified diffs from public GitHub solution references, checkout-deferral prose, and generic output-contract failures. Public GitHub reference detection is case-insensitive, diagnostic previews are redacted when such references are present, and `senior-swe-bench-extract-patch` remains fail-closed.

Focused validation: `cargo fmt --check`; `cargo test -p a2d --test senior_swe_bench_diagnose_artifact -- --nocapture` (4 passed); `cargo test -p a2d senior_swe_bench_candidate_patch_extractor -- --nocapture`. Full `cargo test` passed (306 passed, 2 ignored). Reviewer blockers on mixed-case GitHub references and diagnostic preview leakage were fixed before persistence.

Fresh diagnosis artifact: `runs/20260704-senior-swe-bench-artifact-diagnosis-evidence/diagnosis/prose-artifact-diagnosis.json` classifies the prior live prose output as `checkout_context_not_exercised` with `contains_unified_diff_candidate_patch: false`. Negative smoke `runs/20260704-senior-swe-bench-artifact-diagnosis-evidence/negative-smoke/mixed-case-github.err` proves mixed-case public GitHub references are rejected before extraction. Fresh source-patch gate evidence: `runs/20260704-senior-swe-bench-artifact-diagnosis-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, full-passing `a2d.fitness-evidence.v1`, `actual_tests_evaluated: true`, `non_regressing: true`, `fitness: 1.0`, `failed_cases: []`, `source_diff_hash: 1d526318a6426ba61cc79bda7c6c5c04a5867397`, matching `git diff --binary HEAD -- crates | git hash-object --stdin`. This gates the diagnostic CLI source patch only; diagnosis is not Senior SWE-Bench fitness evidence or official benchmark mastery.

## 2026-07-04 Update: Cycle Input Checkout Context

`a2d cycle-input <artifact-bundle.json|-> [cycles]` now accepts `--checkout <dir>`. Because provider invocations are intentionally artifact-only/no-tools in isolated cwd, the CLI does not hand providers mutable checkout access. Instead it reads a bounded UTF-8 source/context snapshot from the local checkout, injects it into the coder-visible `design` artifact, and reserves a `benchmark_checkout_context` artifact that user bundles cannot spoof.

Safety gates: root checkout symlinks are rejected; file symlinks are skipped and files are canonicalized/revalidated under the checkout root immediately before read; secret-like files/directories are excluded (`.env*`, tokens, credentials, secrets, key/cert containers, `.npmrc`, `.pypirc`); common dependency/build directories are skipped; the provider-facing checkout path is redacted as `<benchmark-checkout>`; and context size/file-count limits bound exposure. Senior SWE-Bench/checkout-specific code remains in `a2d-cli`; `a2d-core` remains benchmark-generic.

Focused validation: `cargo fmt --check`; `cargo test -p a2d cycle_input -- --nocapture`. Full `cargo test` passed (309 passed, 2 ignored). First reviewer blockers on secret leakage and symlink/TOCTOU risk were fixed; follow-up reviewer found no blockers. Negative smoke `runs/20260704-cycle-input-checkout-context-evidence/negative-smoke/empty-checkout.err` proves empty checkout context fails closed before provider invocation. Boundary check `runs/20260704-cycle-input-checkout-context-evidence/boundary/a2d-core-boundary-rg.txt` has no `a2d-core` matches. Fresh source-patch gate evidence: `runs/20260704-cycle-input-checkout-context-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, full-passing `a2d.fitness-evidence.v1`, `actual_tests_evaluated: true`, `non_regressing: true`, `fitness: 1.0`, `failed_cases: []`, `source_diff_hash: c003e6f4143413a5b40973ae7093ea14516bf12f`, matching `git diff --binary HEAD -- crates | git hash-object --stdin`. This is checkout-context plumbing/source-patch evidence only; it is not official Senior SWE-Bench mastery.
