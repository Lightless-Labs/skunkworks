# Test Evolution: Tests Are Part of the Self-Modification Surface

**Created:** 2026-04-06
**Addendum:** 2026-06-08 — Expanded non-sudoku challenge holdout coverage in `crates/a2d-core/src/challenges.rs`. Chess now has hidden tests for legal-move safety, castling, en passant, and Fool's mate/no-escape. Rubik's now has an explicit callable API plus hidden tests for rotation inverses, quarter-turn order, known inverse roundtrip, solver-on-known-scrambles, and seeded scramble replayability/solve roundtrip. Plan: `docs/plans/challenge-acceptance-test-expansion.md`.

## The Problem

The architect can modify `metabolism.rs` and the self-sandbox validates the patch by running `cargo test`. If the existing tests pass, the patch is accepted. But:

1. **Stale tests can hide semantic drift.** A test that asserts "the coder fires before the tester" may keep passing after the architect changes the scheduling order, if the test happens to still hold for the wrong reason. The test no longer encodes the intended invariant.

2. **Correct changes can be rejected by stale tests.** A test that asserts "fitness is measured at end of cycle" will break when the architect (correctly) moves fitness measurement inside the invocation loop. The patch is correct; the test is wrong; the patch gets rejected.

3. **The architect treats tests as immutable.** Currently, tests are not in `PROTECTED_FILES`, but the architect's prompt doesn't explicitly tell it to evolve tests alongside production code. It defaults to leaving them alone.

## The Requirement

When the architect proposes a change to production code, it must also consider whether the tests covering that code still encode the right invariants. If not, the patch must include updated tests — or be split into a paired patch (test update first, then production change).

## Implementation Sketch

### 1. Update the architect's system prompt
Add to `architect_system_prompt()`:

> Tests are part of the system you can modify. They are NOT protected files.
>
> When you change production code:
> - Identify the tests that exercise the changed code
> - Decide whether they still encode the right invariants
> - If a test asserts behavior you're changing, update the test in the same patch
> - If a test asserts an invariant that should still hold, leave it alone (the self-sandbox will verify)
>
> You can output multiple SystemPatch objects in a single response — one per file. Use this when a production change requires a test change.

### 2. Multi-patch support in the metabolism
Currently `apply_system_patch` parses a single `SystemPatch` from the architect's output. Extend to parse `Vec<SystemPatch>` so the architect can ship a production change + test change atomically. Both must validate together (cargo test on the combined modification) before either is applied.

### 3. Test the test evolution capability
A mock-based test that:
- Sets up a synthetic project with a production file and a test file
- Mocks the architect to return a paired patch (production change + test change)
- Verifies both patches are applied together if cargo test passes on the combined state
- Verifies neither is applied if cargo test fails

### 4. Document in CLAUDE.md
Add to design principles: "Tests are not protected. The architect must evolve tests alongside production code when semantics change. The self-sandbox validates the combined state, not each patch in isolation."

## What This Doesn't Solve

Test evolution doesn't catch the case where the architect changes BOTH production code and tests in a way that's internally consistent but semantically wrong (e.g., adds a bug to production code, updates the test to expect the bug). The acceptance tests on real challenges are the backstop — they're holdout, the architect can't see them, and they catch behavioral regressions that internal tests miss.

So the layered defense is:
1. **Internal tests** verify production code matches the architect's intended behavior
2. **Acceptance tests** verify the system's behavior matches the user's intended outcome
3. The architect can game (1) by editing both sides; it cannot game (2) because it doesn't see them

## Priority

This should be built before the escalation ladder. Otherwise, every escalation rung the architect proposes will be tested against stale logic and either pass falsely or fail falsely.
