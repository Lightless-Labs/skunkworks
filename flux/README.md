# Flux

Flux is a skunkworks "stray thought" sidecar for coding agents. It listens to agent lifecycle hooks, asks a second configurable LLM for a bounded note, then injects that note back into the active session as a **stray thought**.

Goal: help agents escape tunnel vision, retries, spirals, long-running dead ends, and over-committed plans without handing control to a full sub-agent.

## What it bootstraps

- **Pi extension**: `src/adapters/pi/index.ts`
- **Claude Code hook/plugin integration**: `src/adapters/claude-code/hook.ts`, `.claude-plugin/plugin.json`, `hooks/claude-code-hooks.json`, `claude-code-plugin.json`, `examples/claude-code-settings.json`
- **Codex hook/plugin integration**: `src/adapters/codex/hook.ts`, `.codex-plugin/plugin.json`, `hooks/codex-hooks.json`, `codex-plugin.json`, `examples/codex-config.toml`
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

- direct-provider model definitions (`models[]`), including provider, model id, base URL, max tokens, temperature, optional thinking effort, and API key env var,
- host-native sidecar model preferences (`hostSidecar`) for harness-backed calls, with `active` defaults and optional model/thinking selections where a host supports them,
- per-trigger model pools (`modelPools`), keyed by trigger name, trigger kind, or `default`,
- random injections (`randomInjections`) and random frequency (`random.probability`, `random.minIntervalMs`, `random.afterEvents`),
- Pi/session delivery mode (`steer`, `followUp`, `nextTurn`); hook integrations always write their host JSON response to stdout,
- trigger list (`triggers[]`), probabilities, throttles, loop patterns, repeat-loop settings (`repeatThreshold`, `repeatWindowEvents`, `repeatRequireError`), and optional `modelPool` / `promptPool` names,
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

- `/flux status` shows config state plus Pi configured-vs-resolved sidecar model/thinking when available
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
- `/flux config model cheap-openai-compatible openai-compatible gpt-4.1-mini apiKeyEnv=OPENAI_API_KEY thinkingEffort=low` adds or updates a direct-provider model definition
- `/flux config host models` lists available harness models where the host exposes a model registry, currently Pi
- `/flux config host pi model active|provider/model-id` selects the Pi host-native sidecar model
- `/flux config host pi thinking active|off|minimal|low|medium|high|xhigh` selects Pi sidecar thinking effort, clamped to model capability
- `/flux config host codex model active|model-id` and `/flux config host codex thinking active|off|minimal|low|medium|high|xhigh` pass Codex CLI sidecar preferences
- `/flux config host claude-code model active|model-id` and `/flux config host claude-code thinking active|off|minimal|low|medium|high|xhigh` pass Claude Code CLI sidecar preferences (`minimal` is clamped to Claude's lowest supported `low` effort)
- `/flux config pool random cheap-openai-compatible,anthropic-haiku` assigns a model pool
- `/flux config prompt manual sharper-question 1 Ask one sharp question grounded in the session.` adds or updates a prompt profile
- `/flux config models` lists models and model pools
- `/flux config prompts` lists prompt profile pools with styles
- `flux_stray_thought` tool lets the agent request a thought
- Other Pi extensions can call `pi.events.emit("flux:trigger", { reason: "..." })`

## Claude Code / Codex hooks

Flux can be installed directly from the git-managed skunkworks repo; no npm publish and no manual clone are required. The repo root is a small Claude/Codex marketplace that points at `./flux`.

Claude Code:

```bash
claude plugin marketplace add Lightless-Labs/skunkworks --sparse .claude-plugin flux
claude plugin install flux@lightless-labs-skunkworks
```

Codex:

```bash
codex plugin marketplace add Lightless-Labs/skunkworks --sparse .agents/plugins/marketplace.json --sparse flux
codex plugin add flux@lightless-labs-skunkworks
```

For local development from an existing skunkworks checkout, run the same marketplace add commands with the repo root path instead:

```bash
claude plugin marketplace add /absolute/path/to/skunkworks
codex plugin marketplace add /absolute/path/to/skunkworks
```

The installed hooks call `scripts/flux-hook-wrapper.mjs`. On first use it runs `npm install --ignore-scripts --no-audit --no-fund --include=dev --omit=peer` and `npm run build:hooks` inside the installed Flux copy if `dist/` is missing or stale, then invokes the compiled hook CLI. If setup or Flux itself fails, the wrapper still exits 0 and emits host-safe JSON so the host agent continues.

To avoid first-hook setup latency while developing locally, build first:

```bash
npm install
npm run build
```

Manual hook wiring is still possible with the wrapper command:

```bash
node /absolute/path/to/flux/scripts/flux-hook-wrapper.mjs --host=claude-code
node /absolute/path/to/flux/scripts/flux-hook-wrapper.mjs --host=codex
```

See `examples/` for scaffold settings. Host plugin APIs move quickly, so these adapters intentionally expose a conservative hook CLI: read JSON on stdin, emit documented hook JSON with `hookSpecificOutput.additionalContext`, and never fail the host agent.

## Prompt/model selection

Flux uses a neutral base system prompt plus trigger/profile-specific instructions. That means `random` can rotate between narrow local sparks, more global “inspiration hit me” notes, playful reframes, and left-field leaps, while `loop-detected` can ask for kind-but-honest critical feedback when explicit loop language or repeated errored tool-result fingerprints suggest the agent is stuck.

Model execution is host-native when possible:

- Pi extension: Pi selected model + Pi auth by default, or a configured `hostSidecar.pi.model` from Pi's available model registry. `/flux status` shows configured vs resolved Pi sidecar model/thinking; stale configured Pi model pins warn and fall back to the active model.
- Claude Code hook: `claude` CLI print mode, optionally with configured sidecar model and `--effort` for configured thinking. Claude currently accepts `low`, `medium`, `high`, and `xhigh`; Flux maps the cross-host `minimal` setting to `low`. If a configured model invocation fails, Flux retries once with the active/default host model and records a warning in thought metadata.
- Codex hook: `codex exec` in read-only ephemeral mode, optionally with configured sidecar model and `model_reasoning_effort`. If a configured model invocation fails, Flux retries once with the active/default host model and records a warning in thought metadata.
- Generic hook/fallback: configured direct provider model pool.

Flux deliberately does not maintain provider `latest` alias mappings. Defaults delegate to the host active/default model path; explicit pins or host-supported patterns are user-owned configuration.

Selection order for direct-provider fallback:

1. Model pool: trigger name → trigger kind → `default` → any usable model.
2. Prompt profile pool: trigger name → trigger kind → `default`.
3. Within a prompt pool, profiles are selected by `weight`.

## Design note

The trigger system keeps "ADHD brain" whimsical without hard-coding randomness everywhere: random nudges become just one trigger among manual, external, loop-detected, tool-specific, or future signals.
