# Live-Validate Pi Extension

**Created:** 2026-05-28
**Status:** Completed 2026-06-15
**Completed:** 2026-06-15 — interactive Pi TUI smoke reported passing after refreshing the git-installed skunkworks package.

## Context

`src/adapters/pi/index.ts` compiles and uses documented Pi extension APIs. Flux now uses Pi's host-native selected/authenticated model path instead of requiring a separate sidecar model config when running as a Pi extension.

A non-interactive Pi JSON-mode smoke was run on 2026-05-30 and confirmed host-native generation for `/flux think` and the `flux_stray_thought` tool. Interactive Pi TUI validation was reported passing on 2026-06-15 after reinstalling/updating Flux from `git:git@github.com:Lightless-Labs/skunkworks.git`.

## Acceptance Criteria

- [x] Confirm Pi host-native model generation works without a Flux-specific sidecar API key config in non-interactive JSON mode.
- [x] Expose Flux as a repo-level Pi package/extension wrapper so `pi install git:git@github.com:Lightless-Labs/skunkworks.git` can install it without manual cloning or npm publishing.
- [x] Smoke repo-level package loading with `pi --no-extensions -e . --no-session --mode json -p "/flux status"`.
- [x] Smoke project-local install from a temp workspace with `pi install -l /Users/thomas/Projects/lightless-labs/skunkworks`, then `pi --no-session --mode json -p "/flux status"`.
- [x] Start Pi from a project with Flux installed (`pi install -l /path/to/skunkworks` or git install) in interactive TUI mode.
- [x] Confirm startup shows Flux status and no extension load errors.
- [x] Run `/flux status` and verify it reports config path, enabled/random state, models, and random frequency.
- [x] Run `/flux config prompts` and `/flux config models`.
- [x] Run `/flux think smoke test` in JSON mode and verify a `flux:stray-thought` custom message is injected.
- [x] Trigger `flux_stray_thought` from the agent in JSON mode and verify it returns tool content plus optional displayed custom message.
- [x] With `random.probability=1` and low `minIntervalMs`, verify `turn_end` can auto-inject.
- [x] With `/flux random off`, verify random triggers stop while manual `/flux think` still works.
- [x] Emit `pi.events.emit("flux:trigger", { reason: "smoke" })` from a small companion extension or temporary command and verify external trigger injection.
- [x] Record results in `docs/HANDOFF.md` and update this todo.

## Notes

Pi smoke commands used:

```bash
pi --no-extensions -e . --no-session --mode json -p "/flux status"

# From a temporary workspace:
pi install -l /Users/thomas/Projects/lightless-labs/skunkworks
pi --no-session --mode json -p "/flux status"

cd flux
pi --no-extensions -e ./src/adapters/pi/index.ts --no-session --mode json -p "/flux think smoke test"

pi --no-extensions -e ./src/adapters/pi/index.ts --no-session --mode json \
  --tools flux_stray_thought \
  -p "Use the flux_stray_thought tool with reason smoke test, then stop."
```

The `/flux think` command disables `triggerTurn` in non-UI print/json mode to avoid stale-context errors after the smoke custom message is emitted. In interactive/RPC modes it still triggers a turn.

2026-06-15 interactive TUI smoke: after pushing the Flux Pi runtime config fix and refreshing the git-installed skunkworks package, live Pi testing reportedly passed for startup/status, `/flux status`, `/flux config prompts`, `/flux config models`, runtime `/flux on|off`, runtime `/flux random on|off`, `/flux reload`, persistent `/flux config set enabled true|false`, persistent `/flux config random on|off`, `/flux config edit`, random turn-end injection, manual `/flux think`, and external `flux:trigger` injection. The manual `/flux think` path produced a `flux:stray-thought` custom message in-session.

The Flux sidecar should only receive bounded snapshots, not filesystem/shell tools.
