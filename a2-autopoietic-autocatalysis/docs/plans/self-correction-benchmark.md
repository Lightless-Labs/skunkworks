# Self-Correction Benchmark Plan

**Created:** 2026-04-24
**Completed:** 2026-04-28
**Verified:** 2026-04-28 — `--self-test`, `--smoke-only`, Minimax N=3 real-provider run, and one Kimi smoke.
**Addendum:** 2026-04-28 — `compound-hidden` exercises prior lineage but did not self-correct; next work is loop recovery, not harness creation.
**Addendum:** 2026-05-31 — Added `compound-constitution-same-crate-hidden` to extend same-crate fixture diversity into bootstrap-profile behavior; smoke-only injection verified both failures and Minimax N=3 resolved 3/3 with pass@1 2/3 and self-corrected 1/3.

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
- reconciles the newest lineage row with the external verification result so failed post-apply attempts are visible to later motifs
- appends one JSON object per attempt

Implemented `bench/self_correction_score.py` to separate first-pass model capability from actual self-correction:

- `resolved`
- `pass@1`
- `loop exercised` (any later attempt saw prior lineage)
- `self-corrected` (attempt 1 failed, later prior-lineage attempt passed)

Keep it observational with respect to the main workspace. Candidate fixes may mutate only the isolated task workspace.

## 2026-04-28 Result

Minimax N=3 on the `fibonacci` fixture:

- `resolved`: 3/3
- `pass@1`: 3/3
- `loop exercised`: 0/3
- `self-corrected`: 0/3

Kimi smoke also passed `fibonacci` on attempt 1. Gemini produced lineage records across two attempts but failed due 429 provider capacity errors.

Added `compound-hidden`, which injects the Fibonacci regression plus a hidden TODO-scanner string-literal regression. Its verification command reveals both failures after each attempt.

Minimax on `compound-hidden` for 3 attempts:

- `resolved`: 0/1
- `pass@1`: 0/1
- `loop exercised`: 1/1
- `self-corrected`: 0/1

Conclusion: the harness now distinguishes three cases: easy pass@1, loop exercised, and actual self-correction. A² memory is visible on later attempts, but the current prompt/motif path did not recover from the compound hidden failure.

## Follow-up TODOs

- [x] Render prior external verification failures prominently in the catalyst prompt instead of relying on compact motif snippets. Completed 2026-05-01.
- [x] Persist post-apply verification outcome in the main `a2ctl run`/lineage path; remove benchmark-only lineage reconciliation once the core path records truth. Completed 2026-05-01.
- [x] Add attempt diff/touched-file summaries to self-correction JSONL records. Completed 2026-05-01.
- [x] Re-run `compound-hidden` N≥3 after motif/run-path changes. Completed 2026-05-21 after candidate-worktree verifier wiring.
- [x] Add a second hard fixture once at least one provider self-corrects `compound-hidden`. Completed 2026-05-18 with `compound-membrane-hidden`.
- [x] Add additional same-crate fixture diversity beyond Sensorium/RAF/Eval/Broker. Completed 2026-05-31 with `compound-constitution-same-crate-hidden`.

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
