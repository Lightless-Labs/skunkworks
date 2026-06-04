---
title: "Escalation Rungs Need a Deterministic Live Validation Harness"
date: 2026-06-04
module: metabolism
tags:
  - escalation
  - validation
  - provider-routing
  - cli
problem_type: best-practice
---

# Escalation Rungs Need a Deterministic Live Validation Harness

## Problem

Rungs 4–6 were unit-tested, but live validation depended on waiting for organic repeated provider behavior. That made the safety and observability contract hard to verify under bounded time: provider swap, clean-session stripping, and rung-6 candidate evaluations might all work in tests while remaining unproven under the real provider registry.

There was also a naming-risk boundary: internal scheduling variables can use loop-counter language, but provider-health reports and CLI JSON should expose stable external fields such as `escalation_rung`.

## Fix

Add a diagnostic-only validation path:

- `Metabolism::force_escalation_rung_for_validation(enzyme_id, rung)` accepts only rungs 4–6, validates the enzyme exists, and mutates only in-memory escalation state.
- `a2d validate-escalation <challenge> [enzyme]` runs fresh metabolism instances for rungs 4, 5, and 6 with the real runtime registry.
- Persistence is disabled by construction: no lineage commits, no accepted patch application, and no durable provider-policy writes.
- The CLI emits JSON using `escalation_rung`, `provider_swap`, `clean_session`, `candidate_evaluations`, and `provider_policy_changed`.
- Failure-history visibility is checked with a non-empty seeded marker, so empty food artifacts cannot create a false positive.

## Validation

Bounded live smoke:

```bash
A2D_PROVIDER_TIMEOUT_SECS=1 A2D_MAX_CYCLE_SECS=1 A2D_RUNG6_MAX_PROVIDERS=2 \
  cargo run -q -p a2d -- validate-escalation sudoku coder
```

Observed:

- rung 4 routed to the swapped provider and preserved the seeded failure marker;
- rung 5 routed to the swapped provider and stripped the marker via clean session;
- rung 6 invoked the bounded Kimi + DeepSeek provider portfolio and recorded two candidate evaluations;
- all provider calls timed out under the intentional 1s bound;
- provider policy stayed unchanged;
- emitted JSON exposed `escalation_rung` and did not expose internal counter names.

Full validation after implementation: `cargo test` passes with 205 passing tests and 2 ignored integration tests.

## Lesson

For adaptive mechanisms, unit tests are not enough. Add a bounded diagnostic lane that can force rare internal states, run through the real registry, and emit machine-readable evidence without mutating durable system state.
