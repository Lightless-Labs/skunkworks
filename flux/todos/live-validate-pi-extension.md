# Live-Validate Pi Extension

**Created:** 2026-05-28
**Status:** Open

## Context

`src/adapters/pi/index.ts` compiles and uses documented Pi extension APIs, but the extension has not yet been exercised in an interactive Pi session with a real sidecar model config.

## Acceptance Criteria

- [ ] Create `.flux/config.json` from `.flux/config.example.json` with at least one usable API-key-backed model.
- [ ] Start Pi from `flux/` with `pi -e ./src/adapters/pi/index.ts`.
- [ ] Confirm startup shows Flux status and no extension load errors.
- [ ] Run `/flux status` and verify it reports config path, enabled/random state, models, and random frequency.
- [ ] Run `/flux config prompts` and `/flux config models`.
- [ ] Run `/flux think smoke test` and verify a `flux:stray-thought` message is injected and rendered.
- [ ] Trigger `flux_stray_thought` from the agent and verify it returns tool content plus optional displayed custom message.
- [ ] With `random.probability=1` and low `minIntervalMs`, verify `turn_end` can auto-inject.
- [ ] With `/flux random off`, verify random triggers stop while manual `/flux think` still works.
- [ ] Emit `pi.events.emit("flux:trigger", { reason: "smoke" })` from a small companion extension or temporary command and verify external trigger injection.
- [ ] Record results in `docs/HANDOFF.md` and update this todo.

## Notes

Use a cheap sidecar model first. The Flux sidecar should only receive bounded snapshots, not filesystem/shell tools.
