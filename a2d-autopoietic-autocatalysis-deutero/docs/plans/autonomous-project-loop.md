# Autonomous Project Loop

**Created:** 2026-05-24
**Started:** 2026-05-24 — first executable `a2d autopilot` surface
**Enhanced:** 2026-05-24 — structured monitor logs for external steering
**Enhanced:** 2026-05-24 — temp-worktree patchset validation
**Enhanced:** 2026-05-24 — gated real-tree apply and local commit
**Enhanced:** 2026-05-25 — bounded repair loop
**Enhanced:** 2026-05-26 — provider-diverse repair, state refresh, and completed-task filtering

## Problem

A²D has an inner metabolism for challenge artifacts, but the outer project-work loop is still performed by the human/operator plus coding assistant:

```text
read handoff/todos → choose next repo task → edit code/docs → run gates → validate live behavior → commit → update handoff → repeat
```

That is why a human still has to prompt “keep moving.” The executable system terminates after bounded `cycle`, `challenge`, or `compare-topologies` commands. It can mutate germline/provider policy during those runs, but it does not own the repo-maintenance workflow that has been happening in assistant sessions.

## Goal

Close the outer loop by making project maintenance and self-modification itself a typed, mechanically gated metabolism.

Self-modification is not a non-goal or a later add-on. It is the central behavior: A²D should improve its own source, topology, provider policy, tests, plans, and operating procedure while it works. The constraint is not “no self-modification”; the constraint is “no ungated self-modification.”

A²D should be able to run a bounded unattended loop that:

1. Reads `docs/HANDOFF.md`, `todos/`, `docs/plans/`, recent lineage, and git state.
2. Selects one concrete project task, including tasks that modify A²D's own source.
3. Produces a typed project change proposal.
4. Applies the proposal only through mechanical gates.
5. Runs required checks.
6. Recovers or escalates when gates fail instead of waiting for the human.
7. Commits an atomic change when gates pass.
8. Updates `docs/HANDOFF.md`.
9. Repeats until budget/safety stop.

## Non-goals

- No unbounded daemon in the first slice.
- No direct provider access to the working tree. Provider cwd isolation remains mandatory.
- No bypass of protected files or self-sandbox gates.
- No hidden acceptance-test or benchmark weakening.
- No automatic push to remotes.

These are safety boundaries, not a ban on self-modification. Autopilot must be allowed to change A²D's own eligible source files through the same or stronger gates as the architect path.

## Minimal slice: `a2d autopilot`

Add a CLI command:

```bash
a2d autopilot --iterations 1 --dry-run
a2d autopilot --iterations 3 --budget-minutes 60
```

The first implementation should be conservative and deterministic where possible.

Implemented first slice on 2026-05-24:

- `a2d autopilot --iterations N --dry-run --allow-dirty` exists.
- Builds `project_state` from handoff, todos, plans, git status, and `a2d status`.
- Selects `todos/autonomous-project-loop.md` before other todos.
- Builds a maintainer prompt that explicitly permits eligible source self-modification.
- Defines typed `project_patchset` JSON and fenced-JSON parsing.
- Path-gates patchsets: rejects absolute/traversal/protected paths; allows approved docs; allows eligible mechanism source files and marks them as requiring cargo-test/self-sandbox validation.
- Non-dry-run can invoke the maintainer provider and gate the returned patchset, but does not yet apply files.
- Every autopilot run emits structured JSONL events plus per-run artifacts under `.a2d/autopilot/` so an external monitor/steerer can evaluate both model outputs and mechanical outcomes.
- Gated patchsets are copied into a temp worktree, replacements are applied there first, allowlisted validation commands can run there, source self-modification requires an existing source target and injects `cargo test` when needed, and the validation report is logged as JSON before any real-tree application.
- Patchsets that pass temp validation are applied to the real tree, validation commands rerun, handoff updates are appended when needed, touched paths are committed locally, and failures roll back original file contents before stopping.
- Failed parse, path-gate, temp-worktree validation, real-tree validation/apply, or provider invocation now routes to a bounded repair prompt with the original task/context, previous output, and mechanical failure report. Configure with `--repair-attempts N` or `A2D_AUTOPILOT_REPAIR_ATTEMPTS` (default 1).

Implemented on 2026-05-26: provider-diverse repair escalation, multi-iteration state refresh after commits, and checkbox-based completed-task filtering for task selection.

Next slice: durable provider-policy topology gating, or deeper task completion semantics once more todos acquire machine-readable acceptance markers.

### Loop state artifacts

Introduce typed artifacts for outer project work:

- `project_state`
  - handoff summary
  - todo index
  - relevant plan headers
  - git status
  - latest test status
  - latest lineage/status summary
- `project_task`
  - selected todo/plan path
  - explicit objective
  - acceptance gates
  - stop conditions
- `project_patchset`
  - one or more file replacements with complete new content
  - commit message
  - handoff update text
  - validation commands to run
- `project_validation_report`
  - command statuses and output previews
  - git diff summary
  - accepted/rejected reason

### Monitor/steering logs

Autopilot must log for a separate monitor/controller, not only for humans reading stdout.

Current log locations:

- aggregate: `.a2d/autopilot/events.jsonl`
- per run: `.a2d/autopilot/runs/<run-id>/events.jsonl`
- per-run artifacts: `.a2d/autopilot/runs/<run-id>/...`

Events are JSON objects with:

- `ts_unix_ms`
- `run_id`
- `event`
- `data`

Current event types include:

- `run_started`
- `project_state_collected`
- `project_state_refreshed`
- `artifact_written`
- `task_selected`
- `maintainer_prompt_built`
- `dry_run_stop`
- `maintainer_provider_topology`
- `maintainer_invocation_started`
- `maintainer_invocation_failed`
- `maintainer_output_received`
- `patchset_parse_failed`
- `patchset_path_gate_evaluated`
- `temp_worktree_validation_completed`
- `real_tree_apply_started`
- `real_tree_apply_completed`
- `repair_attempt_started`
- `repair_output_received`
- `repair_budget_exhausted`
- `run_stopped_after_temp_validation`
- `run_stopped_before_apply`

The key property: outputs and outcomes are both logged. Provider outputs are written as artifacts; parse/path-gate/temp-validation/real-apply outcomes are JSONL events referencing those artifacts. Future repair attempts and multi-iteration state refresh must log the same way.

### New enzyme roles

Add outer-loop project enzymes outside the challenge topology:

- `task_selector`: consumes `project_state`, emits `project_task`.
- `maintainer`: consumes `project_state` + `project_task`, emits `project_patchset`.
- `repairer`: consumes `project_patchset` + `project_validation_report`, emits a repaired `project_patchset` after validation failure.

The maintainer/repairer prompts must require:

- JSON only.
- Complete file content replacements, not ad hoc shell commands.
- Small atomic changes.
- Source self-modifications when the selected task requires them.
- Tests or validation command list.
- Handoff update text.

### Patch gates

Project patches need stricter gating than normal assistant edits, while still allowing self-modification:

1. Normalize paths; reject absolute paths and `..` traversal.
2. Reject protected files unless a human explicitly changes the constitution/protection list outside autopilot.
3. For source/mechanism files, reuse `SystemPatch`/self-sandbox or an equivalent multi-file self-sandbox.
4. For docs/todos/plans, allow only repository-local markdown paths under approved directories.
5. Apply to a temp worktree first.
6. Run required commands, with `cargo test` mandatory for Rust/source changes.
7. If gates fail, route the failure report to `repairer` for a bounded number of repair attempts.
8. Refuse commit on failing gates, exhausted repair budget, or dirty unexpected files.

### Commit gate

If validation passes:

1. Apply patchset to the real working tree.
2. Run gates again on the real tree.
3. Update `docs/HANDOFF.md` with what happened and current test status.
4. Commit atomically.

Suggested commit message format:

```text
Autopilot: <short task description>
```

No remote push in this slice.

## Recovery and stop conditions

Autopilot should not stop at the first ordinary validation failure. It should recover mechanically first:

1. Convert failed command output, self-sandbox rejection, malformed patch output, or dirty-diff mismatch into `project_validation_report`.
2. Invoke `repairer` with the original task, rejected patchset, and validation report.
3. Re-apply in a fresh temp worktree.
4. Escalate provider/model after repeated equivalent failures.
5. Only stop after repair/escalation budget is exhausted or a hard safety boundary is hit.

Autopilot must stop and report instead of improvising when:

- working tree is dirty before start unless `--allow-dirty` is explicitly supplied;
- selected task has no concrete acceptance gate;
- provider output remains empty/malformed after bounded repair/escalation;
- patchset touches protected paths;
- validation command still fails after bounded repair attempts;
- `cargo test` still fails after bounded repair attempts;
- repeated iterations produce no accepted diff;
- budget/iteration limit is exhausted.

## First useful task source

The first deterministic task selector should rank existing todos by:

1. explicit dependency readiness;
2. handoff “Best next moves” order;
3. small bounded validation surface;
4. safety relevance to autonomy.

Given the current handoff, the likely first autopilot task is `todos/provider-policy-topology-gate.md`, but the autonomy gap itself should now be represented by this plan.

## Validation

Unit tests:

- task selector ignores completed checkbox todos;
- project patch parser rejects malformed JSON;
- path gate rejects absolute and traversal paths;
- docs-only patch can be validated without `cargo test`;
- source self-modification requires self-sandbox/cargo test;
- failed source self-modification routes to `repairer` before stopping;
- protected-file self-modification is rejected without repair;
- autopilot dry-run never modifies files;
- failed validation after repair budget does not commit.

Live smoke:

```bash
cargo test
a2d autopilot --iterations 1 --dry-run
```

Dry-run should print selected task, proposed gates, and whether a provider invocation would be made, without changing git state.

## Why this matters

Provider policy, topology comparison, and challenge feedback are inner mechanisms. They do not remove the operator as the outer scheduler. Closing this loop is the difference between “A²D can run cycles when invoked” and “A²D can continuously improve the project under bounded mechanical safety gates.”
