# Improve `/flux config` UX

**Created:** 2026-05-28
**Status:** Open

## Context

`/flux config` exists and can initialize, show, edit, and update random frequency settings. Model/prompt-pool changes still require full JSON editing via `/flux config edit`.

Useful next step: make common config operations safe and discoverable without requiring users to hand-edit a large JSON blob.

## Acceptance Criteria

- [ ] Add `/flux config set enabled true|false` or decide runtime-only `/flux on|off` is enough.
- [ ] Add `/flux config random on|off` if persistent random enable/disable should be separate from runtime `/flux random on|off`.
- [ ] Add a command to add/update a model definition, or provide an interactive editor flow for model fields.
- [ ] Add a command to assign a model pool: e.g. `/flux config pool random haiku,gpt-mini`.
- [ ] Add a command to list full prompt profile styles, not just names/weights.
- [ ] Add a command or editor flow to add/update prompt profiles under a trigger kind/name.
- [ ] Validate JSON after every command-written config mutation.
- [ ] Keep file permissions restrictive (`0600`) for config files because users may accidentally include literal API keys.
- [ ] Update README command list and `docs/HANDOFF.md` after implementation.

## Notes

The simplest acceptable path may be interactive Pi UI dialogs rather than dense slash-command syntax.
