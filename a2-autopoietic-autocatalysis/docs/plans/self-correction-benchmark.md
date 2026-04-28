# Self-Correction Benchmark Plan

**Created:** 2026-04-24
**Completed:** 2026-04-28

## Goal

Add the first loop-shaped benchmark for A²: inject a deterministic bug, hide the exact repair location from the model, run repeated A² attempts with the same pinned `TaskId`, and measure whether prior lineage helps later attempts correct earlier failures.

## Why This Is Next

The previous prerequisite work is complete:

- Prior lineage is loaded into `ContextPack`.
- Prior motifs include pass/fail, rationale, and bounded diff snippets.
- `a2ctl run` can pin `TaskId` across invocations via JSONL `task_id`.
- The lockfile sentinel is green.

The remaining current benchmark suite is single-pass and mostly measures model capability. A self-correction benchmark exercises the loop: memory, repeated attempts, failure motifs, and changed strategy.

## Acceptance Criteria

1. A new command or script can create an isolated bugged workspace without mutating the germline.
2. The benchmark runs at least two A² attempts with the same task ID.
3. Attempt lineage is persisted and visible to later attempts.
4. The final result is machine-readable JSONL with fields for task ID, attempt number, provider/model, pass/fail, and whether prior lineage was present.
5. The benchmark can be run with a cheap provider without requiring Claude.
6. The implementation has unit tests for task ID reuse / result parsing and a small non-model smoke path where possible.

## Proposed Shape

Implemented as `bench/self_correction.py`:

- creates an isolated git worktree from the current germline
- injects and commits a deterministic `a2_core::fibonacci` bug only in the isolated branch
- emits repeated JSONL tasks with the same `task_id`
- invokes `a2ctl run --provider <provider> --apply`
- runs `cargo test -p a2_core test_fibonacci` after each attempt
- appends one JSON object per attempt

Keep it observational with respect to the main workspace. Candidate fixes may mutate only the isolated task workspace.

## Implementation Steps

1. Define the smallest deterministic bug fixture.
2. Add a JSONL result schema documented in `bench/README.md`.
3. Implement the harness with explicit attempt count and provider flags.
4. Add tests for result parsing / task ID reuse / workspace isolation.
5. Run a smoke verification without a real model where possible.
6. Run one real provider smoke only after the harness is deterministic.

## Non-Goals

- Do not claim benchmark superiority from one run.
- Do not integrate SWE-bench yet.
- Do not mutate the germline during evaluation.
- Do not use Claude for initial harness development.
