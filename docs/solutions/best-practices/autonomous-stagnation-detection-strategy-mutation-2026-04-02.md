---
title: "Stagnation detection and strategy mutation in autonomous self-improvement loops"
date: 2026-04-02
category: best-practices
module: "A² autonomous loop / stagnation detection"
problem_type: best_practice
component: tooling
severity: critical
applies_when:
  - "Running autonomous self-improvement loops that iterate without human oversight"
  - "A fitness metric has plateaued across multiple consecutive rounds"
  - "The self-repair mechanism itself may be the broken component"
  - "Designing hill-climbing systems that need to escape local optima"
tags:
  - autonomous-loop
  - stagnation-detection
  - strategy-mutation
  - hill-climbing
  - worktree-catalyst
  - self-repair
  - a2
  - autoresearch
---

# Stagnation detection and strategy mutation in autonomous self-improvement loops

## Context

A² ran 14 autonomous rounds overnight, producing zero successful auto-applies and zero test growth (stuck at 41 tests). The system did not detect its own stagnation. A human had to diagnose that the diff-generation approach was fundamentally broken and switch to worktree-based editing. The loop kept generating different task descriptions but used the same broken mechanism (text-based diffs) every round -- mutating content but not strategy. (auto memory [claude]: A² is a self-producing software factory using autopoiesis, autocatalytic set theory, and closure to efficient causation)

## Guidance

### 1. Treat stagnation as a first-class signal

If N consecutive rounds produce zero metric improvement (tests passing, successful applies, new capabilities), the system must recognize this as a plateau and change strategy -- not retry with different content. Define a concrete stagnation threshold (e.g., 3 rounds with no improvement) and wire it into the loop as a hard trigger.

```rust
struct StagnationDetector {
    window_size: usize,        // e.g., 3 rounds
    metrics: VecDeque<RoundMetrics>,
}

impl StagnationDetector {
    fn is_stagnant(&self) -> bool {
        if self.metrics.len() < self.window_size {
            return false;
        }
        let recent = self.metrics.iter().rev().take(self.window_size);
        recent.clone().all(|m| m.successful_applies == 0)
            || recent.clone().all(|m| m.test_growth == 0)
    }
}
```

### 2. Mutate the approach, not just the content

Following Karpathy's autoresearch pattern: when fitness plateaus, mutate the STRATEGY (how patches are produced), not just the PROMPT (what to produce). The overnight loop was hill-climbing on task descriptions while holding the editing mechanism constant. The mechanism was the bottleneck, not the task selection.

Design the system with a strategy registry -- a set of alternative approaches that can be swapped in when the current one stagnates:

```rust
enum EditStrategy {
    TextDiff,          // generate unified diffs (original, fragile)
    WorktreeCatalyst,  // edit files in worktree, let git compute diff
    InPlaceEdit,       // direct file modification with backup
    // future strategies added here
}
```

When stagnation is detected, promote the next strategy. Do not retry the same strategy with different parameters -- that is content mutation, not strategy mutation.

### 3. LLMs cannot produce git-apply-compatible diffs -- use WorktreeCatalyst

Unified diffs require byte-level precision on whitespace, indentation, line numbers, and context. Models consistently fail at this even with exhaustive prompting (see the related doc on diff generation patterns). The fix: let models edit files directly in git worktrees (what they are good at), then let git compute the diff (what it is good at).

```
WorktreeCatalyst pattern:
  1. git worktree add /tmp/a2-round-N branch
  2. LLM edits files directly in the worktree (natural file editing)
  3. git diff computes the patch (byte-perfect)
  4. git apply --check validates on the main tree
  5. git worktree remove /tmp/a2-round-N
```

This separates "what LLMs are good at" (understanding and editing code) from "what they are bad at" (producing byte-exact diff formatting).

### 4. Detect the bootstrap paradox of self-repair

A² could not fix its own diff quality because the fix required producing a diff. When the self-repair mechanism is the thing that is broken, the system enters an infinite loop of failing to fix itself. The system needs:

- A meta-level health check: "is my self-repair capability itself functional?"
- An escalation path: if self-repair is broken, switch to an alternative strategy or signal for external intervention
- A circuit breaker: after N failed self-repair attempts on the same subsystem, stop trying and escalate

This is distinct from the compiler bootstrap paradox (documented in the related diff patterns doc). The bootstrap paradox is a one-time startup problem. The self-repair paradox can occur at runtime whenever a core capability degrades.

### 5. Track multi-dimensional fitness signals

Test count alone was insufficient -- the system needed to track:

| Metric | What it reveals |
|--------|----------------|
| Successful applies | Whether the editing mechanism works at all |
| Test growth rate | Whether the system is making forward progress |
| Diff quality (apply success rate) | Whether the strategy is fundamentally viable |
| Round-over-round improvement | Whether the system is hill-climbing or flat |

A single metric (tests pass) masked the stagnation. The tests kept passing because nothing was being applied -- 41 tests passing 14 times in a row looks like stability, not failure.

```rust
struct RoundMetrics {
    tests_passing: usize,
    tests_added: usize,
    successful_applies: usize,
    attempted_applies: usize,
    candidates_generated: usize,
    candidates_promoted: usize,
}

impl RoundMetrics {
    fn apply_success_rate(&self) -> f64 {
        if self.attempted_applies == 0 { return 0.0; }
        self.successful_applies as f64 / self.attempted_applies as f64
    }

    fn is_progressing(&self) -> bool {
        self.tests_added > 0 || self.successful_applies > 0
    }
}
```

## Why This Matters

Without stagnation detection, an autonomous loop burns compute, time, and context indefinitely while making zero progress. The 14-round overnight failure consumed resources equivalent to what a single strategy switch (to WorktreeCatalyst) solved in one round. More critically, a system that cannot detect its own stagnation cannot self-improve -- it violates the autopoietic requirement that the system maintain and repair its own organization. Stagnation detection is not an optimization; it is a prerequisite for genuine autonomy.

## When to Apply

- Building any autonomous loop that iterates without human oversight
- Designing self-improvement systems with multiple possible strategies
- Implementing hill-climbing or evolutionary approaches where local optima are possible
- Running overnight or long-duration autonomous sessions
- Any system where a subsystem failure could prevent self-repair of that same subsystem

## Examples

### Before: single-strategy loop with no stagnation detection

```rust
loop {
    let task = select_next_task();
    let diff = generate_diff_via_llm(task);  // always the same strategy
    match apply_diff(diff) {
        Ok(_) => run_tests(),
        Err(_) => continue,  // silently retries forever
    }
}
```

### After: multi-strategy loop with stagnation detection

```rust
let mut strategy = EditStrategy::TextDiff;
let mut detector = StagnationDetector::new(window_size: 3);

loop {
    let task = select_next_task();
    let metrics = execute_round(task, &strategy);
    detector.record(metrics);

    if detector.is_stagnant() {
        log::warn!("Stagnation detected after {} rounds, switching strategy", detector.window_size);
        strategy = strategy.next_alternative();
        detector.reset();

        if strategy.is_exhausted() {
            log::error!("All strategies exhausted, escalating");
            escalate_to_external();
            break;
        }
    }
}
```

## Related

- [Autonomous diff generation and self-modification patterns](autonomous-diff-generation-self-modification-patterns-2026-04-01.md) -- covers the diff-quality patterns that the WorktreeCatalyst approach supersedes
- Karpathy's autoresearch pattern: hill-climbing with strategy mutation
- A² DESIGN.md: autopoietic self-repair requirements
