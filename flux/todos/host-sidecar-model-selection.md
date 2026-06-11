# Host-Native Sidecar Model Selection

**Created:** 2026-06-05
**Status:** In Progress
**Enhanced:** 2026-06-11 — initial hostSidecar implementation landed for Pi/Codex/Claude model selection paths plus direct-provider thinking effort.

## Context

Flux currently supports host-native model access for Pi, Claude Code, and Codex, but the host-native callers mostly use the host's active/default model:

- Pi uses `ctx.model` and Pi auth through `ctx.modelRegistry`.
- Claude Code uses `claude -p` with host auth.
- Codex uses `codex exec` with host auth.

Users should be able to choose a dedicated Flux sidecar model and thinking/reasoning effort when the harness exposes that capability, without hard-coding particular model names such as Mythos/Fable. If a model is available through the harness model registry or host CLI, Flux should be able to discover/list/select it.

## Desired Behavior

- Keep current behavior as default: use the active host model.
- Add an explicit host-native sidecar model selection layer, separate from direct-provider `models[]`.
- Let users list available host models where the harness exposes them.
- Let users choose a Flux sidecar model by provider/model id or pattern where supported.
- Let users choose sidecar thinking effort/level where supported.
- Clamp/validate thinking effort against host/model capabilities when the harness exposes capability metadata.
- Fall back safely with a clear warning when a host does not support model/effort selection.

## Candidate UX

Host commands:

```text
/flux config host models
/flux config host pi model active|<provider>/<model-id>
/flux config host pi thinking active|off|minimal|low|medium|high|xhigh
/flux config host codex model active|<model-id>
/flux config host codex thinking active|off|minimal|low|medium|high|xhigh
/flux config host claude-code model active|<model-id>
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
- Claude Code configured model selection is wired through `--model`, but Claude thinking/effort flags still need live CLI/doc validation before wiring. Codex configured model and effort are wired through `-m` and `-c model_reasoning_effort=...`.
- Avoid hard-coded model names. Mythos/Fable-style models should work if present in the harness registry or CLI model list.

## Acceptance Criteria

- [x] Add config shape for host-native sidecar model selection and thinking effort.
- [x] Add validation and `/flux config host ...` commands.
- [x] Pi: list available harness models and select a sidecar model if supported.
- [x] Pi: apply/clamp sidecar thinking level if supported without mutating the user's active session settings.
- [x] Direct providers: parse and send `thinkingEffort` where provider-compatible.
- [ ] Claude Code: validate and wire thinking/effort CLI flags if current Claude Code supports them. Model selection is wired through `--model`.
- [x] Codex: validate and wire model/effort CLI flags if current Codex supports them.
- [x] Add tests for config parsing/validation and host caller argv/options.
- [x] Document host-specific support/limitations in `docs/HANDOFF.md` and `README.md`.
- [ ] Live-smoke Pi/Codex/Claude configured sidecar selections in real harness contexts.
