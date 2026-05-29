# Test Core Selection Logic

**Created:** 2026-05-28
**Status:** Completed
**Completed:** 2026-05-28

## Context

Flux now has non-trivial provider-agnostic logic in `src/core/`:

- config discovery and deep merge,
- random trigger frequency/throttle defaults,
- loop-pattern matching,
- weighted prompt-profile selection,
- per-trigger model-pool resolution,
- bounded context formatting.

This should be tested without hitting any live LLM provider.

## Acceptance Criteria

- [x] Add a test harness for TypeScript core tests (`node --test`, `tsx`, Vitest, or similar; keep it lightweight).
- [x] Test `loadConfig` / deep merge behavior with partial config overrides.
- [x] Test `shouldFireTrigger` random defaults from `config.random`.
- [x] Test trigger-level `probability`, `minIntervalMs`, and `afterEvents` override `config.random`.
- [x] Test `loop-detected` pattern matching fires only when recent context matches.
- [x] Test `selectPromptProfile` resolves trigger name → kind → default and respects weights deterministically with injected RNG.
- [x] Test `resolveModelPool` resolves trigger name → kind → default and falls back to any usable configured model.
- [x] Test `formatSnapshotForPrompt` clamps context and includes recent users/assistants/tools.
- [x] `npm run check` and the test command pass.

## Notes

Avoid requiring real API keys. For model-pool tests, use fake `apiKey` literals or temporary environment variables.
