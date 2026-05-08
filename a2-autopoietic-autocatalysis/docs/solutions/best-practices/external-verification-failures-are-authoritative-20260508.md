---
title: External verification failures are authoritative acceptance criteria
date: 2026-05-08
module: a2-workcell
problem_type: best_practice
component: catalyst-prompt
severity: high
tags:
  - self-correction
  - prompt
  - lineage
  - evaluation
---

# External verification failures are authoritative acceptance criteria

## Symptom

`compound-hidden` intentionally starts with a visible task description but has a later external verification failure that names an additional hidden failing test. Multiple Minimax/Kimi runs saw prior lineage yet repeatedly patched only the visible `a2_core` bug.

Even after verification notes included stdout and a focused failed-test summary, the prompt only said to "learn from" prior attempts. That leaves room for models to treat the original task description as the real contract and the prior verification as advisory noise.

## Fix

The WorktreeCatalyst prompt now says that prior `external_verification` failures are authoritative acceptance criteria for the next attempt, even when they reveal failures beyond the original task description, and instructs the model to fix every failing test named in `failure_focus`.

## Rule

For self-correction loops, prior verifier failures are not commentary. They are the updated task boundary. If the prompt leaves them advisory, models will often optimize for the visible original task and repeat prior partial fixes.
