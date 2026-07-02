# Directed Evolver Mutation Policy — 2026-07-02

## Purpose

Constrain routine benchmark-driven evolver mutations to the lever that prior evidence says can plausibly improve code quality: existing enzyme `prompt_template` fields. Structural enzyme graph changes remain available through non-routine/architecture paths, but the benchmark feedback loop no longer treats add/remove/topology rewrites as ordinary evolver search after `fitness_report` exists.

Lineage search before this change found two constraining findings:

- `docs/solutions/architectural-insights/evolver-enzyme-produces-zero-measurable-improvement-2026-04-04.md` — blind structural mutations produced no measurable improvement; prompt mutations need failure context.
- `docs/solutions/architectural-insights/broken-feedback-loop-coder-never-sees-failure-output-2026-04-04.md` plus later `fitness_report` work — feedback now exists, but failed public/aggregate cases were not called out in the evolver prompt.

## Change

- `route_outputs` marks `enzyme_defs` outputs as routine evolver mutations only when the producer is the `evolver` and benchmark `fitness_report` exists.
- Routine evolver mutations may update `prompt_template` on existing enzymes only; additions and reactant/product/catalyst changes are rejected as structural architecture work.
- Non-routine mutation paths preserve existing add/replace behavior so architecture/bootstrap tests can still add enzymes.
- The evolver prompt extracts `failed_cases` / per-case status from structured `a2d.fitness-evidence.v1` and tells the evolver to target prompt templates without inferring hidden holdout details.

## Validation

Focused tests:

```bash
cargo test -p a2d-core rejects_routine_evolver_structural_mutations -- --nocapture
cargo test -p a2d-core evolver_prompt_surfaces_structured_failed_cases -- --nocapture
cargo test -p a2d-core loop_detection_byte_hash_escalates_non_benchmarked_enzyme -- --nocapture
cargo test -p a2d-core cycle_firing_cap_force_advances_when_enzymes_keep_retriggering -- --nocapture
cargo test -p a2d-core metabolism::tests::architect_system_patch_batch_is_accepted_and_queued_atomically_through_metabolism -- --nocapture
cargo test -p a2d-core metabolism::tests::fenced_json_array_is_extracted_for_system_patch_batches -- --nocapture
cargo test -p a2d-core metabolism --quiet
```

Full suite:

```bash
cargo test
```

Result: 259 passed, 2 ignored.

## Fresh evidence

Command:

```bash
A2D_GERMLINE=seed \
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260702-directed-evolver-fitness-evidence/challenge-sudoku2 \
A2D_PROVIDER_TIMEOUT_SECS=90 \
A2D_MAX_CYCLE_SECS=180 \
cargo run -p a2d -- challenge sudoku 2
```

Artifacts:

| Artifact | Fitness | `all_tests_pass` | Source revision | Source diff hash | Notes |
|---|---:|---|---|---|---|
| `runs/20260702-directed-evolver-fitness-evidence/challenge-sudoku2/sudoku-solver-cycle-0-fitness-evidence.json` | 83% (5/6) | false | `f155d39` | `7cc9a1c73e7f78fe74953c0d1e986b60ede18ea3` | Fresh actual-test evidence for the source patch |
| `runs/20260702-directed-evolver-fitness-evidence/challenge-sudoku2/sudoku-solver-cycle-0-consumed-by-cycle-1-fitness-evidence.json` | 83% (5/6) | false | `f155d39` | `7cc9a1c73e7f78fe74953c0d1e986b60ede18ea3` | Cycle 1 consumed fresh cycle 0 evidence before invoking evolver |

Both artifacts are `a2d.fitness-evidence.v1`, `actual_tests_evaluated: true`, and `non_regressing: true`, and bind to the exact source diff under `crates`. The hidden/acceptance aggregate still failed (`all_tests_pass: false`), so this is non-regressing source-patch evidence for directed-evolver gating, not evidence of benchmark mastery or repeated reliability improvement.
