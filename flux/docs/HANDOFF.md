# Flux Handoff — Read This First

**Last updated:** 2026-06-11
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
- `ee1816c Clarify Flux delivery semantics`
- `e490d2c Improve Flux config commands`
- `8e9bd2a Add Flux model config command`
- `b1eca30 Add Flux prompt config command`
- `5bac099 Test Flux direct model clients`
- `ac4371e Update Flux handoff latest commit`
- `fbeca67 Fix Flux Codex host CLI invocation`
- `bb4675b Update Flux host CLI handoff notes`
- `cab683f Expose Flux as repo-level Pi extension`
- `5874325 Document Flux git Pi install path`
- `c965d4b Add left-field Flux prompt profile`
- `ef546a1 Make Flux host hooks installable from git`
- `e815c42 Detect repeated Flux tool-result loops`
- `a86dae7 Document Flux loop detection blind spot`
- `cf3fa7c Document Flux sidecar model selection plan`
- `dce9662 Add Flux host sidecar model selection`

Implemented surfaces:

- **Pi extension:** `src/adapters/pi/index.ts`
  - Listens to `turn_end` and `tool_result`.
  - Registers custom message renderer for `flux:stray-thought`.
  - Registers `/flux` command.
  - Registers `flux_stray_thought` tool.
  - Listens for external extension trigger: `pi.events.emit("flux:trigger", payload)`.
  - Repo root exposes it through `extensions/flux.ts` so another machine can run `pi install git:git@github.com:Lightless-Labs/skunkworks.git` without manual cloning or npm publishing.
- **Claude Code hook/plugin:** `src/adapters/claude-code/hook.ts`, `.claude-plugin/plugin.json`, `hooks/claude-code-hooks.json`, `claude-code-plugin.json`, `examples/claude-code-settings.json`. The skunkworks repo root exposes a Claude marketplace at `../.claude-plugin/marketplace.json` so users can install from git without npm publishing or manual cloning.
- **Codex hook/plugin:** `src/adapters/codex/hook.ts`, `.codex-plugin/plugin.json`, `hooks/codex-hooks.json`, `codex-plugin.json`, `examples/codex-config.toml`. The skunkworks repo root exposes a Codex marketplace at `../.agents/plugins/marketplace.json` so users can install from git without npm publishing or manual cloning.
- **Core:** config loading, trigger matching, bounded context snapshots, prompt-profile selection, host-native sidecar preferences, per-trigger direct-provider model pools, Anthropic/OpenAI-compatible model calls, thought logging. `loop-detected` supports both language pattern matching and repeat-based tool-result fingerprints.
- **Core tests:** `test/core-selection.test.ts` covers config/deep merge, config command mutations/validation, host sidecar config, trigger frequency overrides, loop matching, prompt-profile/model-pool resolution, injected model callers, and context formatting/clamping. `test/hook-cli.test.ts` covers host hook event-kind inference, fixture snapshot extraction, and output shapes. `test/model-client.test.ts` covers non-network OpenAI-compatible and Anthropic request/response/error handling including direct-provider thinking effort. `test/host-cli-model-client.test.ts` covers non-network Claude/Codex host CLI argv/stdin construction and configured host sidecar model/effort flags. `test/plugin-install.test.ts` covers repo-root marketplace manifests, plugin hook wrapper commands, and wrapper safe-fail output.
- **Host-native model path in progress:** Pi adapter now calls Pi's selected/authenticated model by default and can select a configured `hostSidecar.pi.model` from Pi's harness registry plus clamped thinking effort. Claude/Codex hook CLI paths call their host CLIs with `FLUX_SUPPRESS=1` to avoid recursive hook triggering; Claude can receive a configured `--model` preference, while Codex can receive configured `-m` and `-c model_reasoning_effort=...`. Pi JSON-mode smoke for `/flux think` and `flux_stray_thought` passed on 2026-05-30. Local CLI surface check on 2026-05-31 used `claude` 2.1.119 and `codex-cli` 0.130.0; Codex requires `--ask-for-approval never` before the `exec` subcommand. See `todos/host-native-models.md`.
- **Delivery semantics clarified:** shared `DeliveryMode` is now only Pi/session message delivery (`steer`, `followUp`, `nextTurn`). Hook CLIs still emit host JSON on stdout as transport. Stale Pi configs using unsupported modes warn instead of silently mapping to `steer`. See `todos/delivery-semantics.md`.
- **Repo-installable host hooks:** Claude/Codex plugin hooks run through `scripts/flux-hook-wrapper.mjs`, which builds only hook code (`npm run build:hooks`) on first use if `dist/` is missing/stale. The wrapper always exits 0 and emits host-safe JSON on setup/runtime failure.
- **Dedicated sidecar model selection underway:** Flux now has `hostSidecar` config for harness-native sidecar model/thinking preferences across **all harnesses**. Pi uses its model registry for dynamic listing/selection; Codex model and reasoning-effort args are wired; Claude Code model args are wired but thinking flags still need live validation. The abstraction intentionally avoids hard-coding future model names such as Mythos/Fable. See `todos/host-sidecar-model-selection.md`.

Generated/ignored local artifacts:

- `dist/` exists after `npm run build` but is ignored by `flux/.gitignore`.
- `node_modules/` exists after `npm install --ignore-scripts` but is ignored.

## Verify State

Repo-level Pi package smoke from the skunkworks root:

```bash
pi --no-extensions -e . --no-session --mode json -p "/flux status"
```

Project-local install smoke from a temporary workspace:

```bash
pi install -l /Users/thomas/Projects/lightless-labs/skunkworks
pi --no-session --mode json -p "/flux status"
```

Install from git on another machine:

```bash
pi install git:git@github.com:Lightless-Labs/skunkworks.git
```

Run from `flux/`:

```bash
npm install --ignore-scripts
npm run check
npm test
npm run build
npm run build:hooks
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

Latest local verification on 2026-06-11: `npm run check` passed and `npm test` passed 32/32 tests.

Install Claude Code from git/local skunkworks root:

```bash
claude plugin marketplace add Lightless-Labs/skunkworks --sparse .claude-plugin flux
claude plugin install flux@lightless-labs-skunkworks
# local checkout alternative:
claude plugin marketplace add /Users/thomas/Projects/lightless-labs/skunkworks
```

Install Codex from git/local skunkworks root:

```bash
codex plugin marketplace add Lightless-Labs/skunkworks --sparse .agents/plugins/marketplace.json --sparse flux
codex plugin add flux@lightless-labs-skunkworks
# local checkout alternative:
codex plugin marketplace add /Users/thomas/Projects/lightless-labs/skunkworks
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
/flux config set enabled true|false
/flux config random on|off
/flux config random probability 0.1
/flux config random minIntervalMs 300000
/flux config random afterEvents 3
/flux config model model-a openai-compatible gpt-4.1-mini apiKeyEnv=OPENAI_API_KEY thinkingEffort=low
/flux config host models
/flux config host pi model active|provider/model-id
/flux config host pi thinking active|off|minimal|low|medium|high|xhigh
/flux config host codex model active|model-id
/flux config host codex thinking active|off|minimal|low|medium|high|xhigh
/flux config host claude-code model active|model-id
/flux config pool random model-a,model-b
/flux config prompt manual sharper-question 1 Ask one sharp question grounded in the session.
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
- Host-native sidecar model: `hostSidecar[host].model` / `thinkingEffort` when running inside a harness adapter; `active` means use the host/session default.
- Direct-provider model pool: trigger name → trigger kind → `default` → any usable configured model.

Current default `random` prompt pool:

- `local-spark` — narrow context-specific edge case / constraint / cheap validation / alternative hypothesis.
- `ambient-inspiration` — broader/global thought, strategic/aesthetic/cautionary/curiosity-driven, not necessarily an immediate action item.
- `weird-reframe` — playful but technically grounded reframe.
- `left-field-leap` — context-anchored left-field idea or suggestion, such as an unconventional analogy, inversion, adjacent-domain tactic, surprising simplification, or wild-but-cheap experiment.

Current default `loop-detected` pool:

- `kind-critical-feedback` — honest feedback comparing recent agent activity to apparent task/goal.
- `break-loop-smallest-check` — smallest concrete check/reproduction/state inspection to break repetition.

Default loop trigger behavior:

- Trigger name: `loop-language`.
- Trigger kind / prompt pool key: `loop-detected`.
- Language patterns still match recent user/assistant/tool text, e.g. `same error`, `stuck`, or `retry`.
- Repeat detection watches recent `tool-result` fingerprints and fires after 3 matching errored tool results within the last 12 tool results (`repeatThreshold=3`, `repeatWindowEvents=12`, `repeatRequireError=true`).
- Fingerprints preserve input numbers to avoid conflating distinct commands, but normalize volatile result numbers, timestamps, UUIDs, temp paths, and long hex strings.
- Repeat history is held in `FluxState`; this works for long-lived adapters like Pi. Per-invocation hook CLIs need persisted trigger state before repeat history can span separate Claude/Codex hook process launches.
- Known blind spot: language patterns and repeated tool fingerprints still only catch surface loops. They do not catch "wrong frame with local progress" loops where every step is novel and successful but aimed at the wrong problem. Random injections and the `left-field-leap` prompt profile are intentionally kept as the outside-channel mechanism for that class.

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

- Pi integration compiles and uses Pi-authenticated model access. Non-interactive JSON-mode smoke passed, including repo-level package loading and project-local install loading; full interactive TUI validation is still pending.
- Claude Code and Codex integrations now have host-CLI sidecar callers plus fixture/output-shape tests, but exact host-specific hook output contracts and real hook behavior still need validation against current host versions.
- Core selection/config/trigger/context logic, language/repeat loop detection, direct-provider HTTP clients, and host CLI argv construction now have automated coverage; host adapters still need focused/live tests.
- Loop detection remains heuristic: pattern matching and repeat fingerprints can miss wrong-frame-with-local-progress situations. Random/left-field nudges are the current mitigation.
- Direct-provider sidecar model calls support Anthropic and OpenAI-compatible chat completions only; thinking effort is best-effort/provider-compatible.
- Claude Code sidecar model selection emits `--model` when configured, but Claude Code thinking/effort CLI flags remain unvalidated and are not emitted yet.
- Config command UX now covers persistent enable/random toggles, random frequency, add/update model definitions with thinking effort, host sidecar model/thinking preferences, model-pool assignment, add/update prompt profiles, and full prompt-style listing.

## Best Next Moves

1. Finish live-testing the Pi extension in interactive TUI mode using Pi's host-native model path. See `todos/live-validate-pi-extension.md` and `todos/host-native-models.md`.
2. Finish validating Claude Code / Codex hook contracts against current docs and live hook contexts. Fixture/output-shape tests are in place. See `todos/host-hook-contracts.md`.
3. Live-validate host-native sidecar model selection: Pi registry selection/thinking clamp in TUI, Codex configured `-m`/reasoning effort, and Claude Code configured `--model`. See `todos/host-sidecar-model-selection.md`.
4. Continue validating host integrations against current Claude Code/Codex contracts and interactive Pi TUI behavior. See `todos/host-hook-contracts.md` and `todos/live-validate-pi-extension.md`.

## Important Files

| Path | Purpose |
|------|---------|
| `src/core/types.ts` | Shared config/event/snapshot/thought types |
| `src/core/config.ts` | Defaults, discovery, deep merge |
| `src/core/configActions.ts` | Reusable config mutation/validation helpers for `/flux config` |
| `src/core/delivery.ts` | Shared delivery mode validation helpers |
| `src/core/triggers.ts` | Trigger matching, random frequency/throttle logic, language/repeat loop detection |
| `src/core/engine.ts` | Prompt-profile selection, thought generation/logging |
| `src/core/modelClient.ts` | Anthropic/OpenAI-compatible direct-provider calls and model-pool resolution |
| `src/core/hostCliModelClient.ts` | Claude/Codex host CLI model callers for hook integrations |
| `src/core/context.ts` | Bounded host context snapshots |
| `test/core-selection.test.ts` | Node test coverage for core selection/config/trigger/context behavior |
| `test/hook-cli.test.ts` | Node test coverage for hook event inference, fixture snapshots, and host output shapes |
| `test/model-client.test.ts` | Non-network tests for direct-provider request/response/error handling |
| `test/host-cli-model-client.test.ts` | Non-network tests for Claude/Codex host CLI invocation argv/stdin |
| `test/fixtures/` | Representative Claude/Codex hook payload fixtures |
| `src/core/hookCli.ts` | Generic stdin/stdout hook runner for Claude Code/Codex/generic hooks |
| `todos/host-sidecar-model-selection.md` | Plan for configurable host-native sidecar model/thinking selection across Pi, Claude Code, and Codex |
| `src/adapters/pi/index.ts` | Pi extension |
| `../extensions/flux.ts` | Repo-level Pi package wrapper for git/local installs from skunkworks root |
| `.flux/config.example.json` | User-facing config template |
| `README.md` | User-facing usage summary |
| `docs/architecture.md` | Architecture details |
| `todos/` | Pending work items |
