# Autonomous Project Loop

**Created:** 2026-05-24
**Started:** 2026-05-24 — first executable autopilot surface
**Enhanced:** 2026-05-24 — structured monitor logs
**Enhanced:** 2026-05-24 — temp-worktree validation
**Enhanced:** 2026-05-24 — real-tree apply and local commit gate
**Enhanced:** 2026-05-25 — bounded repair loop
**Enhanced:** 2026-05-25 — bounded repair/escalation contract captured
**Plan:** `docs/plans/autonomous-project-loop.md`

## Context

A²D still depends on a human/operator to do the outer repo-maintenance loop:

```text
read handoff/todos → choose next task → edit code/docs → run gates → validate → commit → update handoff → repeat
```

The inner challenge metabolism is bounded and self-adaptive, but no command owns this outer loop. Current CLI commands terminate after `cycle`, `challenge`, or `compare-topologies`; they do not select backlog work, edit the repo as project work, commit changes, or update handoff autonomously.

## Acceptance criteria

- [x] `a2d autopilot --iterations 1 --dry-run` exists and does not modify the working tree.
- [x] Autopilot builds a typed `project_state` from handoff, todos/plans, git status, tests/status summaries.
- [x] Autopilot selects one concrete `project_task` with explicit validation gates, including tasks that modify A²D's own source.
- [x] Provider-generated project work is represented as typed `project_patchset` JSON, not arbitrary shell commands.
- [x] Autopilot emits structured monitor logs under `.a2d/autopilot/`, including prompts/provider outputs as artifacts and parse/path-gate outcomes as JSONL events.
- [x] Patchsets are path-gated and validated in a temp worktree before real application.
- [x] Source/mechanism self-modifications go through self-sandbox/cargo-test gates. Path gate identifies eligible source self-modification, temp validation requires the source target to exist, and `cargo test` is injected when needed.
- [x] Docs/todos/plans changes are limited to approved markdown paths.
- [x] Failed validation creates a typed `project_validation_report` and routes to a bounded repair/escalation loop instead of immediately waiting for a human. Parse, path, temp-validation, real-apply, and provider invocation failures now route to bounded repair attempts.
- Protected-file changes are rejected as hard safety stops; eligible source self-modifications are not.
- [x] Passing non-dry-run iterations apply changes, rerun gates, update handoff, and make an atomic local git commit.
- [x] Failure after repair/escalation budget stops the loop with a clear report and a machine-readable monitor log; no silent partial application. Rollback exists for failed real-tree validation and `repair_budget_exhausted` records terminal failure.
- [ ] Provider-diverse escalation for repair attempts. Current repair loop uses the assigned maintainer provider; model/provider swap remains open.

## Bounded repair/escalation contract

The remaining implementation gap is not another unconstrained provider retry. It should be a typed, bounded continuation of the same gated patchset flow:

1. When parse, path-gate, temp-worktree validation, or real-tree validation fails, write a `project_validation_report` artifact and emit a JSONL monitor event that names the failed phase.
2. Treat protected-file edits, traversal attempts, invalid replacement paths, and non-eligible mechanism edits as hard safety stops. These must not be repaired automatically in-place.
3. For repairable failures, re-prompt the project-work provider with the original `project_state`, selected `project_task`, rejected `project_patchset`, and `project_validation_report`.
4. Require the repair response to use the same typed `project_patchset` JSON contract. Do not accept shell commands or free-form instructions as repairs.
5. Apply the same parse, path, temp-worktree, source self-sandbox, and validation gates to each repair patchset.
6. Keep a small explicit repair budget, initially one repair attempt per autopilot iteration.
7. If the budget is exhausted, stop the iteration with a clear failed status, leave the real working tree unchanged unless rollback has succeeded after a real-tree gate failure, and record the final report in the monitor log.

This contract is the next executable slice for closing the two remaining repair/escalation acceptance criteria.

## Notes

This is the missing loop that currently requires the human to keep prompting the coding assistant. Provider-policy topology gating remains important, but this task is more fundamental to the project's stated autonomy goal.

Do not implement this as a neutered docs-only task runner. The point is gated autonomous self-modification: source changes are allowed and expected when they pass mechanical safety, validation, repair, and commit gates.
