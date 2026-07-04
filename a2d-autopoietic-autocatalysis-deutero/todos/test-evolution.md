# Test Evolution: Tests Are Part of the Self-Modification Surface

**Created:** 2026-04-06
**Addendum:** 2026-06-08 — Expanded non-sudoku challenge holdout coverage, now housed in `crates/a2d-cli/src/challenges.rs` after the 2026-07-04 core-boundary cleanup. Chess has hidden tests for legal-move safety, castling, en passant, and Fool's mate/no-escape. Rubik's has an explicit callable API plus hidden tests for rotation inverses, quarter-turn order, known inverse roundtrip, solver-on-known-scrambles, and seeded scramble replayability/solve roundtrip. Plan: `docs/plans/challenge-acceptance-test-expansion.md`.
**Addendum:** 2026-06-09 — Bounded seed chess live smoke was inconclusive: coder timed out before producing code, so the expanded holdouts did not execute. A 1s trace probe confirmed Kimi and DeepSeek did launch in the parallel coder portfolio. Next validation should isolate provider quality or mechanically replay candidate chess code against the holdouts rather than repeating the same one-cycle smoke.
**Addendum:** 2026-06-10 — Mechanical replay path implemented: `a2d score-artifact <challenge> <path|->` scores a saved artifact through `Challenge::score_artifact()`, which centrally attaches hidden acceptance tests via `Challenge::scoring_benchmark()`. Raw challenge benchmark/acceptance fields are private to prevent visible-only scoring bypasses. Failed replay exits 2 and diagnostics are redacted by default to preserve the hidden-test barrier. This is now the preferred next step for candidate chess/Rubik's code before another slow provider smoke.
**Completed:** 2026-06-11 — Architect multi-`SystemPatch` support landed. `self_sandbox::validate_patches()` validates patch batches atomically in one temp project copy, standalone internal test files are eligible for architect modification, `Metabolism::apply_system_patch()` accepts legacy single patches/noops plus JSON arrays, and mock tests prove combined production+test acceptance plus no partial queue on combined failure. Plan: `docs/plans/test-evolution-multipatch.md`.

## The Problem

The architect can modify `metabolism.rs` and the self-sandbox validates the patch by running `cargo test`. If the existing tests pass, the patch is accepted. But:

1. **Stale tests can hide semantic drift.** A test that asserts "the coder fires before the tester" may keep passing after the architect changes the scheduling order, if the test happens to still hold for the wrong reason. The test no longer encodes the intended invariant.

2. **Correct changes can be rejected by stale tests.** A test that asserts "fitness is measured at end of cycle" will break when the architect (correctly) moves fitness measurement inside the invocation loop. The patch is correct; the test is wrong; the patch gets rejected.

3. **The architect treats tests as immutable.** Currently, tests are not in `PROTECTED_FILES`, but the architect's prompt doesn't explicitly tell it to evolve tests alongside production code. It defaults to leaving them alone.

## The Requirement

When the architect proposes a change to production code, it must also consider whether the tests covering that code still encode the right invariants. If not, the patch must include updated tests — or be split into a paired patch (test update first, then production change).

## Implementation Sketch

### 1. Update the architect's system prompt — IMPLEMENTED
Added to `architect_system_prompt()`:

> Tests are part of the system you can modify. They are NOT protected files.
>
> When you change production code:
> - Identify the tests that exercise the changed code
> - Decide whether they still encode the right invariants
> - If a test asserts behavior you're changing, update the test in the same patch
> - If a test asserts an invariant that should still hold, leave it alone (the self-sandbox will verify)
>
> You can output multiple SystemPatch objects in a single response — one per file. Use this when a production change requires a test change.

### 2. Multi-patch support in the metabolism — IMPLEMENTED
`apply_system_patch` now accepts a single `SystemPatch`, a no-op object, or `Vec<SystemPatch>` JSON (including raw/fenced arrays after artifact materialization). Batches route through `self_sandbox::validate_patches()`, which applies all patches to one temp copy and runs one `cargo test` before any member is queued.

### 3. Test the test evolution capability — IMPLEMENTED
Mock/minimal-fixture tests now:
- Sets up a synthetic project with a production file and a test file
- Mocks the architect to return a paired patch (production change + test change)
- Verifies both patches are applied together if cargo test passes on the combined state
- Verifies neither is applied if cargo test fails

### 4. Document in CLAUDE.md — IMPLEMENTED
Documented the design principle in `CLAUDE.md` and mirrored it in `AGENTS.md`: tests are not protected physics; the architect must evolve internal tests alongside production semantics in an atomic `SystemPatch` batch; hidden holdouts remain the behavioral backstop.

## What This Doesn't Solve

Test evolution doesn't catch the case where the architect changes BOTH production code and tests in a way that's internally consistent but semantically wrong (e.g., adds a bug to production code, updates the test to expect the bug). The acceptance tests on real challenges are the backstop — they're holdout, the architect can't see them, and they catch behavioral regressions that internal tests miss.

So the layered defense is:
1. **Internal tests** verify production code matches the architect's intended behavior
2. **Acceptance tests** verify the system's behavior matches the user's intended outcome
3. The architect can game (1) by editing both sides; it cannot game (2) because it doesn't see them

## Status

Implemented for the current single-repo self-modification surface. Future work may broaden test-file eligibility or improve architect context/full-source inclusion for larger test edits, but the atomic batch mechanism and core coverage are in place.
