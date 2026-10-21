---
title: "Spend-Once Instruments: What the Confirmation-3 Registration Review Found That Unit Tests Did Not"
date: 2026-10-21
category: best-practices
module: nano/confirm3.py, crates/a2d-providers, crates/a2d-lift, tools/holdout
problem_type: evaluation_integrity
component: confirmation_runner
symptoms:
  - "A Luna coder called codex's built-in list_mcp_resources on a 'no tools' provider (4 events, 0.89%: integrity-invalid)"
  - "Sol 'model at capacity' failures counted as tool events and made a phase-B run integrity-invalid"
  - "A Python port of the Rust detector split lines on U+2028 and would have counted phantom events"
  - "A malformed replicate summary was folded in as clean (fail-open on the integrity gate)"
  - "Two concurrent provider calls shared a cwd and CODEX_HOME (microsecond clock names)"
---

# Spend-Once Instruments: What the Confirmation-3 Registration Review Found

## Context
Confirmation-3 is a spend-once blind test: the runner (`nano/confirm3.py`) is bound by sha at the
registration commit, so any defect found after it is unrecoverable. One review plus four narrow
re-checks (Codex Sol and a fresh Claude, copies-only packets), four dev dress rehearsals and one
real-scale dev run (phase-B round 1) all found defects that 50+ unit tests had not.

## What we found, by source
- **Real-scale dev run (not a test, not a review):** codex 0.162's `code_mode_host` feature carries
  every MCP tool (built-in `list_mcp_resources`, configured servers) *and* shell execution. Disabling
  it removes them but makes codex emit one fixed `error` notice before the turn. At ~0.12–0.24% of
  calls × 2–4 counted events, the "harmless" listings would have decided IntegrityReview about half
  the time. Fix: disable the host; exempt exactly one byte-identical pre-turn notice; *require* it
  on every call (proof the host was off).
- **Counts-only look at audit ledgers:** 4 "no information" voids were pairs of calls whose cwd
  names collided (pid + microsecond clock; `create_dir_all`). A re-audit had hidden it.
- **Reviews:** fail-open paths in the bound runner — a malformed replicate summary folded in as
  clean; a failed attempt's in-flight calls invisible (now a durable `invocation_started` record);
  `str.splitlines()` vs Rust `str::lines()` (U+2028/U+2029/U+0085 stay raw inside serde JSON strings);
  NaN accepted by `json.loads`; preconditions checked only after the attempt was reserved.
- **Rehearsals with the real binary and exact flags:** proved the command line, arm labels,
  invocation counts (judge-lane calls are inside the claimed counts) and, in the last one, a real
  Sol capacity failure failing closed.

## Practice
- **Run the instrument at scale on dev before registering it.** A 450-call dev round found what
  four review rounds and every probe missed. Rehearse the bound runner end to end with the real
  binary and exact flags; re-pin the runner's sha in the rehearsal.
- **A port of a detector must be byte-faithful**, including line splitting and JSON strictness, and
  should be max-ed with the original count, never substituted for it.
- **Every malformed-input path must fail closed *through the real call chain*.** A unit test that
  feeds the helper a raw list missed a sentinel dict produced one function earlier.
- **Classify "no information" voids by fixed labels before re-running them.**
- **Instrument errors counted as tool events are a real risk.** Measure their rate by lane and hour;
  mitigate operationally (pre-launch API health probe; hold other work during windows) when the
  rule itself is frozen.
- **Give the review loop a written stopping rule** (fix only what can bias toward the claim or break
  the runner on real data; disposition and disclose the rest), or it hardens forever.
