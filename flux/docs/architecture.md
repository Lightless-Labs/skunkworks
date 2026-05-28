# Flux Architecture

Flux is split into a small host-neutral core and thin host adapters.

## Core

- `config.ts` loads `.flux/config.json`, `flux.config.json`, or `~/.config/flux/config.json`.
- `triggers.ts` decides whether an observed event should generate a stray thought.
- `context.ts` builds bounded context snapshots from host session data.
- `modelClient.ts` calls a configured sidecar model pool (Anthropic or OpenAI-compatible APIs).
- `engine.ts` turns a snapshot into a logged `StrayThought`.

## Adapters

- `src/adapters/pi/index.ts` is a Pi package extension. It listens to `turn_end` and `tool_result`, registers `/flux`, registers the `flux_stray_thought` tool, and listens for `pi.events.emit("flux:trigger", payload)` from other extensions.
- `src/adapters/claude-code/hook.ts` is a command hook entrypoint. It expects hook JSON on stdin and returns JSON with `additionalContext`/`hookSpecificOutput`.
- `src/adapters/codex/hook.ts` uses the same hook CLI and returns generic instruction JSON for hook-capable Codex runtimes.

## Trigger model

Triggers are data-driven. The initial set contains:

1. `random-turn-end` — probabilistic, throttled creative interruptions.
2. `loop-language` — deterministic nudge when recent context smells stuck.
3. Manual/external triggers — `/flux think`, `flux_stray_thought`, CLI `--force`, and Pi event bus.

Future triggers should only need a new `TriggerKind`, a matcher in `triggers.ts`, and config entries.

## Context contract

The sidecar model receives:

- host name and cwd,
- session starting prompt when known,
- current system prompt when host exposes it,
- last N user messages,
- last N assistant responses,
- last N tool calls/results,
- no direct filesystem/shell tools.

The sidecar is intentionally tool-limited: Flux should nudge, not secretly act.
