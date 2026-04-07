---
title: Arbitrary token/time budgets create false benchmark variance — calibrate, do not guess
date: 2026-04-05
module: a2ctl-benchmark
problem_type: best_practice
component: tooling
severity: high
applies_when:
  - Benchmark scores fluctuate run-to-run with the same model and tasks
  - The harness imposes a per-task token or wall-clock budget
  - You suspect model regression but cannot reproduce
tags:
  - benchmark
  - budget
  - variance
  - reproducibility
  - noise-floor
---

# Arbitrary token/time budgets create false benchmark variance — calibrate, do not guess

## Context

Same model (GLM 5.1), same five tasks, three runs: 5/5, 4/5, 3/5. The instinct was "model variance" or "provider flakiness." The actual cause: a 50k token budget and 300s wall-clock timeout, both chosen by gut feel, were tight enough that any task that wandered (extra reasoning, retried tool call, slow network) tripped one or the other and was scored as a failure.

After raising the budget to 100k tokens and the timeout to 1800s, the same model scored 5/5 on every run. The model wasn't varying; the harness was clipping legitimate work.

## Guidance

1. **Calibrate budgets against the slowest successful run, not the median.** A budget that lets the median pass will fail the tail.
2. **Treat budget-tripped failures as harness noise, not model signal.** They should be excluded from scoring or scored as "indeterminate," never as "wrong answer."
3. **Log the actual token and time consumption on every task.** Without this you cannot tell a budget trip from a wrong answer.
4. **When variance appears, raise the budget first, debug the model second.** It is much cheaper to confirm a budget is generous than to chase ghost regressions.
5. **Document the budget as part of the benchmark definition.** "GLM 5.1 scored 5/5 at 100k tokens / 1800s" is reproducible. "GLM 5.1 scored 5/5" is not.

## Why This Matters

Benchmarks with hidden noise floors actively destroy the signal a self-modifying system depends on. A run-to-run delta of ±2 on a 5-task suite makes any "improvement" indistinguishable from variance, and the system either freezes (no signal) or chases phantoms (false signal). The cost of a too-generous budget is wall-clock time. The cost of a too-tight budget is the entire evaluation loop's credibility.

The deeper rule: **noise floors must be measured before claims are made**. Run the same configuration N times and look at the distribution before interpreting any single result.

## Examples

Symptom (with old budget):
```
Run 1: GLM 5.1 — 5/5
Run 2: GLM 5.1 — 4/5  (task 003 hit token cap mid-edit)
Run 3: GLM 5.1 — 3/5  (tasks 003, 008 hit timeout)
```

After raising budget to 100k / 1800s:
```
Run 1: GLM 5.1 — 5/5
Run 2: GLM 5.1 — 5/5
Run 3: GLM 5.1 — 5/5
```

## Related

- `crates/a2ctl/src/bench.rs` — budget constants
- `docs/HANDOFF.md` — variance investigation notes
