---
module: docs
date: 2026-04-04
problem_type: workflow_issue
component: documentation
severity: medium
tags: [handoff, documentation, discipline, interpretation]
applies_when:
  - "Updating HANDOFF.md or other handoff documents"
  - "Writing session summaries from limited data"
  - "Reporting benchmark or experiment results"
---

# Handoffs state facts, not interpretation

## Rule

HANDOFF.md is a state file, not an essay. Write what happened, what is true now, and what to do next. Do NOT write what it means.

## What went wrong

After a single benchmark run with one model on five tasks, the handoff was updated with:

> "the baseline result proves the current benchmark tests the wrong thing"

The user replied: "?". The line was wrong on two axes:
1. One run cannot prove anything (see `single-run-conclusions-20260404.md`)
2. Even if it could, "proves the wrong thing" is interpretation that belongs in an analysis doc, not a handoff

## What to do instead

- Facts only: "task 014 fails with X. Tasks 001-013, 015 pass."
- Next step as imperative: "investigate why task 014 fails."
- No "proves", "demonstrates", "confirms", "reveals" verbs unless backed by replicated data.
- If you feel the urge to narrate meaning, write a separate analysis doc and link it.

## Self-check before saving HANDOFF.md

1. Could a future agent contradict any sentence with one counter-example? If yes, weaken it.
2. Does any sentence editorialize ("the real problem is...", "this proves...")? Strip it.
3. Is every claim grounded in something a future session can reproduce? If no, remove or qualify.
