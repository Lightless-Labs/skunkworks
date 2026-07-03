# Plan: Stage 2 — Verified Self-Improvement

**Created:** 2026-04-01
**Status:** In progress
**Enhanced:** 2026-06-29 — structured `a2d.fitness-evidence.v1` artifacts now gate durability; live export/inspection path added for challenge runs
**Enhanced:** 2026-06-30 — comparison modes export labeled canonical fitness evidence; provenance tightened to reject provider-produced evidence
**Enhanced:** 2026-07-01 — exported evidence now records and validates source revision/diff provenance for the `crates` source scope
**Enhanced:** 2026-07-03 — Senior SWE-Bench catalog integration is CLI-only external benchmark audit plumbing, not `a2d-core` challenge physics; coder prompt now carries generic benchmark no-solution-search integrity rule
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

## 2026-07-03 Update: Coder Benchmark Integrity Prompt

The seed coder prompt now includes a generic benchmark-integrity rule: if the requirements, design, or plan say not to search GitHub, issues, pull requests, commits, forks, public web pages, or solution writeups for benchmark answers, the coder must obey that restriction and solve from provided context plus local tests only. This supports Senior SWE-Bench task contexts without adding benchmark-specific code to `a2d-core`.

Loaded coder prompt normalization now preserves evolved prompt text when it already contains `design` and `plan`; if such a prompt lacks the integrity rule, A²D appends the rule instead of replacing the whole prompt. Focused coverage includes `normalize_loaded_enzymes_preserves_evolved_coder_prompt_when_adding_integrity_rule`.

Fresh source-patch evidence: `runs/20260703-coder-benchmark-integrity-evidence/challenge-smoke/sudoku-solver-cycle-0-fitness-evidence.json`, `a2d.fitness-evidence.v1`, `actual_tests_evaluated: true`, `non_regressing: true`, fitness 67% (4/6), failed cases `all_tests_pass` and `has_tests`, `source_revision: 2f88a93`, `source_diff_hash: 9916603b8d352a3316b9e1964392693f33fa41ec`. Saved-artifact support evidence `runs/20260703-coder-benchmark-integrity-evidence/baseline-good/baseline-sudoku-solver-cycle-0-fitness-evidence.json` is full-passing with the same source diff hash. This is non-regressing source-patch/prompt-integrity evidence, not proof of Senior SWE-Bench task-solving.
