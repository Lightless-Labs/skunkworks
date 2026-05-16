# Retry Task Contract From Verification TODO

Created: 2026-05-10
Completed: 2026-05-12

## Problem

A² remembers external verification failures, but the next attempt still receives them mainly as prior-attempt context. The model has repeatedly treated the original visible task as the real contract and ignored hidden verifier failures.

Observed fact: `compound-hidden` attempts after prior-lineage fixes repeatedly patched only `a2_core/src/lib.rs` and did not repair the `a2ctl` failure named by verification.

## Goal

On retry, verifier failures become part of the actual task contract / acceptance criteria, not just prompt motifs.

## Proposed approach

When `Governor::run_task` loads prior lineage for the same `TaskId`, derive retry requirements from failed external verification records:

- failing test names
- assertion focus lines
- verification command
- files mentioned in failure output

Then construct the workcell task so the model sees something equivalent to:

```text
Additional acceptance criteria from prior external verification:
- cargo test -p a2ctl ignores_non_task_mentions_inside_comments_and_strings must pass
- assertion failed: find_scan_marker("let s = \"// TODO: not a comment\";").is_none()
```

This may be implemented by enriching `TaskContract.acceptance_criteria`, adding a retry-specific field, or adding a typed `RetryContext` to `WorkcellConfig`.

## Acceptance criteria

- [x] Retry attempts include verifier-derived acceptance criteria in the task seen by the catalyst.
- [x] A test proves prior failed external verification changes the next prompt/task contract.
- [x] The original task description is preserved, but verifier-derived requirements are rendered as mandatory.
- [x] Multiple prior failures deduplicate failing tests/assertions.
- [x] The implementation avoids adding verifier requirements on first attempt.

## Verification

```bash
cargo test -p a2d -p a2_workcell
cargo run -p a2ctl -- sentinel --workspace .
```

After implementation, run:

```bash
bench/self_correction.py --fixture compound-hidden --provider opencode/minimax-coding-plan/MiniMax-M2.7 --attempts 3 --results /tmp/a2-compound-retry-contract.jsonl
bench/self_correction_score.py /tmp/a2-compound-retry-contract.jsonl
```
