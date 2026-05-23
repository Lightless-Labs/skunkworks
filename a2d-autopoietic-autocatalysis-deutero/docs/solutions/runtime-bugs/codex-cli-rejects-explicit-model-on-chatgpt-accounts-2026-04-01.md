---
title: "Codex CLI Rejects Explicit Model Names on ChatGPT Accounts"
date: 2026-04-01
category: runtime-bugs
module: providers
problem_type: runtime_error
component: tooling
symptoms:
  - "Coder enzyme fails with exit code 1, cycle produces 0 invocations and 0 mutations"
  - "Codex CLI returns: The 'o4-mini' model is not supported when using Codex with a ChatGPT account"
  - "Error only visible in process stderr, not surfaced by metabolism orchestration"
root_cause: config_error
resolution_type: code_fix
severity: high
tags:
  - codex
  - cli-provider
  - model-flag
  - chatgpt-account
  - enzyme-invocation
  - provider-dispatch
---

# Codex CLI Rejects Explicit Model Names on ChatGPT Accounts

## Problem

The Codex CLI distinguishes between API accounts and ChatGPT accounts. When invoked with an explicit `--model` flag (e.g., `--model o4-mini` or `--model o3`), ChatGPT-tier accounts reject the request outright even though those model names are valid for API-tier accounts. This silently blocked all Codex-backed enzyme invocations, producing cycles with zero catalytic activity.

## Symptoms

- Coder enzyme fails with exit code 1; the metabolism cycle completes but reports 0 invocations and 0 mutations
- Codex process stderr contains: `The 'o4-mini' model is not supported when using Codex with a ChatGPT account.`
- The error is not surfaced by the metabolism orchestration layer -- the cycle appears to succeed silently with no progress
- Identical configuration works on API-tier accounts, making the failure environment-specific

## What Didn't Work

- Switching model names (e.g., `o3` instead of `o4-mini`) -- ChatGPT accounts reject all explicit model names, not just specific ones
- Looking at metabolism-level logs for errors -- the provider subprocess exit code was consumed but the stderr content was not propagated

## Solution

Made the `--model` flag conditional in `CliProvider::codex()`. When the model parameter is an empty string, the flag is omitted entirely, letting the Codex CLI use the account's default model.

```rust
// crates/a2d-providers/src/cli.rs
pub fn codex(model: &str) -> Self {
    let model = model.to_string();
    let name = if model.is_empty() {
        "codex/default".to_string()
    } else {
        format!("codex/{model}")
    };
    // ...
    args_builder: Box::new(move |req| {
        let mut args = vec![
            "exec".to_string(),
            format!("{}\n\n{}", req.system, req.prompt),
        ];
        if !model.is_empty() {
            args.push("--model".to_string());
            args.push(model.clone());
        }
        args
    }),
}
```

Configuration passes an empty string for the model to use the account default:

```rust
CliProvider::codex("")  // uses account default model
```

## Why This Works

The Codex CLI internally resolves the default model for the authenticated account tier. ChatGPT accounts have a fixed model allocation that cannot be overridden by `--model`. By omitting the flag, we delegate model selection to the CLI itself, which correctly handles both account types. The provider name becomes `codex/default` for traceability.

## Prevention

- When wrapping external CLIs, treat model/configuration flags as optional and test with the flag omitted -- CLIs may enforce account-tier restrictions that are not documented in `--help`
- Surface subprocess stderr in provider error paths so that CLI-level rejections are visible in metabolism logs rather than silently swallowed
- Test provider dispatch against both API and ChatGPT account types when the underlying tool distinguishes them

## Related Issues

- Commit `74df186` — Fix Codex provider: use account default model
- Commit `47c4486` — Restore Codex as coder provider, Gemini Pro for evolver
- `crates/a2d-providers/src/cli.rs` — CliProvider implementation
