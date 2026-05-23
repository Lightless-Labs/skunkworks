# Architect Context Pyramid Summaries

**Created:** 2026-04-17
**Implemented:** 2026-04-23 — `format_system_code_snapshot` now emits Tier 0 purpose + Tier 1 signatures by default, with Tier 2 full source only for files named in `failure_report`; `A2D_ARCHITECT_FULL_CONTEXT=1` keeps the old full-context fallback.
**Priority:** #2 (after cycle iteration cap)

## Problem

The architect enzyme receives the full modifiable system source as
context. Measured 2026-04-17: 15 files, 195 968 bytes raw, ~400 KB after
prompt formatting. Observed consequences on live sudoku run:

- One architect call took 605 s (10m 5s) — past Gemini's advertised 5-min
  subprocess timeout.
- Another took 402 s, another 320 s.
- One call timed out entirely (262 s FAIL, likely bubble-up of a provider
  internal limit).

Architect latency is a significant share of cycle wall-clock time, and
`format_system_code_snapshot` (`metabolism.rs:1064-1071`) is a dumb
concat of every file.

## Fix

Replace `format_system_code_snapshot` with pyramid summaries:

- **Tier 0 (always):** one-line purpose per file. Roughly N × 120 bytes
  = ~2 KB for 15 files.
- **Tier 1 (always):** exported function/type signatures per file. Strip
  bodies, keep `fn name(args) -> Ret`, `pub struct X { fields }`,
  `pub enum Y { variants }`. Roughly 15–30 KB.
- **Tier 2 (targeted):** full source only for the file(s) named in the
  failure report. If the failure mentions `metabolism.rs`, include only
  `metabolism.rs` in full.

Target total prompt size: under 60 KB for typical failures, under 100 KB
worst case.

## Implementation sketch

1. Parse each modifiable file with a light lexer (or `syn` crate if
   already in deps — check first).
2. For each file, emit:
   ```
   === <path> ===
   PURPOSE: <doc comment or filename inference>
   SIGNATURES:
     fn foo(bar: Bar) -> Result<Baz, Err>
     pub struct Quux { ... }
     // ... elided bodies
   ```
3. If failure_report mentions a path substring, upgrade that file from
   Tier 1 to Tier 2 (full source).
4. Keep the existing `format_system_code_snapshot` as a fallback flag
   (`A2D_ARCHITECT_FULL_CONTEXT=1`) for debugging.

## Why one-line-per-file isn't enough

The architect needs to know *what to change*. One-line purpose tells it
which file to edit; signatures tell it the ambient types and callable
surface; full source of the failing file lets it propose a specific
patch. Without signatures of neighbors, patches that touch cross-file
APIs will break compilation and be rejected by self-sandbox — expensive.

## Validation

Unit coverage added for default pyramid summaries and failure-targeted full-source inclusion.

Live validation on 2026-04-23 (`A2D_TRACE=1 cargo run -p a2d -- challenge sudoku 1`):

- `system_code snapshot = 15 files, 17,702 bytes`
- Architect Gemini CLI prompt arg = 37,533 bytes
- Target prompt size met (<60 KB typical, <100 KB worst case)
- Latency target not met: first architect call still took 272.95 s and failed; a second architect call caused the run to exceed the 900 s harness timeout

Conclusion: pyramid summaries solved prompt size. The remaining bottleneck is Gemini provider latency/failure and repeated architect scheduling, not context volume.

## Not in scope

- Embedding-based similarity search on files (overkill).
- Caching summaries across cycles (the source changes between cycles
  when architect patches are accepted — stale cache would mislead).

## References

- `docs/solutions/architectural-insights/harness-engineering-patterns-to-investigate-2026-04-04.md`
- Measured 400 KB prompt size documented in
  `docs/solutions/architectural-insights/escalation-ladder-detects-but-doesnt-halt-degradation-2026-04-17.md`.
