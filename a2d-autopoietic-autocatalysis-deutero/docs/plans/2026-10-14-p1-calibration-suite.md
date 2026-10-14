# Plan: P1 — Calibrate a Non-Saturated Suite

**Created:** 2026-10-14
**Status:** Active
**Parent:** `docs/plans/2026-10-14-lift-first-roadmap.md` § P1
**Exit (from parent):** Confirmation-1 and Confirmation-2 families generated and validated outside the repo with hashes logged; non-Claude review of the suite recorded; power planning done.

## Why

E01 showed Luna-low one-shot passes the legacy chess and sudoku tasks. A lift test on saturated tasks cannot show lift. P1 builds task families where one-shot passes 20–70% and best-of-4 at most 85%, so arm C has room above the comparators.

## Task format (unchanged controller contract)

One directory per task: `task.json` (`id`, `family`, `requirements`, `test_timeout_secs`), `public_tests.rs`, `hidden_tests.rs`. Single-file, std-only Rust 2024; tests are appended as a `#[cfg(test)]` module with `use super::*;`. Validation-only extras in the same directory, never read by arms:

- `reference.rs`: an independent correct implementation.
- `mutants/*.rs`: deliberately faulty variants of the reference (off-by-one, missed edge case, wrong error path, wrong ordering, partial feature).

## Families

Bounded, std-only micro-crates with real edge-case surface, each family with 4–8 tasks of graded difficulty:

1. Streaming codecs/parsers (e.g. base64/hex/varint/CSV with quoting, incremental feeds, error positions).
2. Dependency resolution (topological order, cycles with reported path, version constraints).
3. Transactional key-value store (nested begin/commit/rollback, snapshots, delete semantics).
4. Schedulers (priority with aging, rate limiters, interval merging, cron-like next-fire).
5. Seeded bug-repair: a reference crate with a planted bug and a spec; the task is to return the fixed file.
6. Text/data utilities with tricky specs (unicode-aware wrapping, semantic version ordering, glob matching).

Hold out whole families. Calibration, development and the two confirmation sets use disjoint families (different family *instances*, e.g. a different codec), so P2/P4 claims never rest on a family seen during development.

## Authoring and blindness

- **Calibration and development sets:** authored by Codex (`gpt-6.1-sol`, high effort) into the repo under `tasks/calibration/` and `tasks/dev/`. Free to inspect.
- **Confirmation-1 and Confirmation-2:** authored by Codex directly into `$A2D_HOLDOUT_DIR/confirmation-{1,2}/` (outside the repo). The developing agent never opens these files; it only runs `a2d lift validate` (prints per-task verdicts and hashes, no content) and commits the suite hashes.
- Codex runs in an empty directory per the reviewer-isolation practice; for holdouts its cwd is the holdout target.

## Oracle validation (mechanical, `a2d lift validate <suite>`)

A task is accepted only if:
- `reference.rs` passes all public and all hidden tests;
- every mutant fails at least one hidden test (an oracle that passes a mutant is rejected);
- at least one mutant passes all public tests (hidden tests must add discrimination beyond the public ones);
- hidden tests are not a copy of public tests (no shared test function names).
- (authoring rule, reviewed by hand on calibration sets) public tests exercise every major API item. E02's rubiks task showed that public tests which skip `solve` make the selection arms submit public-green failures.

Output: per-task verdict lines and the suite hash only.

## Experiments

- **E02 — calibration pilot** (pre-register before running): arms A and B (k = 4), Luna (`codex/gpt-6-luna`), effort pinned, on `tasks/calibration`. Headroom gate: A in 20–70%, B ≤ 85%, judged per family as well as pooled. If saturated: raise difficulty or drop to `gpt-5.6-luna` / lower effort — each counts as a suite or model redesign under T7 (max 2).
- **Power planning** from E02's A/B discordance and a D/C pilot on development tasks (P2 pilot): n for 80% power at 10 pp, capped at 150.

## Controller work needed

- `a2d lift validate <suite>` (above).
- `--jobs N`: run tasks concurrently. A k = 4, four-arm run is ~13 Luna calls per task at ~40 s each; sequential runs of 100 tasks would take ~15 h.

## Risks

- Codex authors tasks and Luna solves them (same vendor family): style familiarity may raise all arms equally; recorded, not corrected.
- The coder runs in Codex's read-only sandbox and could read files on disk. The ledger records `tool_calls` per invocation; any non-zero count on a scored run is investigated before its result is used.
