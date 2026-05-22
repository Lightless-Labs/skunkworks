---
title: OpenCode GLM upstream balance failures can present as silent A² timeouts
date: 2026-05-22
module: a2_workcell
problem_type: integration_issue
component: opencode_provider
severity: medium
tags: [opencode, glm, zai, timeout, provider-availability, benchmark]
applies_when:
  - "A²/OpenCode GLM attempts time out with tokens=0 and patch=false"
  - "Calibrating GLM timeout or token budgets"
  - "OpenCode emits no stdout/stderr through normal a2ctl capture"
---

# OpenCode GLM upstream balance failures can present as silent A² timeouts

## Observation

A 2026-05-22 calibration run used:

```bash
bench/self_correction.py \
  --fixture fibonacci \
  --provider opencode/zai-coding-plan/glm-5.1 \
  --attempts 1 \
  --timeout 3600 \
  --max-tokens 200000 \
  --results /tmp/a2-glm-calibration-fibonacci-timeout3600.jsonl
```

The attempt timed out at 3600s with `tokens=0`, `patch=false`, no touched files, and the Fibonacci verifier still failing. This looked like a budget/latency issue from the A² result alone.

A direct OpenCode smoke with logs showed the underlying provider state:

```bash
opencode --print-logs --log-level INFO run \
  --format json \
  --model zai-coding-plan/glm-5.1 \
  --dir . \
  'Respond with exactly OK.'
```

The run produced an upstream error in logs:

```text
statusCode: 429
responseBody: {"error":{"code":"1113","message":"Insufficient balance or no resource package. Please recharge."}}
```

## Rule

Before increasing GLM benchmark timeouts beyond an already-large budget, run a direct `opencode --print-logs` smoke. If logs show provider account/resource errors, mark the provider unavailable and do not treat the result as model capability or A² loop evidence.

## Why this matters

`a2ctl` currently wraps the model call in a wall-clock timeout and captures stdout/stderr after process exit. When OpenCode is stuck retrying or waiting behind an upstream provider error, A² can record only `tokens=0` and a timeout. Without direct OpenCode logs, provider-account failures look like calibration failures.

## Follow-up options

- Re-test GLM only after ZAI balance/resource package is restored.
- Prefer the Pi/ZAI route (`pi/zai/glm-5.1`) when OpenCode-specific provider behavior is not under test.
- Consider adding a short preflight provider smoke or better timeout diagnostics for `tokens=0` OpenCode runs.
- Keep benchmark records factual: timeout/provider-availability evidence, not model-capability conclusions.

## 2026-05-22 addendum

After the ZAI subscription was restored, direct Pi access worked with the existing Pi `zai` API key:

```bash
pi --model zai/glm-5.1 --no-session --no-tools -p 'Respond with exactly OK.'
```

A² added a `pi` provider route and validated `pi/zai/glm-5.1` on the self-correction harness. `compound-hidden` N=3 scored resolved 3/3, pass@1 0/3, loop exercised 3/3, self-corrected 3/3. Results: `/tmp/a2-compound-hidden-pi-zai-glm.jsonl`.
