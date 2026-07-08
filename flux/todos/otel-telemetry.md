# Opt-In OTel Diagnostics

**Created:** 2026-07-03
**Status:** Proposed

## Context

Flux is intentionally non-critical sidecar behavior: a Flux failure must never stop the host agent from running. Recent Pi abort hardening fixed an ilia failure where automatic Flux sidecar generation could surface as an extension error during unrelated tool handling. Opt-in telemetry would make these failures diagnosable without uploading prompt/tool contents by default.

Telemetry is expected to be used primarily by maintainers during development and dogfooding. It must be off by default and safe to leave disabled forever.

## Goals

- Provide opt-in diagnostics for Flux trigger evaluation, sidecar generation, delivery, and error handling.
- Make it obvious whether a Flux error was automatic/non-fatal or explicit/user-requested.
- Capture enough metadata to debug host adapter issues across Pi, Claude Code, Codex, and generic hooks.
- Avoid collecting prompts, user messages, assistant messages, tool inputs/results, API keys, headers, or raw hook payloads unless a separate explicit debug-context flag is enabled.
- Ensure telemetry export failures are swallowed and can never affect the host agent path.

## Non-Goals

- No default telemetry.
- No product analytics or user behavior tracking.
- No provider alias map or model-release tracking beyond configured/effective/resolved values exposed by hosts.
- No raw session content in default telemetry.

## Candidate Configuration

```json
{
  "telemetry": {
    "enabled": false,
    "endpoint": "http://localhost:4318/v1/traces",
    "serviceName": "flux",
    "debugContext": false
  }
}
```

Potential env overrides:

```bash
FLUX_TELEMETRY=1
OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4318
```

## Default Content-Free Attributes

Common attributes:

- `flux.version` / `flux.commit` when available
- `flux.host`: `pi`, `claude-code`, `codex`, `generic`
- `flux.adapter`: `pi-extension`, `hook-cli`, etc.
- `flux.event.kind`: `turn_end`, `tool_result`, `manual`, `external`, etc.
- `flux.trigger.kind`
- `flux.trigger.name`
- `flux.trigger.fired`
- `flux.run.automatic`
- `flux.outcome`: `skipped`, `generated`, `delivered`, `aborted`, `failed`
- `flux.non_fatal`
- `flux.duration_ms`
- `flux.prompt_profile`
- `flux.context.digest` (optional hash only)
- `flux.context.user_messages_count`
- `flux.context.assistant_messages_count`
- `flux.context.tool_events_count`
- `flux.context.chars`
- `flux.context.truncated`

Model/sidecar attributes:

- `flux.model.path`: `pi-native`, `claude-cli`, `codex-cli`, `direct-provider`
- `flux.model.provider`
- `flux.model.configured`
- `flux.model.resolved` when the host exposes it
- `flux.thinking.configured`
- `flux.thinking.effective`
- `flux.sidecar.fallback_attempted`
- `flux.sidecar.fallback_succeeded`

Pi-specific attributes:

- `flux.pi.hook_path`: `tool_result`, `turn_end`, `command`, `tool`, `external_event`
- `flux.pi.signal_aborted`

Error attributes:

- `error.class`: `abort`, `auth`, `model_unavailable`, `timeout`, `rate_limit`, `config`, `provider`, `delivery`, `unknown`
- `error.message`: redacted and truncated
- `error.stack_hash`: hash only
- `error.was_abort`
- `error.fallback_attempted`
- `error.fallback_failed`

## Debug Context Mode

Only when `debugContext: true`, allow bounded and redacted diagnostics such as:

- rendered Flux prompt,
- bounded context snapshot,
- tool names and coarse result metadata,
- host hook payload summary.

Even in debug mode, redact:

- API keys and auth headers,
- environment variables and secrets,
- long file contents and raw tool output by default,
- home-directory absolute paths where possible.

## Acceptance Criteria

- [ ] Add telemetry config shape and validation.
- [ ] Add no-op telemetry helpers so disabled telemetry has near-zero behavioral impact.
- [ ] Emit spans/events for trigger evaluation, context snapshot construction, sidecar generation, fallback, and delivery.
- [ ] Classify and record errors without throwing from telemetry code.
- [ ] Ensure automatic Pi Flux failures record `non_fatal=true` when swallowed.
- [ ] Keep explicit `flux_stray_thought` errors visible to the caller while recording them if telemetry is enabled.
- [ ] Add tests proving telemetry exporter failures are swallowed.
- [ ] Add tests proving default telemetry never includes prompt/tool content.
- [ ] Document local OTel collector setup for maintainer dogfooding.

## Notes

Example diagnostic target for the ilia abort class:

```text
flux.host=pi
flux.pi.hook_path=tool_result
flux.run.automatic=true
flux.outcome=aborted
flux.non_fatal=true
error.class=abort
error.message="Flux Pi sidecar generation was aborted."
error.stack_hash=<hash>
```
