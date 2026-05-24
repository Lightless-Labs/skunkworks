# Autonomous Project Loop

**Created:** 2026-05-24
**Started:** 2026-05-24 — first executable autopilot surface
**Enhanced:** 2026-05-24 — structured monitor logs
**Enhanced:** 2026-05-24 — temp-worktree validation
**Enhanced:** 2026-05-24 — real-tree apply and local commit gate
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
- Docs/todos/plans changes are limited to approved markdown paths.
- [ ] Failed validation creates a typed `project_validation_report` and routes to a bounded repair/escalation loop instead of immediately waiting for a human. Validation report exists; repair/escalation remains open.
- Protected-file changes are rejected as hard safety stops; eligible source self-modifications are not.
- [x] Passing non-dry-run iterations apply changes, rerun gates, update handoff, and make an atomic local git commit.
- [ ] Failure after repair/escalation budget stops the loop with a clear report and a machine-readable monitor log; no silent partial application. Rollback exists for failed real-tree validation; bounded repair/escalation remains open.

## Notes

This is the missing loop that currently requires the human to keep prompting the coding assistant. Provider-policy topology gating remains important, but this task is more fundamental to the project's stated autonomy goal.

Do not implement this as a neutered docs-only task runner. The point is gated autonomous self-modification: source changes are allowed and expected when they pass mechanical safety, validation, repair, and commit gates.
