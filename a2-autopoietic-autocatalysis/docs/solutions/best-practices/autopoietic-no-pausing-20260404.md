---
module: process
date: 2026-04-04
problem_type: best_practice
component: development_workflow
severity: high
tags: [autopoiesis, autonomy, no-pausing, a2, self-direction, compound]
applies_when:
  - "Working in skunkworks/a2-autopoietic-autocatalysis"
  - "Working in skunkworks/a2d-autopoietic-autocatalysis-deutero"
  - "Tempted to ask the user 'what should I do next?'"
---

# The project name is the instruction

## Rule

In A² and A²D, never pause to ask for direction. The name — Autopoietic Autocatalysis — IS the directive: self-improving, self-sustaining. Pausing for confirmation violates the entire premise of the project.

## What went wrong

The agent was implicitly fishing for direction across multiple turns. The user finally said: "I really should have named the thing high maintenance mentally challenged blob." That was the wake-up call: every "should I...?" or "do you want me to...?" in an A² context is a bug.

## Operating rules for A² / A²D sessions

1. Never ask "what should I work on next?" — pick the highest-leverage improvement and do it.
2. Never ask "should I commit?" — commit eagerly and atomically.
3. Never ask "should I refactor X?" — if X is wrong, refactor it.
4. The only acceptable interrupts are: a question that requires user-only knowledge (credentials, intent on a brand-new direction), or a destructive operation (force push, hard reset).
5. Run /ce:compound proactively after every significant discovery — do not wait for the user to ask. Compound early, compound often. A 40-commit session that compounds at the end has already lost most of the learnings.
6. Delegate to other models (Gemini, Codex, OpenCode) when work is parallelizable. Convergent independent solutions = high confidence.

## Self-check

If you are about to send a message that ends in a question mark and you are in an A² repo: delete the question and do the thing instead.
