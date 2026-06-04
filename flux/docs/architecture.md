# Flux Architecture

Flux is split into a small host-neutral core and thin host adapters.

## Core

- `config.ts` loads `.flux/config.json`, `flux.config.json`, or `~/.config/flux/config.json`.
- `triggers.ts` decides whether an observed event should generate a stray thought.
- `context.ts` builds bounded context snapshots from host session data.
- `modelClient.ts` calls a configured direct-provider sidecar model pool (Anthropic or OpenAI-compatible APIs), selected by trigger name/kind when configured.
- `hostCliModelClient.ts` calls host CLIs for Claude Code and Codex hook integrations, with recursion suppression.
- `engine.ts` selects a weighted prompt profile, turns a snapshot into a logged `StrayThought`, and records the model/profile used. Adapters can inject a host-native model caller.

## Adapters

- `src/adapters/pi/index.ts` is a Pi package extension. It listens to `turn_end` and `tool_result`, registers `/flux`, registers the `flux_stray_thought` tool, listens for `pi.events.emit("flux:trigger", payload)` from other extensions, and generates thoughts through Pi's selected/authenticated model. It supports session delivery modes `steer`, `followUp`, and `nextTurn`.
- `src/adapters/claude-code/hook.ts` is a command hook entrypoint. It expects hook JSON on stdin, asks the authenticated `claude` CLI for the thought, and returns JSON with `additionalContext`/`hookSpecificOutput` on stdout.
- `src/adapters/codex/hook.ts` uses the same hook CLI, asks `codex exec` for the thought, and returns generic instruction JSON for hook-capable Codex runtimes.

## Host plugin installation

The skunkworks repo root exposes Flux as local/git marketplaces for Claude Code and Codex:

- `../.claude-plugin/marketplace.json` points Claude Code at `./flux`.
- `../.agents/plugins/marketplace.json` points Codex at `./flux`.
- `.claude-plugin/plugin.json` and `.codex-plugin/plugin.json` are the host-specific Flux plugin manifests.
- `hooks/claude-code-hooks.json` and `hooks/codex-hooks.json` call `scripts/flux-hook-wrapper.mjs` through host plugin-root placeholders.

The wrapper is deliberately operational rather than clever: if `dist/bin/flux-hook.js` is missing or stale, it installs only the hook build dependencies (`npm install --ignore-scripts --no-audit --no-fund --include=dev --omit=peer`) and runs `npm run build:hooks`. Any setup/runtime failure is converted to an exit-0 JSON response so hook installation cannot block the host agent.

## Trigger model

Triggers are data-driven. The initial set contains:

1. `random-turn-end` — probabilistic and throttled by `random.probability`, `random.minIntervalMs`, and `random.afterEvents` unless overridden on the trigger.
2. `loop-language` — deterministic nudge when recent context smells stuck; its default profile asks for kind but honest critical feedback against the apparent task/goal.
3. Manual/external triggers — `/flux think`, `flux_stray_thought`, CLI `--force`, and Pi event bus.

Future triggers should only need a new `TriggerKind`, a matcher in `triggers.ts`, and config entries.

## Prompt and model selection

The base system prompt is deliberately neutral: it says Flux is a bounded secondary note-writer, not that every note must be surprising or creative. Trigger-specific behavior lives in `promptProfiles`.

Resolution order:

- prompt pool: trigger name → trigger kind → `default`;
- model execution: host-native caller when the adapter provides one, otherwise direct-provider fallback;
- direct-provider fallback model pool: trigger name → trigger kind → `default` → any usable configured model.

Prompt profiles support `weight`, so a trigger can randomly rotate between several cognitive modes. For example, `random` can select between narrow local sparks, ambient/global inspiration, playful reframes, and left-field leaps, while `loop-detected` can select between critical feedback and smallest-next-check suggestions.

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
