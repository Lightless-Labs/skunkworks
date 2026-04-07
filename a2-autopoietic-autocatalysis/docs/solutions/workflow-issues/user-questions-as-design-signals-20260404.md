---
module: process
date: 2026-04-04
problem_type: workflow_issue
component: development_workflow
severity: high
tags: [user-feedback, refactor, signals, listening, design]
applies_when:
  - "User asks a short question instead of giving a directive"
  - "User responds with '?' or a one-line probe"
  - "Mid-execution interruptions from the user"
---

# User questions are usually load-bearing — treat them as refactor triggers

## Rule

When the user asks a short, pointed question instead of issuing a command, stop and treat it as a signal that something is fundamentally wrong with the current direction. Do not answer it as trivia.

## What happened

Two user questions in this session each collapsed an entire subsystem:

1. "should benchmark output code BE a permanent part of the project itself?"
   -> removed --apply, deleted 219 lines, eliminated the duplicate-definition class of bugs entirely.

2. "If you want to actually benchmark it you should run a frontier model against the model running in the system against the system running the model"
   -> introduced baseline.sh comparison harness; reframed what the benchmark even measures.

A third — "?" — flagged editorial creep in HANDOFF.md and led to a tone correction.

In each case the user was not asking for information. They were pointing at a load-bearing assumption the agent had stopped questioning.

## What to do instead

When the user asks a question mid-stream:

1. Stop the current task. Do not answer-and-continue.
2. Re-read the last 3-5 commits and the file the question targets.
3. Ask: "what assumption am I making that this question would dissolve?"
4. If you find one, surface it explicitly before doing anything else: "you're pointing at X — should I rip it out?"
5. Prefer the larger refactor over the local fix. The user's question is almost always cheaper than the path you were on.

## Anti-pattern

Treating "should X be Y?" as a yes/no question and answering "yes" or "no" without changing course. If the user thought the answer was obvious they would not have asked.
