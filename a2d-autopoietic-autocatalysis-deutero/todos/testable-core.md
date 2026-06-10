# Testable Core: Mock Everything, Prove Logic Before Live Runs

**Created:** 2026-04-04
**Reviewed:** 2026-06-10 — Most originally missing mock surfaces are now implemented in `crates/a2d-core/src/metabolism.rs`: fitness degradation tracking, failure-report prompt injection, rungs 4/5/6 routing, highest-fitness portfolio selection, and protected-file `SystemPatch` rejection through the metabolism path. New CLI integration coverage for `score-artifact` lives in `crates/a2d-cli/tests/score_artifact.rs`. Remaining gap: `self_sandbox` directly covers valid/breaking patch acceptance/rejection, but there is still no focused mock metabolism test proving an eligible non-noop architect `SystemPatch` is accepted end-to-end into the pending-patch queue.
**Context:** Live challenge runs take 10+ minutes per 3-cycle run and depend on flaky external providers. The metabolism logic (scheduling, fitness evaluation, loop detection, escalation) should be testable with mock providers in under a second.

## What Already Exists

`metabolism::tests` already has `MockProvider` — returns canned responses, tracks call count. The existing tests (routes_artifacts_and_turns_the_cycle, kills_prefailure_workcells, rejects_closure_breaking_mutations) prove scheduling, routing, and gating work.

## What's Missing / Current Coverage

### 1. Mock-based fitness degradation test — IMPLEMENTED
`metabolism::tests::fitness_degradation_tracked_across_cycles` now exercises degradation tracking.

### 2. Mock-based architect validation test — PARTIAL
`self_sandbox` has direct coverage for protected-file rejection, ineligible-file rejection, valid patch acceptance, and breaking-patch rejection. `metabolism::tests::architect_noop_output_is_successful_patch_record` covers noop parsing/routing. `metabolism::tests::architect_system_patch_to_protected_file_is_rejected_through_metabolism` now proves a mock architect `SystemPatch` is parsed by the metabolism, routed into `self_sandbox::validate_patch`, rejected by the protected-file gate, recorded in invocation lineage, and not added to `pending_patches()`. Still missing: a focused metabolism test where an eligible non-noop `SystemPatch` is accepted end-to-end into `pending_patches()`.

### 3. Mock-based escalation ladder tests — IMPLEMENTED FOR CURRENT RUNGS
Current coverage includes loop/escalation prompt behavior, rung 4 provider swap, rung 5 clean swapped session, rung 6 consensus candidate selection, and role/scope isolation (`rung_4_*`, `rung_5_*`, `rung_6_*`).

### 4. Mock-based feedback loop test — IMPLEMENTED
Coverage now includes `feedback_loop_populates_failure_report_on_benchmark_failure`, `coder_prompt_includes_failure_report_when_available`, and `feedback_loop_empty_failure_report_on_perfect_fitness`.

### 5. CLI replay integration — IMPLEMENTED
`crates/a2d-cli/tests/score_artifact.rs` covers file-path and stdin replay through the real binary, verifies hidden-holdout failure, nonzero exit, and diagnostic redaction.

## Principle

The interfaces are NOT set in stone — the system evolves them as it works on real tasks. But the *logic* that decides when to escalate, what to feed back, which enzyme fires next, whether a patch is safe — that's testable with mocks and should be proven before live runs.

Live runs validate outcomes. Tests validate logic.

## Implementation

Add tests to `metabolism::tests` using the existing `MockProvider` pattern. Each test:
1. Sets up a germline with the right enzyme topology
2. Provides mock responses that trigger the behavior under test
3. Runs 1-3 cycles
4. Asserts the metabolism's state (fitness, artifacts, lineage, patches)

No new infrastructure needed — just more tests with the existing mocks.
