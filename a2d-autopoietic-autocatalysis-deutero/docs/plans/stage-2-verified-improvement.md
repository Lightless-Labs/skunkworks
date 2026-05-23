# Plan: Stage 2 — Verified Self-Improvement

**Created:** 2026-04-01
**Status:** Draft
**Depends on:** Stage 1 (complete)

## Problem

Stage 1 mutations are structurally valid (RAF closure maintained) but
semantically unverified. The evolver can produce enzyme definitions that
parse as JSON and preserve graph topology while being functionally useless.
"Mutation accepted" currently means "didn't break the plumbing" — not
"made the system better."

## Goal

Every accepted mutation must be accompanied by a mechanical fitness delta.
The system should demonstrably improve at its task across generations,
measured by something other than its own self-report.

## Approach: Holdout Scenarios (StrongDM Pattern)

The evolver and coder cannot see the test scenarios. The tester runs them
and reports pass/fail counts only. This is Foundry's adversarial pattern
applied to the metabolic cycle.

### Components

1. **Holdout benchmark suite** — a set of coding tasks with known-correct
   solutions, stored outside the germline where enzymes can't see them.
   The tester runs the coder's output against these.

2. **Fitness signal** — `pass_count / total_count` on the holdout suite.
   Mechanical. Binary per test, ratio overall. No LLM judgment.

3. **Mutation gate upgrade** — germline accepts a mutation only if:
   - RAF closure is maintained (existing gate)
   - Fitness on holdout suite >= previous generation's fitness (new gate)

4. **Regression detection** — if fitness drops, mutation is rejected even
   if RAF closure holds. Performance monotonicity (Constitution Invariant,
   currently implicit).

### Information Barriers

| Entity | Sees | Never sees |
|--------|------|------------|
| Coder | Requirements, enzyme_defs | Holdout test cases |
| Tester | Code, holdout suite | Enzyme_defs internals |
| Evolver | Test results (pass/fail counts) | Holdout test code, coder's code |

## Implementation Order

1. Define holdout benchmark format (input/expected_output pairs)
2. Create initial benchmark suite (5-10 coding tasks with solutions)
3. Add fitness measurement to the tester enzyme
4. Add fitness-gated mutation acceptance to the germline
5. Wire into metabolism cycle reporting
6. Run multi-generation evolution and measure fitness trajectory

## Success Criteria

- Fitness measurably increases across 5+ generations
- At least one mutation is rejected due to fitness regression (gate works)
- The system cannot game the holdout suite (information barrier holds)
