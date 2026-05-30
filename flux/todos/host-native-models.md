# Use Host-Native Model Access

**Created:** 2026-05-29
**Status:** In Progress

## Context

Flux should use the host agent's authenticated model path when running as a host integration:

- Pi extension: use Pi's selected model/auth via `ctx.modelRegistry` and `@earendil-works/pi-ai complete()`.
- Claude Code hook/plugin: use the authenticated `claude` CLI in print mode.
- Codex hook/plugin: use the authenticated `codex exec` CLI.
- Generic hook: keep Flux's direct provider model client as fallback.

Direct provider keys in `.flux/config.json` remain useful as a generic/fallback mode, but should not be required for host-native integrations.

## Acceptance Criteria

- [x] Add an injectable model-caller seam to core thought generation.
- [x] Pi adapter uses Pi's selected/authenticated model instead of requiring Flux-specific API keys.
- [x] Claude Code hook uses `claude` CLI with tools disabled and hook-recursion suppression.
- [x] Codex hook uses `codex exec` with read-only/ephemeral execution and hook-recursion suppression.
- [x] Generic hook still uses configured direct providers.
- [x] Add automated coverage for injected host-native model callers.
- [x] Smoke-test Pi host-native generation in non-interactive Pi JSON mode.
- [ ] Live-test Pi host-native generation in an interactive Pi session.
- [ ] Live-test Claude/Codex hook native CLI generation in real hook contexts.

## Notes

Host CLI callers set `FLUX_SUPPRESS=1` for child processes so nested host hook invocations do not recursively trigger Flux.

2026-05-30: Pi JSON-mode smoke confirmed `/flux think smoke test` emits a `flux:stray-thought` custom message with `model: pi/openai-codex/gpt-5.5`, and an agent-triggered `flux_stray_thought` call returns tool content plus an optional displayed custom message. Full interactive TUI validation remains open.
