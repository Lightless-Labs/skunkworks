# Worktree Task Verifier TODO

Created: 2026-05-10

## Problem

The workcell can be promoted based on pre-apply evaluator state, then fail the real post-apply verification gate. The system reconciles that later, but the candidate has already been treated as promotable for the current attempt.

For loop-shaped tasks, the task-specific verifier should be runnable in the candidate worktree before promotion scoring, so hidden verifier failures become somatic fitness immediately.

## Goal

Allow task-specific verification commands to run inside the worktree before promotion decision.

## Proposed approach

Carry verifier command data from benchmark/task input into the task/evaluator path. Options:

- add verification command metadata to `TaskContract`
- add a `TaskVerifier` trait in `a2_core::traits`
- extend `SeedEvaluator` or add a new evaluator that can run configured commands in the candidate worktree

For `compound-hidden`, the candidate should not be considered complete unless both commands pass:

```bash
cargo test -p a2_core test_fibonacci
cargo test -p a2ctl ignores_non_task_mentions_inside_comments_and_strings
```

## Acceptance criteria

- [ ] Task-specific verifier commands can be represented without ad hoc prompt text.
- [ ] WorktreeCatalyst/evaluator can run verifier commands in the candidate worktree.
- [ ] Failed verifier commands set somatic `task_completed=false` and `tests_pass=false` before promotion decision.
- [ ] Verification output is persisted into structured external verification or equivalent lineage data.
- [ ] Existing benchmark/run behavior remains compatible for tasks without explicit verifier commands.

## Verification

```bash
cargo test -p a2_core -p a2_workcell -p a2_eval -p a2d -p a2ctl
cargo run -p a2ctl -- sentinel --workspace .
```

Then run a `compound-hidden` attempt and verify the first attempt is not marked promotable if the hidden verifier fails.
