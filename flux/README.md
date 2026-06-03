# Flux

Flux is a skunkworks "stray thought" sidecar for coding agents. It listens to agent lifecycle hooks, asks a second configurable LLM for a bounded note, then injects that note back into the active session as a **stray thought**.

Goal: help agents escape tunnel vision, retries, spirals, long-running dead ends, and over-committed plans without handing control to a full sub-agent.

## What it bootstraps

- **Pi extension**: `src/adapters/pi/index.ts`
- **Claude Code hook/plugin scaffold**: `src/adapters/claude-code/hook.ts`, `claude-code-plugin.json`, `examples/claude-code-settings.json`
- **Codex hook/plugin scaffold**: `src/adapters/codex/hook.ts`, `codex-plugin.json`, `examples/codex-config.toml`
- **Provider-agnostic core**: config, triggers, context capture, host-native/direct model calls, and thought logging

## Configuration

Copy the example config:

```bash
cp .flux/config.example.json .flux/config.json
```

When Flux runs as a Pi extension, Claude Code hook, or Codex hook, it uses the host's authenticated model path by default. For generic hooks or direct-provider fallback, set at least one API key referenced by the configured model pool:

```bash
export OPENAI_API_KEY=...
# or
export ANTHROPIC_API_KEY=...
```

Users can configure:

- model definitions (`models[]`), including provider, model id, base URL, max tokens, temperature, and API key env var,
- per-trigger model pools (`modelPools`), keyed by trigger name, trigger kind, or `default`,
- random injections (`randomInjections`) and random frequency (`random.probability`, `random.minIntervalMs`, `random.afterEvents`),
- Pi/session delivery mode (`steer`, `followUp`, `nextTurn`); hook integrations always write their host JSON response to stdout,
- trigger list (`triggers[]`), probabilities, throttles, loop patterns, and optional `modelPool` / `promptPool` names,
- per-trigger weighted prompt profile pools (`promptProfiles`),
- context window limits.

## Pi usage

Run directly while developing from `flux/`:

```bash
pi -e ./src/adapters/pi/index.ts
```

Install as a local Pi package from the skunkworks repo root:

```bash
pi install ./
```

Or install from git on another machine without manually cloning the repo:

```bash
pi install git:git@github.com:Lightless-Labs/skunkworks.git
```

The repo root exposes Flux through `extensions/flux.ts`, so git/local Pi installs do not require npm publishing.

Pi commands/tools:

- `/flux status`
- `/flux on` / `/flux off`
- `/flux random on` / `/flux random off`
- `/flux think [reason]` force-injects a thought
- `/flux config status` shows the resolved config and key settings
- `/flux config init` writes `.flux/config.json` from defaults
- `/flux config edit` opens the full JSON config in an editor and reloads it after save
- `/flux config set enabled true|false` persists Flux enablement
- `/flux config random on|off` persists random injection enablement
- `/flux config random probability 0.1` updates random frequency and persists it
- `/flux config random minIntervalMs 300000`
- `/flux config random afterEvents 3`
- `/flux config model cheap-openai-compatible openai-compatible gpt-4.1-mini apiKeyEnv=OPENAI_API_KEY` adds or updates a model definition
- `/flux config pool random cheap-openai-compatible,anthropic-haiku` assigns a model pool
- `/flux config prompt manual sharper-question 1 Ask one sharp question grounded in the session.` adds or updates a prompt profile
- `/flux config models` lists models and model pools
- `/flux config prompts` lists prompt profile pools with styles
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

## Prompt/model selection

Flux uses a neutral base system prompt plus trigger/profile-specific instructions. That means `random` can rotate between narrow local sparks, more global “inspiration hit me” notes, and playful reframes, while `loop-detected` can ask for kind-but-honest critical feedback about what the agent has been trying relative to the apparent task.

Model execution is host-native when possible:

- Pi extension: Pi selected model + Pi auth.
- Claude Code hook: `claude` CLI print mode.
- Codex hook: `codex exec` in read-only ephemeral mode.
- Generic hook/fallback: configured direct provider model pool.

Selection order for direct-provider fallback:

1. Model pool: trigger name → trigger kind → `default` → any usable model.
2. Prompt profile pool: trigger name → trigger kind → `default`.
3. Within a prompt pool, profiles are selected by `weight`.

## Design note

The trigger system keeps "ADHD brain" whimsical without hard-coding randomness everywhere: random nudges become just one trigger among manual, external, loop-detected, tool-specific, or future signals.
