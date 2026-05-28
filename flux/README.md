# Flux

Flux is a skunkworks "stray thought" sidecar for coding agents. It listens to agent lifecycle hooks, asks a second configurable LLM for a short creative nudge, then injects that nudge back into the active session as a **stray thought**.

Goal: help agents escape tunnel vision, retries, spirals, and over-committed plans without handing control to a full sub-agent.

## What it bootstraps

- **Pi extension**: `src/adapters/pi/index.ts`
- **Claude Code hook/plugin scaffold**: `src/adapters/claude-code/hook.ts`, `claude-code-plugin.json`, `examples/claude-code-settings.json`
- **Codex hook/plugin scaffold**: `src/adapters/codex/hook.ts`, `codex-plugin.json`, `examples/codex-config.toml`
- **Provider-agnostic core**: config, triggers, context capture, model calls, and thought logging

## Configuration

Copy the example config:

```bash
cp .flux/config.example.json .flux/config.json
```

Then set at least one API key referenced by the configured model pool:

```bash
export OPENAI_API_KEY=...
# or
export ANTHROPIC_API_KEY=...
```

Users can configure:

- model pool (`models[]`), including provider, model id, base URL, max tokens, temperature, and API key env var,
- random injections (`randomInjections`),
- delivery mode (`steer`, `followUp`, `nextTurn`, `stdout`, `file`),
- trigger list (`triggers[]`), probabilities, throttles, and loop patterns,
- context window limits.

## Pi usage

Run directly while developing:

```bash
pi -e ./src/adapters/pi/index.ts
```

Or install as a Pi package from this directory:

```bash
pi install ./flux
```

Pi commands/tools:

- `/flux status`
- `/flux on` / `/flux off`
- `/flux random on` / `/flux random off`
- `/flux think [reason]` force-injects a thought
- `flux_stray_thought` tool lets the agent request a thought
- Other Pi extensions can call `pi.events.emit("flux:trigger", { reason: "..." })`

## Claude Code / Codex hooks

Build first:

```bash
npm install
npm run build
```

Then wire the hook command in your host's hook settings. Example hook command:

```bash
node /absolute/path/to/flux/dist/src/adapters/claude-code/hook.js --host=claude-code
node /absolute/path/to/flux/dist/src/adapters/codex/hook.js --host=codex
```

See `examples/` for scaffold settings. Host plugin APIs move quickly, so these adapters intentionally expose a conservative hook CLI: read JSON on stdin, emit JSON with `additionalContext`/`instructions`, and never fail the host agent.

## Design note

I like your trigger-system instinct. It keeps "ADHD brain" whimsical without hard-coding randomness everywhere: random nudges become just one trigger among manual, external, loop-detected, tool-specific, or future signals.
