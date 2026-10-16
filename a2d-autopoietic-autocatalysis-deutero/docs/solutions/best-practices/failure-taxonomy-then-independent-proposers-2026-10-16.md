---
title: "Mine a Failure Taxonomy, Then Ask Several Independent Proposers; Convergence Picks What to Test"
date: 2026-10-16
category: best-practices
module: a2d-lift / nano
problem_type: process_lesson
component: experiment-design
symptoms:
  - "Remaining failures are shared misreadings and no selector can fix them"
  - "A single proposer's ideas cannot be ranked before spending evaluation budget"
  - "A proposed candidate may depend on an instrument capability that does not exist"
root_cause: unranked_proposals_and_unverified_instrument_assumptions
---

# Failure Taxonomy, Then Independent Proposers

**Evidence:** LOG C2-FA2 (failure taxonomy) and "Proposer round c1" (E009, E010 registered; c1a-1 declined). Builds on `selector-at-ceiling-moves-bottleneck-to-generation-2026-10-16.md`.

## The practice

1. **Diagnose from spec, code and pass/fail label only** (train-label rule). Every file is labelled "fails at least one hidden test"; no hidden content enters the diagnosis.
2. **Aggregate to task-agnostic patterns.** Per-task content never reaches a harness.
3. **Give only the aggregate to several independent proposers with the same brief.** Here, three Opus proposers each proposed 3 candidates from separate copies.
4. **Treat convergence as a cheap signal** of which ideas to spend evaluation budget on.
5. **Check every proposal's mechanism against the instrument before spending.**

## The numbers

- 34 diagnoses over 26 distinct tasks; the misreading was shared by every file in **29/34**.
- Categories: boundary / edge case 9, semantics / state model 9, performance / complexity 8 (the 8 timeouts), precedence 3, exact format 3, API contract 1, overflow 1.
- Convergence: **3/3** proposed a failure-class checklist (became E009); **2/3** proposed two readings per call, a hedge file taking the other reading (became E010).

## Lessons

- **Convergence across independent proposers ranks ideas for free.** It does not prove an idea works; it says where to look first.
- **Verify the mechanism before spending.** c1a-1 (probe tests) was declined: probes appended to a file would never run, because `public_test` runs only the public test module (filter `a2d_public_<nonce>::`). The harness would have silently degraded to E002 with 3 samples.
- **Record such limits as instrument ideas.** A controller op that runs harness-supplied tests (no hidden content) would make strong-reader probes testable.
- **Disclosure:** the checklist classes were mined from E002's dev failures, so dev estimates of failure-mined checklists are optimistic. The dev-holdout is the check.
- **Scope:** exploratory. The taxonomy comes from subagent diagnoses (confidence high 30, medium 3, low 1), and E009/E010 had not been evaluated when this was written.
