---
title: Read the code before offering takes — HANDOFF is not ground truth
date: 2026-04-16
module: process
problem_type: workflow_issue
component: session-discipline
severity: medium
applies_when:
  - A session opens with a HANDOFF doc summarising project state
  - The assistant is asked for "takes" or prioritisation
tags:
  - handoff
  - grounding
  - research
  - opinion
---

# Read the code before offering takes — HANDOFF is not ground truth

## Symptom

Session 2026-04-16 opened by reading HANDOFF.md and immediately offering three prioritised takes on the project. The user pushed back: *"Did you take a look at the project itself?"* — because the takes were derived entirely from the handoff narrative, not from the code.

When the code was actually explored, the picture changed materially:

- HANDOFF said the #1 priority was designing a loop-shaped benchmark. **The real #1 was wiring `ContextPack`**, because no loop-shaped benchmark could produce meaningful results until prior attempts were actually visible to the catalyst.
- HANDOFF's "61 tests, 6/6 sentinels" claim was partially stale: lockfile sentinel was failing (pre-existing).
- A first-pass Explore agent also got a detail wrong ("lineage store is wired nowhere in a2ctl") — actually it *is* wired. Grounding requires checking multiple sources, not just delegating.

## Rule

When opening a session, **HANDOFF is a starting point, not a verdict**. Before offering any strategic opinion on what to do next:

1. Read HANDOFF to understand the author's model of the state.
2. Verify the claims that matter by touching the code — at minimum the components HANDOFF's next-step recommendations depend on.
3. Treat surprises as the most valuable output of grounding: places where the code disagrees with the doc are where the real decisions live.

An Explore agent pass is cheap; a strategic opinion based on an unverified handoff is expensive, because it chains further decisions onto a stale premise.

## Counter-rule

Don't overcorrect into paralysis. The rule isn't "read every file before saying anything"; it's "before recommending where to spend the next block of work, spot-check the premises the recommendation relies on". For a quick factual question, HANDOFF plus memory is usually enough.

## Confirmation

The ContextPack wiring that shipped this session (commit c32b657) was invisible from HANDOFF. It only surfaced when the code was actually walked. The user's correction — *"Well, duh"* — should be taken as a durable instruction: grounding comes before takes.
