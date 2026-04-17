---
title: ContextPack is a load-bearing extension point — empty population silently invalidates every loop-shaped benchmark
date: 2026-04-16
module: a2-workcell
problem_type: best_practice
component: runtime
severity: high
applies_when:
  - A struct designed as an "extension point" is always constructed with empty/default values
  - Downstream benchmarks or loops assume that extension point is populated
  - The plumbing exists (storage, retrieval APIs) but no code writes into the struct
tags:
  - context-pack
  - lineage
  - governor
  - benchmark
  - extension-point
  - dead-interface
---

# ContextPack is a load-bearing extension point — empty population silently invalidates every loop-shaped benchmark

## Symptom

Prior to 2026-04-16, `ContextPack` in `a2_core::protocol` had fields — `prior_attempts: Vec<LineageId>`, `retrieved_motifs: Vec<String>`, `relevant_files: Vec<PathBuf>` — that were *always* built empty at the only construction site that mattered (`a2_workcell::runtime::run_workcell`, line 47). The `LineageStore` trait existed, was implemented by `SqliteLineageStore`, was wired into `Governor::with_lineage_store()`, and was actually attached in `a2ctl` main. Lineage records were being persisted on every run. But nothing ever *read* from the store to populate `ContextPack`.

Symptom upstream: every proposed "loop-shaped benchmark" on the HANDOFF priority list (multi-round, self-correction, cross-task transfer, SWE-bench Lite, adversarial drift) would have produced a null result for the *wrong reason* — not "A² doesn't help", but "A²'s memory across rounds was never wired, so each round was effectively a single-pass run with extra ceremony".

## Why this kind of gap is easy to miss

1. **All five surrounding components looked healthy.** The struct existed. The trait existed. The store was wired. Tests exercised each component in isolation. The only missing piece was the single call site that would have loaded prior records and passed them into `WorkcellConfig`.
2. **The field names lied about the system's behaviour.** A reader skimming `ContextPack` would reasonably assume `prior_attempts` meant "attempts the governor has seen for this task". It did not. It was an empty vector that was never written.
3. **The handoff was optimistic about readiness.** HANDOFF.md listed loop-shaped benchmarks as the #1 next step with no caveats about plumbing gaps. An Explore agent pass over the code was what surfaced the empty-ContextPack issue; a read of HANDOFF alone would have missed it.

## Fix (commit c32b657)

Three small edits:

1. `WorkcellConfig` gained `prior_lineage: Vec<LineageRecord>`.
2. `run_workcell` populates `ContextPack.prior_attempts` (IDs) and `retrieved_motifs` (compact per-attempt summary: `attempt N [provider/model]: task_completed=…, tests_pass=…, tokens=…, duration=…s`) from `prior_lineage`.
3. `Governor::run_task` queries `lineage_store.for_task(&task.id)` when a store is wired (non-fatal on error) and passes the result into `WorkcellConfig`.

`WorktreeCatalyst::build_prompt` now renders motifs under a "Prior Attempts on This Task" section so the model actually sees them.

End-to-end test: `prior_lineage_surfaces_as_attempts_and_motifs` asserts the full flow via a `CapturingCatalyst`.

## Rule

**If a struct is designed as an extension point, build a failing test that asserts it's populated from the source of truth *before* you claim the surrounding system is ready.** For `ContextPack`, the missing test was: "given a lineage record in the store for task T, running task T again causes the catalyst to see a non-empty `prior_attempts`." Without that assertion, every higher-level benchmark would report results that look like capability measurements but are actually measuring an empty prompt field.

More generally: a type-system extension point with no integration test that exercises the populate→consume path is dead until proven otherwise. Don't reason about behaviour from field names; reason from the call graph.

## Related gaps flagged during the same session

- `crates/a2d/src/governor.rs` is not declared as a module in `lib.rs` — it's a 300-line dead-code shadow of the real `Governor` in `lib.rs`. Delete or promote.
- `StrategyChange::DecomposeTask` and `::RaiseTemperature` are returned by the stagnation detector but never acted on; only `SwitchModel` branches in `a2ctl` (`main.rs:368`). Same shape of bug: the enum implies agency the code doesn't have.
- Prior-attempt motifs currently render pass/fail/tokens/duration but not *rationale* or *diff*. The `PatchBundle.rationale` field is not persisted alongside the `LineageRecord`, so the model learns *that* a prior attempt failed, not *why*. Enriching this will matter for self-correction benchmarks specifically.

## Confirmation

Smoke-tested 4/4 providers end-to-end after the wiring change: gemini, opencode/zai-coding-plan/glm-5.1, opencode/minimax-coding-plan/MiniMax-M2.7, opencode/kimi-for-coding/k2p5. All produced patches, all promoted, no regressions. 62 unit tests pass (up from 61 with the new integration test).
