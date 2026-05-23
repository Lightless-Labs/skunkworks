---
module: metabolism, benchmark, self_sandbox
tags: [positioning, outcomes, correctness, refinery, design-principle]
problem_type: architectural-insight
---

# A²D Converges on Outcomes, Not Correctness

## The Distinction

Refinery converges on *what you tell it to*. It's a tool — configure a prompt, a threshold, a stability window, and it finds consensus among models. The convergence target is human-specified.

A²D converges on *outcomes*. The sandbox doesn't ask "is this correct?" — it asks "does this work?" The acceptance tests don't verify properties of the code; they verify that the artifact achieves the outcome when given to something that doesn't know how it was made.

Correctness is a human-specified proxy. Outcomes are the real thing.

## Implications for Design

1. **Fitness is outcome-based, not property-based.** The benchmark doesn't check "does the code use constraint propagation?" It checks "does the sudoku get solved?" How it gets solved is the coder's problem.

2. **The escalation ladder escalates toward outcomes.** Loop detection doesn't trigger on "bad code" — it triggers on "fitness not improving." Model swap doesn't happen because "the model is wrong" — it happens because the outcome isn't achieved. The system doesn't care about correctness; it cares about results.

3. **The architect modifies the system to improve outcomes.** It doesn't make the metabolism "more correct" — it makes it produce better results. The architect's changes are validated by cargo test (does the system still work?) and by challenge fitness (does the system produce better artifacts?).

4. **Self-modification is outcome-gated, not correctness-gated.** A patch that makes the code uglier but improves fitness is accepted. A patch that's beautifully refactored but doesn't improve fitness is wasted work.

## Relation to Refinery

Refinery is a valuable pattern source (multi-model proposals, cross-evaluation, controversy scoring, self-eval exclusion). A²D can borrow these patterns. But refinery is a convergence engine — it finds agreement. A²D is an outcome engine — it finds solutions. Agreement and solutions are not the same thing.

When A²D uses multi-model proposals (escalation rung 3+), it doesn't pick the answer models agree on. It picks the answer that passes the sandbox. Models can unanimously agree on a wrong solution; the sandbox catches it. Models can violently disagree about a correct solution; the sandbox validates it.

The sandbox is the oracle. Models are the search strategy.
