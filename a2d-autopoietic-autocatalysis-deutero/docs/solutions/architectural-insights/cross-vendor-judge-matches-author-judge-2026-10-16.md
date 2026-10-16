---
title: "A Cross-Vendor Judge Matches the Task-Author Judge: The Selection Lever Is Not Vendor-Specific"
date: 2026-10-16
category: architectural-insights
module: a2d-lift / nano
problem_type: research_finding
component: harness-design
symptoms:
  - "The live judge (Sol) also authored the tasks, so its selection accuracy might be an authorship advantage"
  - "No cross-family evidence that sample-then-judge selection works outside one vendor"
root_cause: authorship_confound_between_task_author_and_judge
---

# A Cross-Vendor Judge Matches the Task-Author Judge

**Evidence:** LOG C2-E2 (offline, `runs/2026-10-16-c2-claude/judge-score.json`). Follow-up to `selector-at-ceiling-moves-bottleneck-to-generation-2026-10-16.md`.

## The setup

- Cases: the 68 decisive tuples in E002's three runs (at least 2 distinct public-green files that split on hidden tests; 38 with 4 files, 21 with 3, 9 with 2).
- Judge: Claude Opus 5.5 subagents (8 batches), given E002's exact judge prompt, candidates in a fresh seeded shuffle. Answer key kept outside the staged directory.
- The bar (>= 64/68) was registered before any result.

## The finding

| Judge | Hidden-passing pick / 68 |
|---|---|
| Sol (live; authored the tasks) | 68 |
| Claude Opus 5.5 | 66 (0.971, Wilson 95% CI 0.899-0.992) |
| Random pick (expected) | 36.3 |

- By file count: 2 files 8/9, 3 files 21/21, 4 files 37/38. Misses: `scc-graph-l3`, `cron-next-l1`.
- Consistent with C2-E1 on the S0 pool (Sonnet 37/39).

## Lessons

- **Test an authorship confound offline before building a new blind task set.** Re-judging existing decisive tuples took no live calls and bounded the confound at once.
- **The remaining author advantage is at most about 3 pp of pick rate** on decisive tuples (2 of 68 picks).
- **The selection lever is not vendor-specific.** A judge from another vendor that did not author the tasks recovers nearly all of the selection headroom.
- **A cross-family judge is a ready candidate once a lane exists.** It would also take load off Sol. The Claude CLI is not usable from the session sandbox, so this stays a candidate.
- **Disclosure:** the subagents were instructed to read only the staged directory and run nothing. Their tool use was instruction-limited, not machine-audited.
- **Scope:** descriptive, no keep decision. It covers the judge mechanism on E002's tuples, not a full cross-family harness run.
