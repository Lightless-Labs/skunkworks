# Autonomous Project Loop

**Created:** 2026-05-24
**Started:** 2026-05-24 — first executable autopilot surface
**Enhanced:** 2026-05-24 — structured monitor logs
**Enhanced:** 2026-05-24 — temp-worktree validation
**Enhanced:** 2026-05-24 — real-tree apply and local commit gate
**Enhanced:** 2026-05-25 — bounded repair loop
**Enhanced:** 2026-05-25 — bounded repair/escalation contract captured
**Enhanced:** 2026-05-25 — provider-diverse repair escalation contract captured
**Enhanced:** 2026-05-26 — provider-diverse repair, state refresh, and completed-task filtering implemented
**Enhanced:** 2026-05-29 — repair-path fault injection added and live Pi → alternate-provider escalation validated; alternate repair provider is configurable
**Enhanced:** 2026-05-30 — tightened repair prompts and live-validated a DeepSeek alternate repair through path/temp/real-tree gates
**Enhanced:** 2026-05-31 — semantic project-reference validation added for autopilot markdown outputs
**Hardened:** 2026-06-14 — real-tree autopilot stage/commit now scopes `git add` and `git commit` to touched project paths so parent-repo staged/untracked noise remains outside the project commit
**Hardened:** 2026-07-03 — source self-modification apply/commit requires fresh source-bound `a2d.fitness-evidence.v1` actual-test evidence, not only cargo-test validation
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
- [x] Source/mechanism real-tree apply/commit requires fresh source-bound `a2d.fitness-evidence.v1` actual-test evidence. Autopilot accepts `--source-fitness-evidence <path>` / `A2D_AUTOPILOT_SOURCE_FITNESS_EVIDENCE`, validates schema, non-regression, hidden/aggregate status, source revision, dirty status, `source_diff_scope: crates`, and `source_diff_hash` against the current applied `crates` diff, and fails closed without it.
- [x] Docs/todos/plans changes are limited to approved markdown paths.
- [x] Markdown replacements and `handoff_update` are semantically checked for repo path references during temp-worktree validation, so invented `crates/...`, `docs/...`, `todos/...`, `examples/...`, and `research/...` paths fail before real-tree apply/commit.
- [x] Failed validation creates a typed `project_validation_report` and routes to a bounded repair/escalation loop instead of immediately waiting for a human. Parse, path, temp-validation, real-apply, and provider invocation failures now route to bounded repair attempts.
- [x] Protected-file changes are rejected as hard safety stops; eligible source self-modifications are not.
- [x] Passing non-dry-run iterations apply changes, rerun gates, update handoff, and make an atomic local git commit scoped to the touched project paths.
- [x] Failure after repair/escalation budget stops the loop with a clear report and a machine-readable monitor log; no silent partial application. Rollback exists for failed real-tree validation and `repair_budget_exhausted` records terminal failure.
- [x] Provider-diverse escalation for repair attempts. Repair attempt 1 now routes to the configured alternate maintainer provider when available, while monitor events and repair prompts record primary/attempted provider metadata. Live fault-injection run `run-1780061191713-0` validated Pi primary → Kimi alternate routing and bounded budget exhaustion. `A2D_AUTOPILOT_REPAIR_PROVIDER` / `--repair-provider` now allows an explicit registered repair provider; DeepSeek probes validated routing. After prompt tightening, run `run-1780125199376-0` validated a DeepSeek repair output through path gate, temp `cargo test`, real-tree `cargo test`, and commit.
- [x] Refresh `project_state` after each committed iteration so `--iterations N` does not select from stale handoff/todo/git status.
- [x] Improve task selection/completion detection so autopilot does not keep selecting already-satisfied checkbox todos.

## Bounded repair/escalation contract

The remaining implementation gap is not another unconstrained provider retry. It should be a typed, bounded continuation of the same gated patchset flow:

1. When parse, path-gate, temp-worktree validation, or real-tree validation fails, write a `project_validation_report` artifact and emit a JSONL monitor event that names the failed phase.
2. Treat protected-file edits, traversal attempts, invalid replacement paths, and non-eligible mechanism edits as hard safety stops. These must not be repaired automatically in-place.
3. For repairable failures, re-prompt the project-work provider with the original `project_state`, selected `project_task`, rejected `project_patchset`, and `project_validation_report`.
4. Require the repair response to use the same typed `project_patchset` JSON contract. Do not accept shell commands or free-form instructions as repairs.
5. Apply the same parse, path, temp-worktree, source self-sandbox, and validation gates to each repair patchset.
6. Keep a small explicit repair budget, initially one repair attempt per autopilot iteration.
7. If the budget is exhausted, stop the iteration with a clear failed status, leave the real working tree unchanged unless rollback has succeeded after a real-tree gate failure, and record the final report in the monitor log.

## Provider-diverse escalation contract

Provider-diverse repair should be a bounded extension of the repair loop, not a second uncontrolled agent path:

1. Record the primary maintainer provider/model used for the original project patchset in the monitor log and in each repair prompt artifact.
2. On the first repairable failure, retry with the same provider only if no alternate project-maintainer provider is configured or healthy.
3. When an alternate provider/model is configured and circuit-breaker policy allows it, escalate exactly one repair attempt to that alternate provider/model before declaring the repair budget exhausted.
4. The escalated provider receives the same structured inputs as normal repair: original `project_state`, selected `project_task`, rejected `project_patchset`, `project_validation_report`, and the prior provider/model metadata.
5. Escalated repair output must still be typed `project_patchset` JSON and must pass the identical parse, path, temp-worktree, source self-sandbox, validation, rollback, handoff, and commit gates.
6. Monitor events must make provider diversity auditable by naming the attempted provider/model, escalation reason, repair attempt index, and terminal outcome.
7. Provider-diverse escalation must never override hard safety stops for protected paths, traversal, invalid replacement paths, or non-eligible mechanism edits.

## Live validation notes

2026-05-29: Added explicit repair-path fault injection (`A2D_AUTOPILOT_FAULT_INJECTION=attempt0_parse_failure`) and ran `cargo run -q -p a2d -- autopilot --iterations 1 --repair-attempts 1`. Attempt 0 invoked `pi/default`, fault injection forced a parse failure, repair attempt 1 escalated to `opencode/kimi-for-coding/k2p6`, and the bounded repair budget exhausted after Kimi timed out at 90s. Monitor run: `.a2d/autopilot/runs/run-1780061191713-0/`; console log: `/tmp/a2d-autopilot-repair-diversity-20260529132612.log`. Added configurable repair provider support and probed DeepSeek: `run-1780062413070-0` proved configured routing and path-gate rejection of zero replacements; `run-1780062590484-0` proved configured routing after parse-failure injection but DeepSeek timed out. Learning: `docs/solutions/runtime-bugs/autopilot-repair-diversity-live-validation-2026-05-29.md`.

2026-05-30: Tightened maintainer/repair prompts to explicitly forbid empty `replacements` and require markdown tasks to update an approved markdown file. Reran the configured DeepSeek fault-injection probe; `run-1780125199376-0` reached the full repair path and committed `ab43b71` after path gate, temp `cargo test`, and real-tree `cargo test`. Follow-up: generated todo text referenced non-existent source files before correction, so semantic project-reference validation is still needed.

2026-05-31: Added semantic project-reference validation to the autopilot temp-worktree gate. Markdown replacements and `handoff_update` now fail validation if they reference repo paths that do not exist after patch application; maintainer/repair prompts warn providers not to invent repo paths. Unit coverage includes the previous failure shape (`metabolism_workcell.rs`, `provider_registry.rs`) and an accepted case for existing paths with anchors/line suffixes.

2026-06-14: Tightened the real-tree stage/commit gate for monorepo/subtree use. `git status --short -- .` already scopes autopilot dirtiness to the A²D project path, but an unscoped `git commit -m ...` could still include unrelated staged paths from the parent git repository. `apply_validated_patchset_to_real_tree` stages with scoped `git add <touched_paths>` and now commits with `git commit -m <message> -- <touched_paths>`. Unit coverage proves a staged parent sibling (`A  ../sibling.md`) remains staged and absent from the autopilot commit.

2026-07-03: Tightened source self-modification persistence. A source-changing `ProjectPatchset` now fails real-tree apply/commit without fresh source-bound `a2d.fitness-evidence.v1` actual-test evidence; matching evidence is accepted only when source provenance and hidden/aggregate non-regression status match the current applied `crates` diff. Unit coverage includes missing evidence, stale hash, bad revision/dirty/scope/command, non-actual evidence, regressing evidence, and apply-report JSON fields. Fresh gate evidence: `runs/20260703-autopilot-source-fitness-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json` with `source_diff_hash: 5d105fe70f380e1635100c4c663642da7fe614df`.

## Notes

This is the missing loop that currently requires the human to keep prompting the coding assistant. Provider-policy topology gating remains important, but this task is more fundamental to the project's stated autonomy goal.

Do not implement this as a neutered docs-only task runner. The point is gated autonomous self-modification: source changes are allowed and expected when they pass mechanical safety, validation, repair, and commit gates.
