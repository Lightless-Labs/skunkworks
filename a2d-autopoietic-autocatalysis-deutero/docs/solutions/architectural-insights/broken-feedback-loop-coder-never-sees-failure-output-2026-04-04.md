---
title: "Broken Feedback Loop: The Coder Never Sees Why It Failed"
date: 2026-04-04
category: architectural-insights
module: metabolism
problem_type: architectural_insight
component: architecture
severity: critical
applies_when:
  - "Diagnosing why catalytic cycle fitness plateaus across multiple cycles"
  - "The evolver modifies enzyme topology but fitness does not improve"
  - "Multiple cycles produce nearly identical code despite fitness < 100%"
  - "Acceptance test failures repeat identically across cycles with different models"
  - "Designing the artifact flow between sandbox evaluation and subsequent coder invocations"
tags:
  - metabolism
  - feedback-loop
  - cycle-architecture
  - acceptance-tests
  - sandbox
  - coder
  - evolver
  - fitness-plateau
  - information-flow
  - catalytic-graph
---

# Broken Feedback Loop: The Coder Never Sees Why It Failed

## Context

The A2D catalytic cycle runs acceptance tests — hidden holdout tests the coder never sees — via the sandbox. When a test fails, the fitness score drops. But the coder in the next cycle receives the same prompt as before. It has no information about what went wrong. The cycle is a loop without feedback.

The current information flow:

```
Cycle N:
  Coder gets: requirements + system prompt → produces code
  Sandbox: compiles + runs tests (including hidden acceptance tests) → fitness score
  Evolver gets: fitness report ("83%, 5/6 passed") → modifies enzyme definitions

Cycle N+1:
  Coder gets: the SAME requirements + system prompt → produces similar code
```

The fitness signal flows to the evolver, which modifies graph topology and enzyme definitions — not code. The coder, which is the only actor that produces code, receives zero signal about what was wrong with its previous output. The thing that can fix the problem never learns what the problem is.

## Evidence

This is visible in the sudoku and chess challenges. The cycle plateaus at 83% (5/6 acceptance tests) regardless of which model serves as the coder. Swapping Codex gpt-5.4 for Kimi k2.5 produces the same fitness — because the bottleneck is not model capability, it is the absence of failure information in the coder's input.

The acceptance test code lives in `Challenge::acceptance_test` and is appended to the produced artifact before compilation in the sandbox. The sandbox returns compilation success/failure, test pass counts, and stderr/stdout. But none of this output flows back into the coder's prompt for the next cycle. The `sandbox::evaluate_rust_code` result contains `compiled`, `tests_passed`, `stdout`, and `stderr` — the information exists, it is simply never routed to the coder.

Consider what the coder would need to hear:

- "Compilation failed: expected `[[u8; 9]; 9]` but got `Vec<Vec<u8>>`" — the coder can fix a type mismatch.
- "Test `hard_puzzle_solve` failed: `solve()` returned `None` for this input" — the coder can improve its solver.
- "5/6 tests passed, 1 failed: `test_check_detection` — `is_in_check()` returned `false` for position where black king is attacked by white bishop" — the coder can fix the check detection logic.

Without this, the coder is flying blind. It produces code from the same requirements, hits the same wall, and the cycle stalls.

## Diagnosis

The catalytic graph has a missing edge. In RAF (Reflexively Autocatalytic and Food-generated) terms, the sandbox produces an output — the failure report — that is food for the coder's next invocation. But this food is never routed. The catalytic closure is incomplete: the sandbox catalyzes evaluation, but its product does not feed the coder's catalysis of code.

The evolver receives the fitness score, but the evolver operates on the wrong level of abstraction. It mutates enzyme definitions (which model to use, what system prompt to give, how to decompose the task). It cannot fix a type mismatch in generated code. It cannot add an edge case to the solver logic. Only the coder can do that — and only if it knows what failed.

This is why model-swap invariance occurs (see sibling learning). Two models of vastly different capability produce the same fitness because both receive the same incomplete input. The ceiling is not model capability; it is the information the cycle provides to the model.

## The Fix

Add a `test_results` artifact to the catalytic graph: an edge from the sandbox back to the coder.

```
Cycle N:
  Coder gets: requirements + system prompt → produces code
  Sandbox: compiles + runs tests → fitness score + failure details
  Evolver gets: fitness report → modifies enzyme definitions

Cycle N+1:
  Coder gets: requirements + system prompt + FAILURE DETAILS FROM CYCLE N
    → produces improved code informed by specific errors
```

Concretely:

1. **Capture structured failure output** from `sandbox::evaluate_rust_code`: compilation errors (stderr), test failure messages (stdout with assertion details), which tests passed and which failed by name.

2. **Store failure details as a cycle artifact** in the metabolism, alongside the fitness score. This is not a prompt hack — it is a first-class artifact in the catalytic graph, with a defined type, producer (sandbox), and consumer (coder).

3. **Inject failure context into the coder's prompt** on subsequent cycles. Format it as actionable feedback: "Previous attempt scored 83%. The following test failed: [test name]. Error output: [stderr/stdout excerpt]. Fix the identified issue while preserving passing behavior."

4. **Preserve holdout secrecy**: The coder should see the failure *output* (error messages, assertion failures) but not the acceptance test *source code*. The test source remains hidden. The coder learns "your function returned None for a hard puzzle" but not the specific test input or the test implementation. This maintains the holdout property while providing actionable signal.

5. **Bound the feedback window**: Include failure details from the most recent cycle only, not the full history. Accumulating failure context across many cycles will bloat the prompt and may confuse the model with stale information from architecturally different attempts.

## Why This Is Critical

Without this edge, the catalytic cycle cannot improve code quality across cycles. The evolver can rearrange the graph, swap models, and modify system prompts — but none of these address a specific bug in the generated code. The cycle degenerates into repeated independent one-shot attempts with the same input, which is strictly worse than a single one-shot attempt (it adds overhead without adding information).

This is the difference between evolution and repetition. Evolution requires selection pressure that is *legible to the thing being selected*. A fitness score visible only to the evolver is selection pressure on the graph topology. Failure output visible to the coder is selection pressure on the code. Both are needed. Currently only the former exists.

## Generalization

This pattern — fitness signal routed to the wrong actor — will recur in any catalytic system where evaluation and production are separated. The principle: **feedback must flow to the actor with the agency to act on it**. If the sandbox detects a type error, the coder must hear about it. If the observer detects a structural invariant violation, the evolver must hear about it. Routing all feedback to a single actor (the evolver) creates an information bottleneck that no amount of model capability can overcome.

## Related

- `docs/solutions/architectural-insights/catalytic-cycle-bottleneck-is-cycle-not-model-2026-04-04.md` — sibling finding: the bottleneck is the cycle, not the model. This learning identifies the specific missing edge that causes that bottleneck.
- `docs/solutions/best-practices/fitness-gated-evolution-holdout-raf-lineage-2026-04-01.md` — the holdout benchmark and fitness gating system. The feedback loop proposed here complements fitness gating: gating prevents regression, feedback enables improvement.
- `docs/solutions/best-practices/acceptance-test-coverage-one-test-is-not-testing-2026-04-02.md` — acceptance test design. More tests increase the resolution of the fitness signal, but without routing that signal to the coder, higher resolution does not help.
- `docs/solutions/runtime-bugs/metabolism-cross-cycle-dedup-kills-evolution-2026-04-01.md` — another cycle-level information flow bug where deduplication silently killed evolution across cycles.
