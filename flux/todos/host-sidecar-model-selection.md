# Host-Native Sidecar Model Selection

**Created:** 2026-06-05
**Status:** In Progress
**Enhanced:** 2026-06-11 — initial hostSidecar implementation landed for Pi/Codex/Claude model selection paths plus direct-provider thinking effort.
**Enhanced:** 2026-06-15 — documented the model-release invariant: Flux delegates latest/default selection to the host and must not own provider alias maps.
**Enhanced:** 2026-06-15 — Pi `/flux status` now shows configured vs resolved sidecar model/thinking, and stale/unavailable Pi pins warn and fall back to the active model.
**Enhanced:** 2026-06-15 — Claude/Codex host CLI sidecars now retry once with the active host model when an explicit configured model invocation fails, and propagate a warning in thought metadata.
**Enhanced:** 2026-06-18 — validated Claude Code 2.1.181 exposes `--effort <low|medium|high|xhigh|max>` and wired configured Flux Claude thinking through `--effort`, mapping cross-host `minimal` to Claude `low`.
**Enhanced:** 2026-06-18 — `/flux config models` now formats host sidecar preferences with effective Claude/Codex CLI args; true alias/default resolution remains host-owned because the CLIs do not expose enough resolved-model metadata for Flux to maintain safely.
**Enhanced:** 2026-06-18 — Codex real CLI smoke passed for `thinkingEffort=off`; Claude real generation remains blocked by local account/model policy, so `--safe-mode`/`--effort` are covered by help output, status visibility, and argv tests rather than successful live generation.

## Context

Flux currently supports host-native model access for Pi, Claude Code, and Codex, but the host-native callers mostly use the host's active/default model:

- Pi uses `ctx.model` and Pi auth through `ctx.modelRegistry`.
- Claude Code uses `claude -p` with host auth.
- Codex uses `codex exec` with host auth.

Users should be able to choose a dedicated Flux sidecar model and thinking/reasoning effort when the harness exposes that capability, without hard-coding particular model names such as Mythos/Fable. If a model is available through the harness model registry or host CLI, Flux should be able to discover/list/select it.

Invariant: **Flux must never own provider “latest” alias resolution.** Defaults delegate to the host active/default model. Explicit model pins or host-supported patterns are user-owned choices. Flux should surface configured vs resolved model information and warn clearly when a configured pin is stale or unavailable, falling back safely rather than silently chasing provider release names.

## Desired Behavior

- Keep current behavior as default: use the active/default host model so Flux inherits new model releases through the harness rather than internal aliases.
- Add an explicit host-native sidecar model selection layer, separate from direct-provider `models[]`.
- Let users list available host models where the harness exposes them.
- Let users choose a Flux sidecar model by provider/model id or host-supported pattern where supported; treat explicit pins/patterns as user-owned configuration.
- Never maintain an internal provider "latest" alias map in Flux.
- Let users choose sidecar thinking effort/level where supported.
- Clamp/validate thinking effort against host/model capabilities when the harness exposes capability metadata.
- Fall back safely with a clear warning when a host does not support model/effort selection or a configured model is stale/unavailable.
- Expose/log configured vs resolved sidecar model where possible, including `/flux status`, `/flux config models`, and thought metadata/logging.

## Candidate UX

Host commands:

```text
/flux config host models
/flux config host pi model active|<provider>/<model-id>
/flux config host pi thinking active|off|minimal|low|medium|high|xhigh
/flux config host codex model active|<model-id>
/flux config host codex thinking active|off|minimal|low|medium|high|xhigh
/flux config host claude-code model active|<model-id>
/flux config host claude-code thinking active|off|minimal|low|medium|high|xhigh
```

Direct-provider command should also grow the already-typed `thinkingEffort` option:

```text
/flux config model <name> <openai-compatible|anthropic> <model-id> thinkingEffort=medium
```

## Implementation Notes

- `FluxModelSpec.thinkingEffort` exists and is now exposed through config commands. OpenAI-compatible direct calls send `reasoning_effort`; Anthropic direct calls send budget-based `thinking` and raise `max_tokens` to fit the budget.
- Pi's actual registry API exposes `ctx.modelRegistry.getAll()`, `getAvailable()`, `find(provider, modelId)`, `hasConfiguredAuth(model)`, and `getApiKeyAndHeaders(model)`. Models carry `provider`, `id`, `name`, `api`, `reasoning`, `thinkingLevelMap`, `contextWindow`, and `maxTokens` metadata.
- Pi exposes active thinking controls through `pi.getThinkingLevel()` / `pi.setThinkingLevel()`. Flux avoids mutating the active session and passes `reasoning` to the sidecar call when a non-`active`/non-`off` Pi sidecar thinking level is configured, clamped against model metadata.
- Pi sidecar generation should not permanently change the user's active model/thinking level just to generate a Flux thought. Prefer passing the chosen model/effort directly into the sidecar `complete()` call if the API supports it; otherwise document the limitation.
- Claude Code configured model selection is wired through `--model`, and configured thinking/effort is wired through `--effort` after local CLI validation (`claude` 2.1.181). Claude's CLI does not accept Flux's cross-host `minimal` label, so Flux maps `minimal` to `low`; Flux does not expose Claude's `max` value in the shared cross-host config. Codex configured model and effort are wired through `-m` and `-c model_reasoning_effort=...`. If a configured Claude/Codex model CLI invocation fails, Flux retries once with the active/default host model and records a warning in thought metadata.
- Avoid hard-coded model names. Mythos/Fable-style models should work if present in the harness registry or CLI model list.
- Do not add provider-release aliases such as `latest-fast` → concrete model id inside Flux. If a host or provider exposes aliases/patterns, Flux may pass through user configuration, but the host/provider remains the source of truth.

## Acceptance Criteria

- [x] Add config shape for host-native sidecar model selection and thinking effort.
- [x] Add validation and `/flux config host ...` commands.
- [x] Pi: list available harness models and select a sidecar model if supported.
- [x] Pi: apply/clamp sidecar thinking level if supported without mutating the user's active session settings.
- [x] Direct providers: parse and send `thinkingEffort` where provider-compatible.
- [x] Claude Code: validate and wire thinking/effort CLI flags if current Claude Code supports them. Model selection is wired through `--model`; thinking is wired through `--effort` with `minimal` mapped to `low`.
- [x] Codex: validate and wire model/effort CLI flags if current Codex supports them.
- [x] Add tests for config parsing/validation and host caller argv/options.
- [x] Document host-specific support/limitations in `docs/HANDOFF.md` and `README.md`.
- [ ] Live-smoke Pi/Codex/Claude configured sidecar selections in real harness contexts. Codex `thinkingEffort=off` real CLI smoke passed on 2026-06-18; Claude still needs an environment with working Claude Code auth/API-key policy.
- [x] Pi: show configured vs resolved host sidecar model/thinking in `/flux status`.
- [x] Pi: warn and fall back safely when an explicit host sidecar model pin is stale or unavailable.
- [x] Codex/Claude hook sidecars: retry with active/default host model and propagate a warning when an explicit configured model invocation fails.
- [x] Extend configured-vs-resolved status visibility to Codex/Claude where their CLIs expose enough information. Flux now shows configured preferences and effective CLI args in `/flux config models`; actual alias/default resolution stays with the host CLIs because they do not expose stable resolved-model metadata here.
