# Improve `/flux config` UX

**Created:** 2026-05-28
**Status:** Completed 2026-05-30

## Context

`/flux config` exists and can initialize, show, edit, and update random frequency settings. Model/prompt-pool changes still require full JSON editing via `/flux config edit`.

Useful next step: make common config operations safe and discoverable without requiring users to hand-edit a large JSON blob.

## Acceptance Criteria

- [x] Add `/flux config set enabled true|false` or decide runtime-only `/flux on|off` is enough.
- [x] Add `/flux config random on|off` if persistent random enable/disable should be separate from runtime `/flux random on|off`.
- [x] Add a command to add/update a model definition, or provide an interactive editor flow for model fields.
- [x] Add a command to assign a model pool: e.g. `/flux config pool random haiku,gpt-mini`.
- [x] Add a command to list full prompt profile styles, not just names/weights.
- [x] Add a command or editor flow to add/update prompt profiles under a trigger kind/name.
- [x] Validate JSON after every command-written config mutation.
- [x] Keep file permissions restrictive (`0600`) for config files because users may accidentally include literal API keys.
- [x] Update README command list and `docs/HANDOFF.md` after implementation.

## Notes

The simplest acceptable path may be interactive Pi UI dialogs rather than dense slash-command syntax.

## Progress

2026-05-30: Added persistent `/flux config set enabled true|false`, `/flux config random on|off`, `/flux config model <name> <provider> <model-id> [key=value...]`, `/flux config pool <pool> <models>`, `/flux config prompt <pool> <profile-name> <weight> <style text...>`, full prompt-style listing, config validation before command-written persistence, and explicit `0600` chmod after config writes.
