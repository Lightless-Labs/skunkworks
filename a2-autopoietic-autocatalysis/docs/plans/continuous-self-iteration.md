# Continuous Self-Iteration Plan

**Created:** 2026-05-25
**Status:** Initial slice implemented 2026-05-25
**Scope:** Make A² continuously pick, execute, verify, and log self-improvement work inside this repository.

## Goal

A² should not require a human to repeatedly say "keep going." The repository needs a first-class self-iteration path that can select project work, run workcells, preserve durable logs, and expose enough instrumentation for monitoring and steering.

## Non-goals

- No dependency on sibling projects.
- No directory traversal outside this repository.
- No hidden autonomous mutation without explicit `--apply`.

## Initial Slice

**Implemented:** 2026-05-25

Add `a2ctl autopilot` as the in-repo control loop entrypoint.

It should:

1. Accept explicit work through repeated `--task` and `--task-file` flags.
2. Discover candidate work from unchecked markdown checklist items in `todos/` and `docs/plans/` when no explicit task is supplied.
3. Include source-code `TODO`/`FIXME` scan results as lower-structure candidate work.
4. Pin stable task IDs from explicit task content or candidate source locations so lineage survives repeated autopilot runs.
5. Run up to `--max-iterations` workcells using configured providers.
6. Optionally apply promoted patches via the existing apply + verification path when `--apply` is explicit.
7. Write durable JSONL events under `.a2/autopilot/runs/<run-id>/events.jsonl`.
8. Support `--dry-run` for monitoring candidate selection without model calls.

## Follow-up Slices

- [ ] Persist richer run summaries with per-iteration patch stats and verifier focus.
- [ ] Add stop conditions for repeated failure classes, budget exhaustion, and provider quota failures.
- [ ] Add a resident/daemon wrapper once the CLI loop is reliable.
- [ ] Teach autopilot to update checklist state only after verified application.
- [ ] Add dashboard-friendly aggregate logs.
