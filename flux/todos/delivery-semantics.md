# Clarify Cross-Host Delivery Semantics

**Created:** 2026-05-28
**Status:** Completed 2026-05-30

## Context

`DeliveryMode` currently includes:

```ts
"steer" | "followUp" | "nextTurn" | "stdout" | "file"
```

The Pi adapter implements `steer`, `followUp`, and `nextTurn` through `pi.sendMessage()`. The hook CLI prints JSON to stdout. There is no general `file` delivery implementation, and `stdout` is more a hook transport than a Pi delivery mode.

## Acceptance Criteria

- [x] Decide whether `stdout` and `file` belong in shared `DeliveryMode` or in host-specific hook config.
- [x] If keeping `file`, implement it in hook CLI and/or Pi adapter with documented path config and safe permissions. Decision: do not keep `file` in shared delivery until a concrete host implements it.
- [x] If keeping `stdout`, document it as hook-only and prevent Pi from silently mapping it to `steer`.
- [x] Update config example and README to avoid over-promising incomplete delivery modes.
- [x] Add tests or smoke fixtures for selected behavior.

## Notes

Avoid surprising users: a config value should either work as documented for the current host or fail with a clear warning.

## Result

Shared `DeliveryMode` now only covers agent/session message delivery: `steer`, `followUp`, and `nextTurn`. Hook CLIs continue to use stdout as their transport, but `stdout` is no longer presented as a configurable delivery mode. `file` is removed until a real host-specific implementation exists. The Pi adapter validates delivery before sending and warns instead of silently mapping unsupported stale config values to `steer`.
