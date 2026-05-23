---
module: metabolism, self_sandbox, architect
tags: [harness-engineering, fowler, feedback-loops, architectural-fitness, self-modification]
problem_type: design-investigation
---

# Harness Engineering Patterns Worth Investigating

**Source:** Martin Fowler, [Harness Engineering](https://martinfowler.com/articles/harness-engineering.html) (2026)
**Context:** A²D already implements core harness patterns (sandbox as computational sensor, fitness ratchet, RAF closure gate). The architect enzyme goes beyond Fowler's "agentic flywheel" by modifying the orchestration engine itself. These are patterns from the article that A²D doesn't yet have but could benefit from.

## 1. Custom Linters with LLM-Optimized Error Messages

**What Fowler describes:** Instead of raw compiler output, create custom linters that produce error messages optimized for LLM consumption — structured, actionable, with fix suggestions.

**A²D opportunity:** The `failure_report` currently sends raw `rustc` stderr and test output. An intermediate step could reformat these into structured diagnostics: "Test `test_hard_sudoku` failed: expected solved grid but got zeros in row 7. The solver likely doesn't handle constraint propagation for hard puzzles." This is a feedforward guide disguised as a feedback sensor.

**Where:** `benchmark.rs` `format_sandbox_diagnostic()` — add an LLM pass to interpret raw output into actionable feedback. Or make it a new enzyme (inferential sensor).

## 2. Mutation Testing for Acceptance Tests

**What Fowler describes:** Use mutation testing to verify that tests actually catch real bugs, not just pass on correct code.

**A²D opportunity:** The acceptance tests are the most critical component (they're the holdout fitness signal). But we don't verify they're actually discriminating. A mutated sudoku solver that returns hardcoded values for easy puzzles but fails on hard ones should be caught. Mutation testing would verify acceptance test quality mechanically.

**Where:** New module or challenge-level validation. Mutate known-good solutions, verify acceptance tests catch the mutations.

## 3. Garbage Collection Agents

**What Fowler describes:** Periodic agents that scan the codebase for drift, entropy, dead code, style violations — and fix them automatically.

**A²D opportunity:** After multiple architect patches, the metabolism could accumulate cruft or inconsistencies. A "garbage collection" enzyme that periodically reviews system code for dead branches, unused imports, or style drift — gated by `cargo test` like all other patches.

**Where:** New enzyme with low-priority scheduling (fires only when other enzymes are idle).

## 4. Structural Fitness Functions (ArchUnit pattern)

**What Fowler describes:** Compile-time or test-time verification of architectural invariants — module boundaries, dependency direction, naming conventions.

**A²D opportunity:** Beyond RAF closure (which checks catalytic graph integrity), add structural fitness functions for the system code itself:
- No enzyme should import from another enzyme's internals
- Protected files must not be referenced in architect prompts
- All public functions in metabolism.rs must be covered by tests
- The self_sandbox's PROTECTED_FILES list must include self_sandbox.rs (turtles all the way down)

**Where:** Integration tests or a new `cargo test` module that verifies architectural invariants.

## 5. Inferential Controls (LLM Judges)

**What Fowler describes:** Use strong LLM models as judges for semantic quality — code review, design review, security review — sparingly and with good models.

**A²D opportunity:** The self-sandbox currently only gates patches on `cargo test`. An inferential control could review the architect's proposed patches for semantic quality: "Does this change actually address the failure diagnostic?" "Does this change introduce subtle behavioral changes that tests don't cover?" This is Constitution Invariant 4 (information barriers) applied to self-modification.

**Where:** Optional second gate in `apply_system_patch()` — after cargo test passes, run an LLM judge (different provider than the architect) on the diff.

## 6. Harness Templates

**What Fowler describes:** Bundled configurations of guides and sensors for common service topologies (API services, event processors, etc.).

**A²D opportunity:** Different challenges may benefit from different enzyme topologies. A "harness template" for algorithmic challenges (sudoku, chess) vs. system challenges (build a web server) vs. data challenges (parse and transform). The architect could select or compose harness templates based on the challenge type.

**Where:** Extension of `challenges.rs` — each challenge could specify a recommended enzyme topology, not just requirements and acceptance tests.

## 7. Risk Scoring for Auto-Applied Changes

**What Fowler describes:** Score the risk of proposed changes. High-confidence, low-risk changes auto-apply. High-risk changes require human review.

**A²D opportunity:** The architect's patches currently auto-apply if cargo test passes. But some patches are riskier than others — a one-line change to a format string is low risk; restructuring the invocation loop is high risk. Risk scoring could gate whether patches auto-apply or queue for review.

**Heuristic candidates:**
- Lines changed (more = riskier)
- Functions modified (control flow changes = riskier)
- Whether the change touches scheduling logic
- Whether similar patches have been rejected before

**Where:** `self_sandbox.rs` or `metabolism.rs` — add a risk score to `SelfSandboxResult`.

## 8. Harnessability as Design Constraint

**What Fowler describes:** Some languages and frameworks are more "harnessable" than others. Typed languages with built-in checks (module boundaries, exhaustive matching) give the harness more to work with.

**A²D observation:** Rust is exceptionally harnessable — strong type system, `cargo test`, `cargo clippy`, `rustfmt`, edition system. This is why the self-sandbox works at all: `cargo test` is a comprehensive computational sensor that catches most regressions mechanically. A less harnessable language would make self-modification much riskier.

**Implication:** If A²D ever targets other languages, harnessability should be a selection criterion, not just model capability.

## Priority Order

1. **Structural fitness functions** (#4) — cheap, mechanical, high value. Can implement as tests today.
2. **Mutation testing for acceptance tests** (#2) — validates the most critical component.
3. **LLM-optimized diagnostics** (#1) — improves the feedback loop quality with minimal architectural change.
4. **Risk scoring** (#7) — safety as the system gains more autonomy.
5. **Inferential controls** (#5) — adds a second opinion on self-modifications.
6. **Garbage collection** (#3) — becomes important as patch count grows.
7. **Harness templates** (#6) — becomes important when challenge diversity grows.
