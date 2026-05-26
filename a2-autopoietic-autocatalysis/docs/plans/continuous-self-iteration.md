# Continuous Self-Iteration Plan

**Created:** 2026-05-25
**Status:** Initial autopilot loop, verified checklist updates, and aggregate run logs implemented 2026-05-26
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
8. Write `run_summary.json` with per-iteration candidate source, model attribution, patch stats, verifier focus, and apply/rebuild outcome.
9. Support `--dry-run` for monitoring candidate selection without model calls.

## Follow-up Slices

- [x] Persist richer run summaries with per-iteration patch stats and verifier focus. Completed 2026-05-25 via autopilot self-iteration on explicit task `autopilot:explicit:c3569160d99bb4d4`; applied and verified clean.
- [x] Add stop conditions for repeated failure classes, budget exhaustion, and provider quota failures. Completed 2026-05-26 after the first stop-condition autopilot attempt exceeded budget; `autopilot_stopped` events and `run_summary.json.stop_reason` now distinguish budget, quota, repeated-failure, and max-iteration stops.
- [ ] Add a resident/daemon wrapper once the CLI loop is reliable.
- [x] Teach autopilot to update checklist state only after verified application. Completed 2026-05-26. Checklist-sourced candidates (`todos/...:<line>` or `docs/plans/...:<line>`) are marked `- [x]` only after `apply_ok && verify_ok`; updates are captured in `checklist_update` events and per-iteration `run_summary.json` fields.
- [x] Add dashboard-friendly aggregate logs. Completed 2026-05-26. Each completed autopilot run appends a compact record to `.a2/autopilot/run_index.jsonl` and updates `.a2/autopilot/latest_run.json` with the latest run pointer, summary metrics, stop reason, paths, and compact iteration outcomes.
