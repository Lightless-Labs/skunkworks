# Validate Claude Code and Codex Hook Contracts

**Created:** 2026-05-28
**Status:** Open

## Context

Flux includes hook CLI adapters for Claude Code and Codex:

- `src/adapters/claude-code/hook.ts`
- `src/adapters/codex/hook.ts`
- `src/core/hookCli.ts`

They currently implement a conservative contract: read JSON from stdin, infer event kind, and print JSON containing `additionalContext`/`instructions` plus Flux metadata. This needs validation against current host plugin/hook APIs.

## Acceptance Criteria

- [ ] Check current Claude Code hook/plugin docs for exact accepted stdout schema for injecting context on stop/post-tool hooks.
- [ ] Check current Codex plugin/hook docs for exact accepted stdout schema and lifecycle event names.
- [ ] Update `examples/claude-code-settings.json`, `claude-code-plugin.json`, `examples/codex-config.toml`, and `codex-plugin.json` to match real schemas.
- [ ] Add fixture JSON payloads for representative host events.
- [ ] Add non-network smoke tests for `runHookCli` payload parsing/event-kind inference/output shape.
- [ ] Live-smoke at least one host if available locally.
- [ ] Document known host-version assumptions in `docs/HANDOFF.md` or host-specific docs.

## Notes

Do not let hook failures break the host agent. The current behavior of emitting `{ "continue": true, "flux": { "error": ... } }` on errors should be preserved or mapped to the host's safe-continue equivalent.
