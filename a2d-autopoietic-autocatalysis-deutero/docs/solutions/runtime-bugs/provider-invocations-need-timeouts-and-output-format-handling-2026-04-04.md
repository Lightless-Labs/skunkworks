---
title: "Provider Invocations Need Timeouts and Output Format Handling"
date: 2026-04-04
category: runtime-bugs
module: providers
problem_type: bug_fix
component: tooling
symptoms:
  - "A2D hangs indefinitely during a metabolism cycle with no error output"
  - "OpenCode subprocess never returns — entire system blocked on a single provider"
  - "Extracted code contains ANSI escape sequences instead of valid source"
root_cause: missing_defensive_constraints
resolution_type: code_fix
severity: high
tags:
  - opencode
  - cli-provider
  - timeout
  - subprocess
  - ndjson
  - ansi-escape
  - kimi-k2.5
  - silent-failure
  - provider-dispatch
---

# Provider Invocations Need Timeouts and Output Format Handling

## Problem

During Kimi k2.5 integration via OpenCode CLI, A2D hung indefinitely. Three independent failures compounded into a single silent system-level hang:

1. **No output format flag**: OpenCode without `--format json` returns human-readable output with ANSI escape codes. Code extraction parsed the ANSI-polluted output as if it were clean source, producing garbage mutations.
2. **No subprocess timeout**: The provider subprocess had no deadline. A hung or slow provider blocked the entire metabolism cycle forever — no timeout, no kill, no fallback.
3. **No failure detection**: The orchestration layer did not detect or report the hang. The system sat there with no log output, no error, no indication that anything was wrong.

Any one of these alone would have been recoverable. Together they created a silent total system failure.

## Symptoms

- A2D starts a metabolism cycle, dispatches to OpenCode/Kimi, and never progresses
- No error messages, no timeout, no log output after the subprocess spawn
- If the subprocess does return, extracted code contains `\x1b[` escape sequences and fails to compile, but this secondary failure is masked by the primary hang

## What Didn't Work

- Waiting longer — the subprocess had no timeout, so "longer" means "forever"
- Checking A2D logs — nothing was logged after the subprocess spawn because the blocking `wait()` call never returned
- Assuming model IDs follow a predictable pattern — `kimi-k2.5` is not a valid OpenCode model ID

## Solution

Three changes, each addressing one failure mode:

### 1. Force structured output with `--format json`

```rust
// Always pass --format json to OpenCode invocations
args.push("--format".to_string());
args.push("json".to_string());
```

OpenCode's JSON format emits NDJSON (newline-delimited JSON) with typed events. Parse the `text` events to extract clean code without ANSI pollution.

### 2. Add subprocess timeout with explicit kill

```rust
// 5-minute timeout — generous enough for slow models, short enough to not block a cycle
let timeout = Duration::from_secs(300);
match tokio::time::timeout(timeout, child.wait()).await {
    Ok(status) => { /* handle normally */ }
    Err(_) => {
        child.kill().await?;
        return Err(ProviderError::Timeout {
            provider: self.name.clone(),
            elapsed: timeout,
        });
    }
}
```

The timeout kills the subprocess and returns a typed error. The metabolism cycle records the failure and continues with remaining providers.

### 3. Record failure instead of hanging

The cycle now treats a provider timeout as a failed invocation — logged, counted, and reported in cycle metrics — rather than an indefinite block. Other providers in the same cycle are unaffected.

## OpenCode Model ID Discovery

Model IDs in OpenCode do not follow obvious naming conventions. Run `opencode models` to discover the correct IDs for your configured providers:

| Common name | OpenCode model ID |
|---|---|
| Kimi k2.5 | `kimi-for-coding/k2p5` |
| GLM 5.1 | `zai-coding-plan/glm-5.1` |

Do not guess model IDs. The CLI will accept an invalid ID without error and then fail at inference time.

## Why This Works

Each fix addresses an independent failure mode:

- **Structured output** eliminates the class of bugs where human-readable formatting corrupts machine-consumed output. NDJSON is unambiguous and parseable.
- **Timeouts** convert an unbounded wait into a bounded one. Five minutes is generous for any reasonable model response; anything longer is a genuine hang.
- **Failure recording** means the system's observability is never worse than "provider X timed out after 5 minutes" — a diagnosable state, not a mystery hang.

The general principle: **every subprocess invocation needs a timeout, and every external tool needs structured output**. Human-readable output is for humans. Machine-consumed output must be machine-parseable.

## Prevention

- When integrating any new CLI provider, always check for a structured output mode (`--format json`, `--output json`, `--json`) and use it. Never parse human-readable CLI output in production code.
- Every `child.wait()` call must have a timeout. No exceptions. A subprocess without a deadline is a system-level liveness hazard.
- Provider failures must be surfaced in cycle metrics. If a failure can happen silently, it will happen silently, and you will not know why the system stopped making progress.
- When adding a new model via OpenCode, run `opencode models` first and use the exact ID from the output. Document the mapping for the next person.

## Related Issues

- `docs/solutions/runtime-bugs/codex-cli-rejects-explicit-model-on-chatgpt-accounts-2026-04-01.md` — Same category: CLI provider integration surprises that silently block the cycle
- `crates/a2d-providers/src/cli.rs` — CliProvider implementation where these fixes land
