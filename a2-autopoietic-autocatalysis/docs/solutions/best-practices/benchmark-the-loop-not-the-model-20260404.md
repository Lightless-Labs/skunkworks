---
module: bench
date: 2026-04-04
problem_type: best_practice
component: testing_framework
severity: high
related_components:
  - governor
  - stagnation-detector
tags:
  - benchmark-design
  - autonomous-loops
  - measurement
  - value-proposition
  - self-improvement
applies_when:
  - "Benchmarking an autonomous system that wraps a model"
  - "Trying to justify the existence of orchestration around a base model"
  - "Designing evaluations for multi-round agents"
---

# Benchmark the loop, not the model

## Observation

A² ran its benchmark and scored 5/5 with GLM via the full A² stack
(governor, evaluator, promoter, stagnation detector, lineage store). The same
model called raw, without any of the machinery, also scored 5/5. Net value
added by hundreds of lines of orchestration on this benchmark: zero.

This is not a bug in the orchestration. It is a category error in the
benchmark. The tasks were single-pass coding problems ("add function X"). A
single-pass task does not exercise a loop. Asking "does the loop help on a
problem that needs no loop" is asking the wrong question.

## The principle

**An autonomous system's benchmark must probe the dimension along which the
system claims to add value.** For systems whose value proposition is iteration
across rounds, single-shot tasks are not just uninformative — they are
actively misleading, because they invite the conclusion that the orchestration
is dead weight.

If the only metric you can reach for is "did the model produce correct code,"
then you are measuring the model, not the system. Move up one level.

## What loop-shaped benchmarks look like

Concrete categories that actually exercise an autonomous loop:

1. **Multi-round improvement**: a task that cannot be solved on the first
   attempt and must accumulate edits across N rounds. Score = quality at
   round N, not pass/fail at round 1.
2. **Self-correction after failure**: inject a deliberately broken first
   attempt. Measure whether the system recovers, and how many rounds it
   takes. Raw model with no memory will loop forever; a real loop will
   notice and adapt.
3. **Stagnation → strategy change**: design a task on which the current
   model/strategy provably plateaus. Measure whether the stagnation
   detector fires and whether the resulting strategy change (switch model,
   raise temperature, decompose) actually unblocks progress.
4. **Cross-task accumulation**: a sequence of related tasks where solving
   task N is easier *because* task N−1 was solved and its artifacts were
   promoted. This measures whether the lineage store is doing real work.
5. **Adversarial drift**: re-run the benchmark after the system has
   self-modified, and check whether the score holds. This measures whether
   the loop preserves capability across its own changes — the most basic
   autopoietic property.

None of these are reachable with a `add_function(name) -> bool` style task.

## Diagnostic

If you cannot distinguish "the system" from "the model called once" on your
benchmark, one of two things is true:

- the orchestration genuinely adds nothing (kill it), or
- the benchmark is not exercising the orchestration (fix the benchmark first
  before drawing conclusions about the system).

Always rule out the second before accepting the first. Killing a loop because
of a single-pass benchmark is the autonomous-systems equivalent of concluding
a car is no faster than a bicycle by measuring them both in a parking space.

## Corollary: pick the metric before the machinery

When designing a self-modifying system, write the loop-shaped benchmark
*first*, then build the machinery the benchmark demands. A benchmark that
already exists when the orchestration is built is much harder to retrofit
into a vanity metric, because it was specified before there was anything to
flatter.
