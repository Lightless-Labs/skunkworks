---
title: "The Catalytic Cycle Bottleneck Is in the Cycle, Not the Model"
date: 2026-04-04
category: architectural-insights
module: metabolism
problem_type: architectural_insight
component: architecture
severity: high
applies_when:
  - "Benchmarking A2D cycle fitness against single-model one-shot baselines"
  - "Deciding whether to upgrade the coder model or improve the orchestration"
  - "Diagnosing why cycle fitness plateaus below one-shot frontier performance"
  - "Allocating engineering effort between model selection and cycle architecture"
tags:
  - metabolism
  - cycle-architecture
  - bottleneck
  - model-independence
  - orchestration
  - benchmark
  - fitness-plateau
---

# The Catalytic Cycle Bottleneck Is in the Cycle, Not the Model

## Context

When benchmarking A2D's catalytic cycle against single-model one-shot baselines on the sudoku challenge, we observed a surprising result: swapping the coder enzyme from a frontier model (Codex gpt-5.4) to a non-frontier model (Kimi k2.5) produced identical fitness — 83% (5/6 acceptance tests). The same acceptance test failed in both configurations. Meanwhile, both Gemini 3 Pro and Codex achieve 100% one-shot on the same challenge without the cycle.

This means the 17% gap between cycle output (83%) and one-shot baselines (100%) is not caused by model capability. It is caused by how the cycle orchestrates the models — the metabolism, prompt routing, artifact flow, or evolver strategy is the limiting factor.

## Evidence

| Configuration | Fitness | Tests | Lines |
|---------------|---------|-------|-------|
| Codex gpt-5.4 one-shot | 100% | 6/6 | 264 |
| Gemini 3 Pro one-shot | 100% | 7/7 | 201 |
| A2D cycle + Codex gpt-5.4 coder | 83% | 5/6 | — |
| A2D cycle + Kimi k2.5 coder | 83% | 5/6 | — |

Key observations:

1. **Model swap invariance**: Two models of vastly different capability (frontier vs. non-frontier) produce the same cycle fitness. This rules out model capability as the bottleneck.
2. **Same failure mode**: The same acceptance test fails regardless of which model is the coder. The failure is deterministic and structural, not stochastic.
3. **One-shot superiority**: Models that achieve 100% when given the problem directly drop to 83% when mediated by the cycle. The cycle is degrading their output.

## Diagnosis

The cycle sits between the model and the problem. Its job is to decompose, route, and compose — but somewhere in that pipeline, it is either:

- **Losing information**: The prompt templates or artifact routing strip context that the model needs to solve the failing test case.
- **Constraining the solution space**: The evolver strategy or prompt structure steers the model away from approaches that would solve the full problem.
- **Failing to feed back acceptance results**: The coder may not receive specific, actionable feedback about which test fails and why, preventing it from correcting the deficit.
- **Imposing unnecessary decomposition**: The cycle may be decomposing a problem that models can solve atomically, introducing seams where errors accumulate.

The model-swap invariance is the critical clue. If two models with different capability profiles produce the same failure, the failure is upstream of model execution — it lives in what the cycle gives the model, not in what the model does with it.

## Implications

1. **Invest in cycle architecture, not model upgrades**: Upgrading from Kimi k2.5 to Codex gpt-5.4 produced zero fitness gain. The next improvement will come from fixing the cycle, not from waiting for a better model.

2. **The cycle must not degrade model capability**: A well-designed orchestration should achieve at least parity with one-shot for problems within model capability. If the cycle scores lower than one-shot, the cycle is actively harmful — it is a tax, not an amplifier.

3. **Acceptance test feedback is likely the lever**: The most probable bottleneck is that the coder receives the challenge but does not receive structured feedback from failed acceptance tests. A one-shot model sees the full problem and solves it. A cycle-mediated model sees whatever the cycle chooses to show it — and that view may be incomplete.

4. **This generalizes beyond sudoku**: Any challenge where one-shot baselines outperform the cycle is exhibiting the same pattern. The cycle must be audited for information loss on every such challenge, not just this one.

## Recommended Next Steps

- Instrument the artifact flow to log exactly what the coder enzyme receives as input, and compare it to the one-shot prompt that achieves 100%.
- Add acceptance test failure details (which test, what assertion, actual vs. expected) to the feedback artifact that flows back to the coder on retry.
- Test a "passthrough" cycle mode that gives the coder the raw challenge with no decomposition, to isolate whether the overhead is in decomposition or in feedback.
- Track the metric: `cycle_fitness / one_shot_fitness` per challenge. Any value below 1.0 is a cycle architecture bug, not a model limitation.

## Related

- `docs/solutions/best-practices/acceptance-test-coverage-one-test-is-not-testing-2026-04-02.md` — the acceptance test suite that revealed this gap
- `docs/solutions/runtime-bugs/metabolism-cross-cycle-dedup-kills-evolution-2026-04-01.md` — a previous cycle-level bug that silently degraded multi-cycle evolution
- `docs/solutions/best-practices/multi-model-dispatch-mechanical-selection-2026-04-01.md` — mechanical model selection, which this finding informs: selection should optimize the cycle, not just the model
