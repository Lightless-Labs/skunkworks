---
title: "Fitness-Gated Evolution: Holdout Benchmarks + RAF Closure + Lineage Gating"
date: 2026-04-01
category: best-practices
module: germline
problem_type: best_practice
component: testing_framework
severity: high
applies_when:
  - "Running self-improvement cycles where code is generated, tested, and evolved"
  - "Germline mutations must be mechanically validated before persisting"
  - "Regression prevention is a hard requirement, not advisory"
  - "Multi-model pipelines produce code that no single model reviews end-to-end"
tags:
  - fitness-gating
  - holdout-benchmark
  - raf-closure
  - lineage
  - regression-prevention
  - self-improvement
  - multi-model
  - evolution
---

# Fitness-Gated Evolution: Holdout Benchmarks + RAF Closure + Lineage Gating

## Context

Stage 2 verified self-improvement runs end-to-end evolution cycles: a coder produces code, a tester evaluates it, an evolver mutates the germline, and a holdout benchmark scores the result. The critical question is what happens when evolution makes things worse. Without a regression gate, a bad mutation overwrites the archive and poisons all future cycles.

## Guidance

Combine three independent verification systems into a fitness gate that ensures the lineage can only improve or stay the same, never regress.

### The three systems

1. **Holdout benchmark** (mechanical fitness): A benchmark the coder never sees during generation. Scores the produced artifact against objective checks. This is the fitness function — not model opinion, not conversational review. In practice: 5 email-validation checks, scored as percentage (0%–100%).

2. **RAF closure gate** (structural integrity): Before any commit, verify that the autocatalytic set remains closed — every catalyst has a source, every product feeds a consumer. This prevents structurally broken germlines from entering the archive regardless of fitness score.

3. **Fitness-gated lineage** (regression prevention): Compare the current cycle's holdout score against the archive's best score. If the new score is lower, SKIP the lineage commit. The archive stays at the previous germline. Only equal-or-better scores advance.

### How they compose

```
Cycle N:
  Coder (Codex) → code artifact
  Tester (Gemini Flash) → evaluation
  Evolver (Gemini Pro) → mutated germline
  Holdout benchmark → score (e.g. 20%)
  RAF closure → pass/fail
  Lineage gate: score >= archive_best? → commit / skip

Cycle N+1:
  Uses germline from archive (which may be from cycle N or earlier)
  Same pipeline, new mutations
  If score drops (e.g. 0%) → lineage commit SKIPPED
  Archive remains at cycle N germline
```

### What happened in practice

| Cycle | Coder | Tester | Evolver | Holdout Score | Lineage |
|-------|-------|--------|---------|---------------|---------|
| 1 | Codex | Gemini Flash | Gemini Pro | 20% (1/5) | Committed |
| 2 | Codex | Gemini Flash | Gemini Pro | 0% (0/5) | **Skipped** (regression detected: -20%) |

- 15 total provider invocations across both cycles, 0 failures
- The fitness gate detected cycle 2's regression (score dropped from 20% to 0%) and prevented the bad germline from entering the archive
- After skip, the archive still contains the cycle 1 germline — future cycles evolve from the best-known state

## Why This Matters

- **Monotonic improvement guarantee**: The archive can only get better or stay the same. Bad mutations are discarded, not accumulated. This is the minimum viable property for autonomous self-improvement.
- **Coarse-grained but effective**: The gate operates per-cycle, not per-mutation. A cycle with 3 good mutations and 1 catastrophic one gets rejected entirely. This is conservative by design — false negatives (rejecting mixed-quality cycles) are preferable to false positives (accepting regressions).
- **Three independent failure modes**: Holdout benchmark catches functional regression. RAF closure catches structural corruption. Lineage gating catches score regression. A germline must pass all three to persist. No single system failure compromises the archive.
- **Multi-model diversity as defense**: Codex writes code, Gemini Flash tests, Gemini Pro evolves. No single model controls the full pipeline. A systematic bias in one model is caught by the others' independent evaluation.
- **Mechanical, not conversational**: Every gate is a computation, not a model opinion. Holdout scores are boolean checks. RAF closure is graph reachability. Lineage comparison is arithmetic. No LLM is asked "is this better?"

## When to Apply

- Any self-improvement or evolution pipeline where generated artifacts persist across cycles
- Systems where regression has compounding cost (bad germline produces worse germline in next cycle)
- Multi-model pipelines where no single actor has end-to-end oversight

**Not needed for:**
- One-shot code generation with human review
- Pipelines where every artifact is independently validated before use
- Research/exploration where regression is acceptable and expected

## Design Considerations

- **Holdout benchmark quality**: The benchmark must test properties the coder cannot game. If the coder sees the benchmark, it optimizes for the benchmark rather than the underlying capability. Keep holdout checks separate from training/generation context.
- **Granularity tradeoff**: Per-cycle gating is simple but conservative. Per-mutation gating would allow partial acceptance of mixed cycles but requires tracking individual mutation effects — significantly more complex. Start coarse, refine if too many good mutations are rejected alongside bad ones.
- **Score ties**: Equal scores should be committed (the germline may have improved in ways the benchmark doesn't measure). Only strict regressions trigger a skip.
- **Bootstrap problem**: Cycle 1 has no prior score to compare against. Use a baseline of 0% or run the holdout against the initial germline before the first evolution cycle.

## Related

- `docs/solutions/best-practices/multi-model-dispatch-mechanical-selection-2026-04-01.md` — mechanical selection pattern that the fitness gate extends to evolution cycles
- `docs/solutions/best-practices/collision-synthesis-cross-project-bootstrapping-2026-04-01.md` — cross-project learnings that motivated the three-system verification approach
- `crates/a2d-core/` — implementation of RAF closure, germline, and observer modules
