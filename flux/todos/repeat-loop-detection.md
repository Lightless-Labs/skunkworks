# Detect Behavioral Tool-Result Loops

**Created:** 2026-06-04
**Status:** Completed 2026-06-04

## Context

Flux's current `loop-detected` trigger is language-based: it scans recent user/assistant/tool text for configured patterns such as `stuck`, `retry`, or `same error`. This catches explicit loop smells, but it does not detect behavioral repetition, such as the agent running the same failing command or receiving the same tool error multiple times.

A Flux stray thought suggested adding rolling fingerprints of recent tool events (`name + normalized input + result/error hash`) to `FluxState`, then firing `loop-detected` when a repeated fingerprint crosses a threshold.

## Acceptance Criteria

- [x] Extend `FluxState` with a schema-safe rolling tool-result fingerprint history.
- [x] Normalize volatile text before fingerprinting so timestamps/UUIDs/numbers/temp paths do not prevent obvious repeat matches.
- [x] Add trigger config knobs for repeat-based loop detection, e.g. `repeatThreshold`, `repeatWindowEvents`, and `repeatRequireError`.
- [x] Preserve existing pattern-based loop detection.
- [x] Add tests for repeated tool-result detection and non-repeated/noisy cases.
- [x] Validate config fields for repeat loop detection.
- [x] Update `.flux/config.example.json`, `docs/architecture.md`, and `docs/HANDOFF.md`.
- [x] Run `npm run check` and `npm test`.

## Notes

Keep defaults conservative: repeated successful reads or ordinary status checks should not nudge too aggressively. A reasonable default is requiring three matching errored tool results within a small recent window.

## Result

Implemented in `src/core/triggers.ts` and enabled on the default `loop-language` trigger with `repeatThreshold=3`, `repeatWindowEvents=12`, and `repeatRequireError=true`. Fingerprinting preserves input numbers so distinct commands stay distinct, while normalizing volatile result numbers, timestamps, temp paths, UUIDs, and long hex strings. Pattern-based `loop-detected` matching still fires on non-tool events.

Note: `FluxState` is currently in-memory. Repeat-loop detection works for long-lived adapters such as the Pi extension; per-invocation hook CLIs will need persisted trigger state if repeat history should span separate Claude/Codex hook process launches.
