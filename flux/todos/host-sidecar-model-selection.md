# Host-Native Sidecar Model Selection

**Created:** 2026-06-05
**Status:** Proposed

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

Pi commands:

```text
/flux config host models
/flux config host model active
/flux config host model <provider>/<model-id>
/flux config host thinking active
/flux config host thinking off|minimal|low|medium|high|xhigh
```

Direct-provider command should also grow the already-typed `thinkingEffort` option:

```text
/flux config model <name> <openai-compatible|anthropic> <model-id> thinkingEffort=medium
```

## Implementation Notes

- `FluxModelSpec.thinkingEffort` already exists, but config commands do not expose it and direct-provider clients do not send it yet.
- Pi's actual registry API exposes `ctx.modelRegistry.getAll()`, `getAvailable()`, `find(provider, modelId)`, `hasConfiguredAuth(model)`, and `getApiKeyAndHeaders(model)`. Models carry `provider`, `id`, `name`, `api`, `reasoning`, `thinkingLevelMap`, `contextWindow`, and `maxTokens` metadata.
- Pi exposes active thinking controls through `pi.getThinkingLevel()` / `pi.setThinkingLevel()`. The lower-level `@earendil-works/pi-ai complete()` accepts provider stream options, and `SimpleStreamOptions` includes `reasoning?: ThinkingLevel`, but Flux currently calls `complete()` with `ProviderStreamOptions`; verify the right option key per provider before implementation.
- Pi sidecar generation should not permanently change the user's active model/thinking level just to generate a Flux thought. Prefer passing the chosen model/effort directly into the sidecar `complete()` call if the API supports it; otherwise document the limitation.
- Claude Code / Codex model and effort flags need live CLI/doc validation before wiring; preserve default host behavior when unsupported.
- Avoid hard-coded model names. Mythos/Fable-style models should work if present in the harness registry or CLI model list.

## Acceptance Criteria

- [ ] Add config shape for host-native sidecar model selection and thinking effort.
- [ ] Add validation and `/flux config host ...` commands.
- [ ] Pi: list available harness models and select a sidecar model if supported.
- [ ] Pi: apply/clamp sidecar thinking level if supported without mutating the user's active session settings.
- [ ] Direct providers: parse and send `thinkingEffort` where provider-compatible.
- [ ] Claude Code: validate and wire model/effort CLI flags if current Claude Code supports them.
- [ ] Codex: validate and wire model/effort CLI flags if current Codex supports them.
- [ ] Add tests for config parsing/validation and host caller argv/options.
- [ ] Document host-specific support/limitations in `docs/HANDOFF.md` and `README.md`.
