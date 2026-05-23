---
title: "The Evolver Enzyme Produces No Measurable Improvement"
date: 2026-04-04
category: architectural-insights
module: metabolism
problem_type: architectural_insight
component: architecture
severity: critical
applies_when:
  - "Evaluating whether the evolver enzyme contributes to cycle fitness"
  - "Diagnosing why multi-cycle evolution does not outperform single-cycle output"
  - "Deciding where to cut cost from the catalytic cycle"
  - "Designing feedback loops for mutation-based self-improvement"
tags:
  - metabolism
  - evolver
  - mutation
  - fitness-plateau
  - feedback-loop
  - blind-mutation
  - dead-weight
  - cycle-architecture
---

# The Evolver Enzyme Produces No Measurable Improvement

## Context

The evolver enzyme is the component responsible for self-improvement across cycles. After the coder produces code and fitness is measured, the evolver mutates enzyme definitions (EnzymeDef fields: reactants, products, catalysts, prompt_template) in the germline, and these mutations are carried forward into subsequent cycles. The expectation is that evolution drives fitness upward over time.

Across every challenge run conducted to date, the evolver has produced zero fitness improvement. Not once has a mutation from the evolver resulted in higher fitness than the coder's first-cycle output.

## Evidence

| Challenge | Provider | Cycle 1 Fitness | Cycle 2 Fitness | Cycle 3 Fitness | Evolver Effect |
|-----------|----------|-----------------|-----------------|-----------------|----------------|
| Sudoku | Codex gpt-5.4 | 83% (5/6) | 83% (0 mutations improved) | Regressed | Zero or negative |
| Sudoku | Kimi k2.5 | 83% (5/6) | 83% (0 mutations improved) | Stuck at 83% | Zero |
| Chess | Codex gpt-5.4 | 50% (3/6) | Never improved | — | Zero |

The pattern is consistent:

1. **Cycle 1**: The coder produces code, fitness is measured. This is the high-water mark.
2. **Cycle 2+**: The evolver proposes mutations to enzyme definitions. Mutations are accepted into the germline. Fitness stays flat or regresses. No mutation has ever improved fitness.

## Diagnosis

The evolver fails because it operates blind. Three specific failure modes compound into total ineffectiveness:

### 1. No failure signal reaches the evolver

The evolver does not know WHY fitness is low. It does not see which acceptance test failed, what the compilation error was, or what the expected vs. actual output looked like. It receives the fitness score (a single number) and the current enzyme definitions, then guesses at improvements. This is equivalent to optimizing a function by mutating its source code while only seeing the final scalar output — no gradients, no error messages, no stack traces.

### 2. Enzyme graph topology changes do not affect code quality

The evolver modifies structural fields on EnzymeDef: reactants, products, catalysts. These control how enzymes connect to each other in the autocatalytic graph — which artifacts flow where, which enzymes catalyze which reactions. But the code quality bottleneck is not in the graph topology. The coder already receives the challenge and produces code. Rewiring which artifacts the coder consumes does not make the coder write better code. The evolver is optimizing the wrong variable.

### 3. Prompt template mutations are undirected

The evolver can modify prompt_template fields, which is the one lever that could theoretically affect code quality. But without knowing what went wrong, prompt changes are random walks in prompt space. The evolver might add "be more careful" or restructure the prompt format, but it has no basis for choosing one mutation over another. Undirected search in a high-dimensional space with no gradient signal does not converge — it wanders.

## Cost

The evolver is the most expensive enzyme in the cycle. It uses high-capability models (GLM 5.1, Gemini Pro) because it must reason about enzyme definitions and propose coherent structural changes. Each evolver invocation costs more tokens than the coder invocation it is trying to improve. Across all runs, the evolver has consumed significant compute budget and produced exactly zero value.

The evolver is pure overhead: expensive, slow, and ineffective.

## Implications

1. **The evolver should be disabled or removed until it can receive failure diagnostics.** Running it in its current form wastes money and time without any possibility of improvement. A cycle that runs coder-only for N cycles will produce identical or better results than a cycle that includes the evolver.

2. **Blind mutation cannot work for this problem class.** Code quality improvement requires causal feedback: which test failed, what the error was, what the fix should address. Biological evolution works because mutations are tested against a rich environment with continuous selection pressure. The evolver has a single scalar fitness score and no selection pressure within a cycle — it is evolution without natural selection.

3. **If the evolver is revived, it must receive structured failure context.** At minimum: the list of acceptance tests, which passed, which failed, the assertion details for failures, and any compilation errors. The evolver should mutate prompt_template based on specific failure modes, not based on the fitness scalar. This transforms blind search into directed repair.

4. **Graph topology mutation should be separated from prompt mutation.** These are fundamentally different operations. Changing the enzyme graph is a structural architecture change (rare, high-risk). Changing prompt content is a behavioral tuning change (frequent, lower-risk). Bundling them in one enzyme conflates two concerns and makes it impossible to attribute improvement to either.

5. **This explains why multi-cycle runs do not outperform single-cycle runs.** The cycle architecture assumes evolution drives improvement over time. With the evolver producing zero value, additional cycles are just re-running the coder with a potentially corrupted germline. The system pays the cost of multi-cycle orchestration without receiving the benefit.

## Recommended Next Steps

- Disable the evolver enzyme and run coder-only cycles as the baseline. Measure whether fitness improves, stays the same, or gets worse (it should stay the same, confirming the evolver is dead weight).
- Design a failure-feedback artifact that flows from the acceptance test runner back to the evolver, containing: test name, pass/fail, assertion details, compilation errors, and the actual code that was tested.
- Prototype a directed evolver that receives failure context and proposes targeted prompt_template changes. Test whether directed mutation outperforms blind mutation.
- Track a new metric: `evolver_value = fitness_after_evolution - fitness_before_evolution` per cycle. This must be positive for the evolver to justify its cost.

## Related

- `docs/solutions/architectural-insights/catalytic-cycle-bottleneck-is-cycle-not-model-2026-04-04.md` — the cycle architecture itself is the bottleneck, and the evolver is a major contributor to that bottleneck
- `docs/solutions/best-practices/fitness-gated-evolution-holdout-raf-lineage-2026-04-01.md` — the fitness gate correctly prevents regressions from evolver mutations, but gating bad mutations is not the same as producing good ones
- `docs/solutions/runtime-bugs/metabolism-cross-cycle-dedup-kills-evolution-2026-04-01.md` — a previous bug where dedup silently killed evolution; even after fixing that bug, the evolver still produces zero improvement
