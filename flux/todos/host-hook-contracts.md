# Validate Claude Code and Codex Hook Contracts

**Created:** 2026-05-28
**Status:** In Progress

## Context

Flux includes hook CLI adapters for Claude Code and Codex:

- `src/adapters/claude-code/hook.ts`
- `src/adapters/codex/hook.ts`
- `src/core/hookCli.ts`

They currently implement a conservative contract: read JSON from stdin, infer event kind, and print JSON containing documented `hookSpecificOutput.additionalContext` plus Flux metadata. Docs-level schema validation is complete; live host-session smoke validation remains open.

## Acceptance Criteria

- [x] Check current Claude Code hook/plugin docs for exact accepted stdout schema for injecting context on stop/post-tool hooks.
- [x] Check current Codex plugin/hook docs for exact accepted stdout schema and lifecycle event names.
- [x] Update `examples/claude-code-settings.json`, `claude-code-plugin.json`, `examples/codex-config.toml`, and `codex-plugin.json` to match real schemas.
- [x] Add fixture JSON payloads for representative host events.
- [x] Add non-network smoke tests for `runHookCli` payload parsing/event-kind inference/output shape.
- [ ] Live-smoke at least one host if available locally.
- [x] Document known host-version assumptions in `docs/HANDOFF.md` or host-specific docs.

## Notes

Do not let hook failures break the host agent. The current behavior of emitting `{ "continue": true, "flux": { "error": ... } }` on errors should be preserved or mapped to the host's safe-continue equivalent.

2026-05-30: Added representative fixture payloads under `test/fixtures/` for Claude `Stop`, Claude `PostToolUse`, Codex `post_turn`, and Codex `post_tool`; added non-network tests for event-kind inference, snapshot extraction, and host output shapes. Current docs/schema validation was completed later on 2026-06-17; real hook-context live smokes remain open.

2026-05-31: Checked local CLI surfaces (`claude` 2.1.119, `codex-cli` 0.130.0) for host-native sidecar invocation. Claude print-mode flags still match. Codex approval policy must be passed before the `exec` subcommand; fixed the caller and covered argv construction in tests. Current stdout hook response docs were checked later on 2026-06-17; live hook validation remains open.

2026-06-17: Checked current public Claude Code hooks/plugins docs and OpenAI Codex hooks/plugins/config docs. Claude Code and Codex both support nested `hookSpecificOutput.additionalContext`; Stop outputs must use `hookEventName: "Stop"`, and PostToolUse outputs must use `hookEventName: "PostToolUse"`. Codex docs list lifecycle events including `PreToolUse`, `PermissionRequest`, `PostToolUse`, `PreCompact`, `PostCompact`, `UserPromptSubmit`, `SubagentStop`, `Stop`, `SessionStart`, and `SubagentStart`; installed plugins can bundle lifecycle config through a manifest or default `hooks/hooks.json`. Updated Flux hook output to emit documented `hookSpecificOutput.additionalContext` for both Claude and Codex, with fixture smoke coverage for Stop. Updated `examples/codex-config.toml` to use Codex's nested matcher-group TOML shape. Local `claude --version` / `codex --version` probes timed out in this shell, but installed paths exist (`/opt/homebrew/bin/claude`, `/opt/homebrew/bin/codex`). A full `npm test` can take >3 minutes here because TypeScript module resolution is I/O-heavy; use a long timeout rather than treating 180s as a test hang.
