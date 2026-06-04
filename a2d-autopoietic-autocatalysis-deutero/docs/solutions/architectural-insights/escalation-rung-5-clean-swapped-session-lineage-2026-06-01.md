---
title: "Escalation Rung 5 Needs Provider-Visible Lineage"
date: 2026-06-01
module: metabolism
tags:
  - escalation
  - provider-routing
  - lineage
  - autonomy
problem_type: architectural-insight
---

# Escalation Rung 5 Needs Provider-Visible Lineage

## Problem

Rung 5 is the clean-session variant of the rung-4 provider swap: once the same enzyme remains stuck after swapped-provider-with-history, the next intervention should ask the alternate provider to solve from a clean context.

The implementation already computed `clean_session` for rung 5+, but rung 5 was not explicit in tests or lineage. Worse, invocation lineage recorded the scheduler inputs rather than the provider-visible inputs, so a clean-session invocation could still appear to include `failure_report` even though it had been stripped before request construction.

That makes live validation ambiguous: a monitor cannot tell whether rung 5 actually ran clean, or merely reached a high loop count.

## Fix

Rung 5 now has explicit mechanical evidence:

- `InvocationLineage` records `escalation_rung`, `provider_swap`, and `clean_session` for each invocation;
- clean-session lineage records the provider-visible inputs after stripping `failure_report`;
- provider-health `recent_invocations` includes the same escalation fields so downstream evolver/architect prompts can see whether failures happened under normal routing, swapped routing, or clean swapped routing;
- topology comparison output annotates escalated invocations, for example `{rung 5, swap, clean}`;
- trace routing distinguishes rung-5 clean provider swaps from rung-4 history-preserving swaps.

## Coverage

Added tests proving:

- rung 5 invokes the swapped provider, marks `provider_swap`, marks `clean_session`, records rung 5, and omits `failure_report` from provider-visible lineage inputs;
- rung-5 request construction contains both provider-swap and clean-session notices while excluding previous-failure injection and consultation text;
- topology lineage formatting prints escalation flags;
- provider-health recent invocation records include escalation fields.

Validation:

```text
cargo test
37 CLI tests + 142 core tests + 11 bootstrap + 7 provider + 1 doctest = 198 passing, 2 ignored
```

## Why this matters

Escalation mechanisms are only useful if the system can later distinguish which intervention produced an outcome. Recording provider-visible inputs and rung metadata turns rung 5 from an implicit branch into inspectable metabolic evidence. That evidence is also required before implementing rung 6, because multi-model consensus needs to know when simpler interventions have already failed.
