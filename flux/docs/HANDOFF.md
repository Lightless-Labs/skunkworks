# Flux Handoff — Read This First

**Last updated:** 2026-05-28
**Update this file:** before context compaction, at session end, or when significant state changes.

## What Is This

Flux is a skunkworks sidecar for coding agents. It listens to host-agent hooks/events, captures a bounded session snapshot, asks a configurable secondary LLM for a short trigger-specific note, and injects that note into the active agent session as a **stray thought**.

The intent is not to run a hidden subagent. Flux nudges: it can be narrow and local, broad/global like an inspiration hit, or kind-but-honest critical feedback when the main agent appears stuck.

## Current State

Repository path: `/Users/thomas/Projects/lightless-labs/skunkworks/flux`

Latest Flux commits on `main`:

- `018f009 Bootstrap flux agent sidecar`
- `7ade4c1 Add trigger-specific Flux prompt profiles`
- `c86bc32 Add Flux configuration command`
- `b2f5f50 Broaden random Flux prompt pool`
- `9c6e928 Add Flux core selection tests`
- `fa0e09b Use host-native Flux model callers`

Implemented surfaces:

- **Pi extension:** `src/adapters/pi/index.ts`
  - Listens to `turn_end` and `tool_result`.
  - Registers custom message renderer for `flux:stray-thought`.
  - Registers `/flux` command.
  - Registers `flux_stray_thought` tool.
  - Listens for external extension trigger: `pi.events.emit("flux:trigger", payload)`.
- **Claude Code hook scaffold:** `src/adapters/claude-code/hook.ts`, `claude-code-plugin.json`, `examples/claude-code-settings.json`.
- **Codex hook scaffold:** `src/adapters/codex/hook.ts`, `codex-plugin.json`, `examples/codex-config.toml`.
- **Core:** config loading, trigger matching, bounded context snapshots, prompt-profile selection, per-trigger model pools, Anthropic/OpenAI-compatible model calls, thought logging.
- **Core tests:** `test/core-selection.test.ts` covers config/deep merge, trigger frequency overrides, loop matching, prompt-profile/model-pool resolution, injected model callers, and context formatting/clamping.
- **Host-native model path in progress:** Pi adapter now calls Pi's selected/authenticated model; Claude/Codex hook CLI paths call their host CLIs with `FLUX_SUPPRESS=1` to avoid recursive hook triggering. See `todos/host-native-models.md`.

Generated/ignored local artifacts:

- `dist/` exists after `npm run build` but is ignored by `flux/.gitignore`.
- `node_modules/` exists after `npm install --ignore-scripts` but is ignored.

## Verify State

Run from `flux/`:

```bash
npm install --ignore-scripts
npm run check
npm test
npm run build
node dist/bin/flux-hook.js --host=generic <<'JSON'
{"event":"turn_end","messages":[{"role":"user","content":"hello"},{"role":"assistant","content":"hi"}]}
JSON
```

Expected hook smoke output when no trigger/model fires:

```json
{"continue":true,"flux":{"fired":false}}
```

Forced hook smoke without API keys should fail safely, not fail the host:

```bash
node dist/bin/flux-hook.js --host=generic --force <<'JSON'
{"event":"external","messages":[{"role":"user","content":"hello"}]}
JSON
```

Expected shape:

```json
{"continue":true,"flux":{"error":"No usable Flux sidecar models. Configure .flux/config.json models with apiKeyEnv/apiKey."}}
```

## Configuration Model

Config discovery order:

1. `FLUX_CONFIG=/path/to/config.json`
2. `<cwd>/.flux/config.json`
3. `<cwd>/flux.config.json`
4. `~/.config/flux/config.json`

Example config: `.flux/config.example.json`.

Runtime Pi config commands:

```text
/flux status
/flux on
/flux off
/flux random on
/flux random off
/flux think [reason]
/flux reload
/flux config status
/flux config init
/flux config edit
/flux config random probability 0.1
/flux config random minIntervalMs 300000
/flux config random afterEvents 3
/flux config models
/flux config prompts
```

Persistent behavior is JSON-configured. `/flux on|off|random on|off` are session/runtime toggles; `/flux config ...` writes or edits `.flux/config.json`.

## Prompt and Model Selection

Flux deliberately uses a neutral base system prompt:

> You are Flux, a secondary model asked to add a bounded note to a coding-agent session. Follow the selected trigger/profile instructions. Use only the supplied context snapshot. Do not pretend to have acted in the workspace. Do not call tools. Output only the requested note.

Trigger-specific behavior lives in weighted `promptProfiles`.

Resolution order:

- Prompt pool: trigger name → trigger kind → `default`.
- Model pool: trigger name → trigger kind → `default` → any usable configured model.

Current default `random` prompt pool:

- `local-spark` — narrow context-specific edge case / constraint / cheap validation / alternative hypothesis.
- `ambient-inspiration` — broader/global thought, strategic/aesthetic/cautionary/curiosity-driven, not necessarily an immediate action item.
- `weird-reframe` — playful but technically grounded reframe.

Current default `loop-detected` pool:

- `kind-critical-feedback` — honest feedback comparing recent agent activity to apparent task/goal.
- `break-loop-smallest-check` — smallest concrete check/reproduction/state inspection to break repetition.

Random injection frequency is controlled by:

```json
"random": {
  "probability": 0.18,
  "minIntervalMs": 180000,
  "afterEvents": 2
}
```

A `random` trigger can override these with its own `probability`, `minIntervalMs`, or `afterEvents`.

## Current Limitations / Unvalidated Areas

- Pi integration compiles and uses Pi-authenticated model access, but has not been live-tested inside an interactive Pi session.
- Claude Code and Codex integrations now have host-CLI sidecar callers, but host-specific hook output contracts and real hook behavior still need validation against current host versions.
- Core selection/config/trigger/context logic now has automated coverage, but provider HTTP clients and host adapters still need focused tests.
- Sidecar model calls support Anthropic and OpenAI-compatible chat completions only.
- `thinkingEffort` is typed in config but not sent to providers yet.
- `delivery: "stdout" | "file"` exists in the type but Pi delivery currently maps non-followUp/non-nextTurn to `steer`; hook CLI writes JSON to stdout. Clarify/implement file delivery before documenting it as complete.
- Config command UX is JSON-editor based. It is functional but not friendly for model/prompt-pool edits beyond full JSON editing.

## Best Next Moves

1. Live-test the Pi extension in interactive Pi using Pi's host-native model path. See `todos/live-validate-pi-extension.md` and `todos/host-native-models.md`.
2. Validate and harden Claude Code / Codex hook contracts and host-CLI generation in real hook contexts. See `todos/host-hook-contracts.md`.
3. Improve `/flux config` UX for adding/removing models and prompt profiles without hand-editing full JSON. See `todos/config-command-ux.md`.
4. Decide whether `stdout`/`file` delivery are real cross-host features or should be removed from the shared type until implemented.

## Important Files

| Path | Purpose |
|------|---------|
| `src/core/types.ts` | Shared config/event/snapshot/thought types |
| `src/core/config.ts` | Defaults, discovery, deep merge |
| `src/core/triggers.ts` | Trigger matching, random frequency/throttle logic |
| `src/core/engine.ts` | Prompt-profile selection, thought generation/logging |
| `src/core/modelClient.ts` | Anthropic/OpenAI-compatible direct-provider calls and model-pool resolution |
| `src/core/hostCliModelClient.ts` | Claude/Codex host CLI model callers for hook integrations |
| `src/core/context.ts` | Bounded host context snapshots |
| `test/core-selection.test.ts` | Node test coverage for core selection/config/trigger/context behavior |
| `src/core/hookCli.ts` | Generic stdin/stdout hook runner for Claude Code/Codex/generic hooks |
| `src/adapters/pi/index.ts` | Pi extension |
| `.flux/config.example.json` | User-facing config template |
| `README.md` | User-facing usage summary |
| `docs/architecture.md` | Architecture details |
| `todos/` | Pending work items |
