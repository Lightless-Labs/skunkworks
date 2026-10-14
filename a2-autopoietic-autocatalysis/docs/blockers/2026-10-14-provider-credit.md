# Blocker: No Funded Cheap Model Provider for E1

**Date:** 2026-10-14
**Blocks:** E1 step 3 (calibration) and everything after it
**Status:** Open. Needs a funded provider. The deterministic harness and boundary are ready.

## Facts (one minimal headless call per model, 2026-10-14)

| Provider / model | Result |
|---|---|
| Pi `zai/glm-5.2`, `zai/glm-4.7` | `429 {"code":"1113","message":"Insufficient balance or no resource package. Please recharge."}`, with and without the E1 sandbox/proxy |
| Pi `minimax/MiniMax-M3`, `minimax/MiniMax-M2.7` | `402 insufficient_balance_error (1008)` |
| Pi `kimi-coding/k2p7`, `kimi-coding/kimi-for-coding` | `403 permission_error: Your current subscription does not have access to Kimi Code` |
| Pi `openai-codex/gpt-5.6-luna` | `No API key for provider: openai-codex` (headless) |
| Gemini CLI `gemini-2.5-flash`, `gemini-3.1-pro-preview` | user setup/auth failure (`_doSetupUser`) |
| OpenCode free tier (`opencode/big-pickle`) | "free tier can only be used from within OpenCode" |
| A² `codex` provider | `run_codex` still passes the removed `--full-auto` flag, so A²-launched Codex cannot start |

The E1 smoke run (`smoke-1`, a gate-failed task, never part of the sample) reached the model through the egress proxy: 3 allowed CONNECTs to `api.z.ai:443`. It produced an empty agent turn only because of the 429. The boundary and the pipeline are not the problem.

## Why I didn't work around it

- **Expected scale.** About 43 tasks × (1 + 3 + 3 worst-case) ≈ 300 agentic attempts, many model calls each.
- **ChatGPT-subscription Codex (`gpt-5.6-luna`)** works today, but it is not a drop-in replacement:
  1. Its server-side web search would bypass the egress proxy, and that cannot be verified from the client.
  2. About 300 attempts would compete with the operator's other active Codex sessions under the plan's usage windows.
  3. It changes the pre-registered provider class from "cheap Pi models".
- **Claude** would spend the session-critical subscription.

E1-scale spend on a shared subscription is the operator's decision. It is surfaced here rather than taken.

## Unblock Options

1. **Top up Z.ai (GLM coding plan) or MiniMax.** This restores the plan exactly as written; calibration can start immediately. This is the preferred option.
2. **Authorize Codex `gpt-5.6-luna`** for E1. Before that, A² needs `run_codex` fixed (remove `--full-auto`, launch under the outer Seatbelt with Codex's own sandbox bypassed), and web search disabled and verified. Pacing would be one worker.

Either way, `bench/external-value/protocol-v1.toml` is committed with the chosen provider before the first non-calibration call.
