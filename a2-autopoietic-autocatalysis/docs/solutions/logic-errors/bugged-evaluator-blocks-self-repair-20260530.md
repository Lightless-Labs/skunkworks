---
module: evaluation
problem_type: logic_error
component: governor
severity: high
date: 2026-05-30
tags: [self-repair, evaluator, verifier, promotion, a2]
applies_when:
  - "A task fixture mutates the evaluator or scorer that decides promotion"
  - "Candidate worktree verifiers pass but the mutable evaluator marks the task incomplete"
---

# A bugged mutable evaluator can block its own repair

## Symptom

`compound-eval-same-crate-hidden` repeatedly produced candidate patches that touched `crates/a2_eval/src/seed.rs` but were discarded. An inspectable Pi/ZAI attempt showed:

- Candidate task verifier passed in the candidate worktree.
- The captured patch fixed both injected regressions.
- Applying the captured patch manually made both verifier tests pass.
- The persisted `FitnessRecord` still had `somatic.task_completed=false`, so the Governor discarded the patch and `a2ctl run --apply` never applied it to the task workspace.

## Cause

The fixture intentionally corrupted `SeedEvaluator::check_budget()` in `a2_eval`. That same mutable evaluator scored the candidate patch. Even though the candidate worktree verifier passed, the bugged evaluator reported the candidate incomplete because it used the injected inverted budget predicate.

This is a self-reference failure: the component being repaired was also the component trusted to decide whether the repair could be promoted.

## Fix

The Governor now has an independent external-verifier backstop for Stage 0 promotion. It may promote a candidate when all of these are true:

1. A patch exists.
2. The task has explicit candidate worktree verifier records.
3. All candidate worktree verifiers passed.
4. Candidate test results have zero failures.
5. The outer Governor token budget still permits the patch.

This does not make prompts or model claims authoritative. It makes system-run candidate verifiers authoritative when the mutable evaluator disagrees.

## Regression test

`candidate_verifier_backstop_promotes_when_mutable_evaluator_is_corrupt` uses a catalyst that returns a verified patch and an evaluator that always reports `task_completed=false`. The Governor must still choose `PromoteGermline` because the independent verifier gate passed.

## Rule

When A² repairs its own evaluator, never rely solely on that evaluator's mutable self-report. A separately executed verifier must be able to carry the candidate through to the outer apply/rebuild gate.
