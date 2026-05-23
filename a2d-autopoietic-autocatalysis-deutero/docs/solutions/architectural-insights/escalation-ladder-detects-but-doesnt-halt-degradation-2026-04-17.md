---
module: metabolism
tags: [escalation, degradation, ladder, benchmark, empirical]
problem_type: architectural-limitation
date: 2026-04-17
status: confirmed
---

# Escalation ladder detects degradation but doesn't halt it (live benchmark, cycle 1)

## Summary

First live benchmark of escalation rungs 0–3 on `a2d challenge sudoku 5` with
Kimi (coder) / Gemini (tester + architect) / GLM (evolver). The ladder
**mechanically fires every rung in order** but **doesn't produce
fitness-improving output**. Cycle 1 never terminated within ~80 minutes.

## What fired (in order, all within cycle 1)

- Rung 0 — loop detection: fired on byte-hash for evolver, architect; on
  fitness-signature for coder. Both variants work.
- Rung 1 — awareness injection: fired for all three dynamic enzymes
  (coder, evolver, architect).
- Rung 2 — alt-provider consultation: fired for coder and architect. The
  log message `rung 2+ consultation: asking gemini/gemini-3.1-pro-preview
  for advice on evolver` appeared even when the escalating enzyme was the
  coder — suspected log-line bug (asks alt-provider but names wrong enzyme).
- Rung 3 — clean session (strip `failure_report`): fired for coder and
  evolver.
- Rung 4 — reached by evolver. Not implemented; behaviour identical to
  rung 3. Ladder is climbing into unimplemented territory.

## What didn't happen

- Cycle 1 never completed. `Cycle 1/5...` was the only cycle header
  written to stdout in 80+ minutes. No fitness score reported externally.
- The coder and evolver kept producing output that either matched their
  previous byte-hash or produced the same fitness signature, even after
  awareness injection + alt-provider consultation + clean session.
- No enzyme reached a "green" state that would end the cycle's readiness
  loop.

## Why it spirals

`run_cycle` loops while any enzyme is ready. Readiness is refreshed after
each enzyme fires (outputs satisfy other enzymes' reactants). When
escalation techniques don't change enzyme output, rung climbing just
re-fires the same enzymes with slightly different prompts, producing
similar output, triggering detection again, climbing another rung. There's
no upper bound on rung or on iterations-per-cycle.

## Costs observed

- One architect call: 605s (10m5s). Exceeded Gemini's 5-min subprocess
  timeout but apparently still returned. Either the timeout isn't
  enforced, or the provider has asynchronous completion that races the
  timeout.
- Tester call: 504s (8m24s). Architect calls: 320–605s range.
- System-code snapshot passed to architect: 15 files, ~196 KB raw,
  ~400 KB after prompt formatting. Load-bearing — this is why architect
  calls are slow.

## Recommendations

1. **Cycle-level iteration cap.** Bound the number of enzyme firings per
   cycle. Once hit, force cycle increment even if enzymes are still
   "ready". Prevents indefinite spiral.
2. **Rung 4+ implementation (model swap) or explicit ceiling with
   fitness-freeze.** If rungs 0–3 are exhausted on an enzyme and fitness
   isn't improving, either escalate to rung 4 (swap the enzyme's provider
   for the next cycle) or freeze the germline and advance to the next
   cycle with what we have.
3. **Architect context pyramid summaries.** 400 KB prompts are eating
   wall-clock time and risk provider timeouts. One-line file summaries
   with full source only for the file targeted by the failure would
   likely cut this to <50 KB.
4. **Investigate the "asking X for advice on evolver" log when escalating
   on coder.** Either a trace-message bug or a logic bug in
   `alternative_provider_for`. Check `metabolism.rs:510` neighborhood.

## What HANDOFF claimed vs. what we saw

| HANDOFF claim | Observed |
|---|---|
| Rungs 0–3 implemented, tested | Confirmed — all fire mechanically |
| "The evolver produces no value. Zero mutations accepted across all runs." | Evolver *does* fire (multiple times per cycle), but its outputs keep matching prior byte-hash, triggering detection. Whether any mutations were accepted in this run: unknown — never reached the end-of-cycle report. |
| "Bottleneck is the cycle, not the model" | Strongly confirmed — adding escalation didn't fix output repetition; the cycle-level orchestration is the constraint. |
| Post-escalation benchmark NOT YET RUN | Now partially run. Cycle 1 didn't complete; next run needs cycle iteration cap first. |

## Next action

Before re-running the benchmark: add cycle-level iteration cap (~20–30
enzyme firings per cycle?). Without it every run will spiral.
