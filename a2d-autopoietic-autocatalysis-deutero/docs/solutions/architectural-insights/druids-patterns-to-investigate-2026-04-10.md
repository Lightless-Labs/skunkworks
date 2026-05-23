---
module: metabolism, escalation, observer
tags: [druids, fulcrum, event-driven, best-of-n, auditor, pattern-source]
problem_type: design-investigation
---

# Druids Patterns Worth Investigating

**Source:** [github.com/fulcrumresearch/druids](https://github.com/fulcrumresearch/druids), [HN discussion](https://news.ycombinator.com/item?id=47695666)

Batteries-included library to coordinate and deploy coding agents across sandboxed VMs. Python-based, event-driven control flow.

## What's Worth Stealing

### 1. The Auditor Pattern (Verify the Verification)
**What it is:** The auditor doesn't verify code — it verifies that the builder's *verification was real*. Explicit "lazy patterns" rejection list: "I verified the endpoint works" without showing curl output, only unit tests without end-to-end demo, fabricated output, happy path only.

**Why A²D should care:** The sandbox verifies correctness (does it compile? do tests pass?). But nobody verifies that the *benchmark suite* is adequate, or that the acceptance tests actually exercise the claimed behavior. An auditor enzyme could verify verification quality: "The fitness score is 100% but the acceptance tests only check easy puzzles — is the verification meaningful?"

**What to borrow:** Not a separate enzyme (too expensive). Instead: add verification-quality checks to the benchmark itself. "Does the acceptance test suite cover diverse inputs? Does the passing test actually exercise the failure mode that was reported?" This is the mutation testing pattern from Fowler, grounded in Druids' auditor concept.

### 2. Event-Driven Control Flow
**What it is:** Agents trigger events (submit, commit, surface). The program defines reactions. Clean separation: agent decides WHEN, program decides WHAT.

**Why A²D should care:** A²D's metabolism is topology-driven: the catalytic graph defines scheduling. Agents don't control when they fire — the metabolism fires them when their inputs are ready. Druids gives agents agency over flow control.

**What to borrow:** Not the full event system (it would break RAF closure). But the "surface" pattern is interesting: let enzymes explicitly declare "I'm stuck" or "I found something unexpected" as structured events that the metabolism can react to. Currently, the only signal is the artifact output. A "stuck" signal from the coder would be a direct escalation trigger instead of waiting for loop detection to infer it from repeated fitness signatures.

### 3. Best-of-N with Judge
**What it is:** N workers implement the same spec independently on N sandboxed copies. A judge picks the best. Clean Python: `for i in range(n): worker = await ctx.agent(f"worker-{i}")`.

**Why A²D should care:** This is exactly escalation rung 6 (multi-model consensus). Druids already has a working implementation of the pattern — N parallel implementations, judge selects winner.

**Difference from A²D's approach:** Druids uses an LLM judge; A²D would use the sandbox (fitness-based selection). The sandbox is more reliable (mechanical) but less nuanced (can't assess code quality beyond test pass/fail). Druids' judge can assess things like "this implementation is cleaner" which the sandbox can't.

**What to borrow:** The coordination pattern, not the evaluation method. N providers run in parallel (or sequence, since Rust is sync). Each produces an artifact. The sandbox benchmarks all N. Highest fitness wins. No LLM judge needed — the sandbox IS the judge.

### 4. Rejection Loop with Max Rounds
**What it is:** Builder submits verification → auditor approves or rejects with specific feedback → builder revises. Abandoned after MAX_AUDIT_ROUNDS (3) rejections.

**Why A²D should care:** The escalation ladder has 7 rungs but no explicit "give up" signal. Druids abandons after 3 rounds. A²D should have a ceiling too: if rung 6 (multi-model consensus) doesn't work after N attempts, the challenge is beyond current capability. Record the failure and move on.

**What to borrow:** Add a `max_escalation_attempts` ceiling (per rung, or globally). After N consecutive failures at the highest rung, emit a "challenge exceeded capability" report instead of looping forever.

### 5. Machine Sharing (Critic Pattern)
**What it is:** Critic shares the builder's machine. After EVERY commit, critic reviews the diff for simplicity, duplication, and style drift. Feedback is non-blocking — builder considers it and keeps moving.

**Why A²D should care:** The tester enzyme currently reviews code once per cycle. A commit-level critic would catch issues earlier. But A²D doesn't have a "commit" concept within a cycle — artifacts are produced in one shot.

**What to investigate:** Could the metabolism split large artifacts into incremental diffs? Probably not worth it. The real pattern is: non-blocking review that doesn't halt progress, sent as advisory context. This is what rung 2 (consultation) already does.

## What's NOT Worth Stealing

### VM-Level Isolation
Each Druids agent gets a sandboxed VM with copy-on-write cloning. A²D uses subprocess providers with cargo test in temp dirs. The isolation is already sufficient and the VM overhead would be enormous.

### Python Runtime + FastAPI Server
Druids is a hosted service with a web dashboard. A²D is a Rust library that runs locally. Different deployment model entirely.

### LLM-as-Judge
Druids uses an LLM to judge between N submissions. A²D uses the sandbox (mechanical). The sandbox is the oracle; the judge is a model that can be wrong.

## Priority

1. **Max escalation ceiling** (#4) — cheap, prevents infinite loops at the top rung
2. **"I'm stuck" event** (#2) — let the coder explicitly trigger escalation instead of waiting for detection
3. **Best-of-N coordination** (#3) — direct implementation for rung 6
4. **Verification quality** (#1) — longer-term, needs mutation testing infrastructure

## Sources

- [GitHub: fulcrumresearch/druids](https://github.com/fulcrumresearch/druids)
- [HN item 47695666](https://news.ycombinator.com/item?id=47695666)
- [build.py example](https://github.com/fulcrumresearch/druids/blob/main/.druids/build.py) — builder + critic + auditor with max 3 audit rounds
