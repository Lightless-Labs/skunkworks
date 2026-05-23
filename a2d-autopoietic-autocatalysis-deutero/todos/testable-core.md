# Testable Core: Mock Everything, Prove Logic Before Live Runs

**Created:** 2026-04-04
**Context:** Live challenge runs take 10+ minutes per 3-cycle run and depend on flaky external providers. The metabolism logic (scheduling, fitness evaluation, loop detection, escalation) should be testable with mock providers in under a second.

## What Already Exists

`metabolism::tests` already has `MockProvider` — returns canned responses, tracks call count. The existing tests (routes_artifacts_and_turns_the_cycle, kills_prefailure_workcells, rejects_closure_breaking_mutations) prove scheduling, routing, and gating work.

## What's Missing

### 1. Mock-based fitness degradation test
Simulate 3 cycles where the coder produces progressively worse code. Verify the metabolism detects and reports the degradation pattern. Currently no test for this — the metabolism happily degrades.

### 2. Mock-based architect validation test
Mock provider returns a `SystemPatch` JSON. Verify the metabolism routes it through `self_sandbox::validate_patch`. With a mock project root, verify protected files are rejected and valid patches are accepted.

### 3. Mock-based escalation ladder tests (future)
When loop detection is implemented: mock provider returns the same output 3 times → verify enzyme is halted.
When clean session is implemented: verify context is reset after 2 degradation cycles.
When model swap is implemented: verify provider assignment changes after 3 degradation cycles.
When multi-model is implemented: verify N providers are called and highest-fitness output wins.

### 4. Mock-based feedback loop test
Cycle 1: coder produces code that fails 1 test. Verify `failure_report` artifact is populated.
Cycle 2: coder receives failure_report in its prompt. Verify the prompt contains "PREVIOUS ATTEMPT FAILED".

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
