# Clarify Cross-Host Delivery Semantics

**Created:** 2026-05-28
**Status:** Open

## Context

`DeliveryMode` currently includes:

```ts
"steer" | "followUp" | "nextTurn" | "stdout" | "file"
```

The Pi adapter implements `steer`, `followUp`, and `nextTurn` through `pi.sendMessage()`. The hook CLI prints JSON to stdout. There is no general `file` delivery implementation, and `stdout` is more a hook transport than a Pi delivery mode.

## Acceptance Criteria

- [ ] Decide whether `stdout` and `file` belong in shared `DeliveryMode` or in host-specific hook config.
- [ ] If keeping `file`, implement it in hook CLI and/or Pi adapter with documented path config and safe permissions.
- [ ] If keeping `stdout`, document it as hook-only and prevent Pi from silently mapping it to `steer`.
- [ ] Update config example and README to avoid over-promising incomplete delivery modes.
- [ ] Add tests or smoke fixtures for selected behavior.

## Notes

Avoid surprising users: a config value should either work as documented for the current host or fail with a clear warning.
