---
date: 2026-06-29
topic: fitness-evidence-self-improvement
---

# Research: Fitness Evidence Self-Improvement

## Codebase Context
- A²D is a Rust 2024 workspace with `a2d-core`, `a2d-providers`, and `a2d-cli`; local validation uses `cargo test`.
- Challenge scoring is centralized in `crates/a2d-core/src/challenges.rs`: `Challenge::scoring_benchmark()` attaches hidden acceptance tests, and `Challenge::score_artifact()` replays generated artifacts against the same visible+hidden benchmark.
- Fitness evaluation lives in `crates/a2d-core/src/benchmark.rs`: `BenchmarkSuite::evaluate()` returns `FitnessReport { total, passed, failed, fitness, results, diagnostic }`. Diagnostics intentionally omit hidden acceptance source code.
- Runtime feedback lives in `crates/a2d-core/src/metabolism.rs`: after a code-producing invocation, the metabolism evaluates code, stores `fitness_report` and `failure_report` artifacts, records `fitness_delta`, and routes later cycles to coder/evolver/architect.
- Durable lineage decisions live in `crates/a2d-cli/src/main.rs`: `run_cycle` commits accepted mutations if the cycle did not regress; `run_challenge` commits the current germline only when challenge best fitness improves.

## Existing Work
- `docs/plans/stage-2-verified-improvement.md` names the desired invariant: accepted mutations need mechanical fitness deltas, not just RAF closure.
- `docs/solutions/architectural-insights/evolver-enzyme-produces-zero-measurable-improvement-2026-04-04.md` says RAF-valid mutations have not produced measurable value.
- `docs/solutions/architectural-insights/broken-feedback-loop-coder-never-sees-failure-output-2026-04-04.md` is partially addressed by `failure_report`, but the main `fitness_report` artifact is still a terse scalar summary.
- `docs/solutions/runtime-bugs/challenge-scoring-must-use-hidden-holdouts-2026-06-10.md` established that replay/baseline/challenge paths must all use hidden holdouts.
- `docs/plans/test-evolution-multipatch.md` and `todos/test-evolution.md` establish that internal tests are mutable by architect patches, while hidden holdouts remain the behavioral backstop.

## Relevant Code
- `crates/a2d-core/src/benchmark.rs`
  - `FitnessReport` already carries per-case pass/fail results and optional diagnostics.
  - `format_sandbox_diagnostic()` captures compile/test/runtime output without exposing hidden acceptance source.
- `crates/a2d-core/src/metabolism.rs`
  - `run_cycle()` evaluates code and currently stores `fitness_report` as `"fitness: X, passed: Y, failed: Z, total: N"`.
  - The same block stores diagnostics separately in `failure_report`.
  - `evolver_system_prompt()` tells the evolver to use `fitness_report` and `failure_report`, but the fitness artifact lacks structured per-case evidence.
- `crates/a2d-cli/src/main.rs`
  - `run_cycle()` can durably commit accepted mutations even when `report.fitness_delta` is `None` because it only checks regression.
  - `run_challenge()` is stricter by committing only on improved best fitness.
  - `format_score_artifact_report()` redacts diagnostics for CLI replay output; this preserves the hidden-test barrier and is not the inner-loop feedback path.

## External References
- No external library research was needed: the slice uses existing Rust/serde/cargo-test patterns already present in the project.

## Test Landscape
- Existing core tests cover feedback report population, failure-report prompt injection, fitness regression ratcheting, and challenge scoring through hidden holdouts.
- Existing CLI tests cover score-artifact redaction and many provider/topology gates.
- Useful new tests:
  - `fitness_report` artifact includes structured schema, delta, per-case results, failed case names, and diagnostic availability.
  - Durable mutation lineage skips accepted mutations when no code was scored in the same cycle.

## Implementation Follow-up
- This research identified the seam implemented in the same session: structured `a2d.fitness-evidence.v1` artifacts now replace the scalar `fitness_report`, hidden-specific case names are redacted, and CLI durability/application gates require non-regressing benchmark evidence before mutations, provider policies, or patches become durable/applied.
- The CLI package is named `a2d`; use `cargo test -p a2d`, not `cargo test -p a2d-cli`.

## Open Questions
- A stronger future gate may need deferred mutation validation: compare pre-mutation and post-mutation topology on the same challenge before accepting mutations in memory, not only before durable lineage commit.
- The sandbox currently exposes aggregate test pass/fail counts rather than individual hidden acceptance test names; richer redacted failure labels would require sandbox output parsing beyond this slice.
