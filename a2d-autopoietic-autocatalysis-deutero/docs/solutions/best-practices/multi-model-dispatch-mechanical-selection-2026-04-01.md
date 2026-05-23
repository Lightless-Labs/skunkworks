---
title: "Multi-Model Dispatch with Mechanical Selection for Design Decisions"
date: 2026-04-01
category: best-practices
module: metabolism
problem_type: best_practice
component: development_workflow
severity: high
applies_when:
  - "Making architectural decisions that shape downstream implementation"
  - "Single-model design would create methodological monoculture"
  - "Multiple model providers are available (Codex, Gemini, OpenCode)"
tags:
  - multi-model
  - mechanical-verification
  - anti-monoculture
  - provider-dispatch
  - design-decisions
---

# Multi-Model Dispatch with Mechanical Selection for Design Decisions

## Context

During A²D Stage 0, the metabolism module (the runtime orchestrator) was the most important remaining design decision. Single-pass Claude design would have been the methodological monoculture the collision synthesis warns against (Sawdust finding: 10/11 overlap when models review each other). The metabolism determines everything downstream — it needed diverse perspectives.

## Guidance

Write an NLSpec (Why/What/How/Done) for the component, then dispatch it to multiple providers simultaneously. Use `cargo test` (or equivalent mechanical gate) to select the winner — not conversational review.

### Dispatch pattern

1. **Bundle context**: Concatenate all relevant source files + NLSpec into a single context file
2. **Dispatch in parallel** to 3+ providers:
   - Codex (full-auto mode, writes directly to repo): `codex exec "..." --full-auto -C /path/to/repo`
   - Gemini (output to temp file): `gemini -p "..." --sandbox -o text < context.md > /tmp/proposal.rs`
   - OpenCode/Kimi (output to temp file): `opencode run --model kimi-for-coding/k2p5 -f context.md "..." | jq -r 'select(.type == "text") | .text' > /tmp/proposal.rs`
3. **Select mechanically**: whichever proposal compiles and passes `cargo test` first wins
4. **Archive alternatives**: save non-selected proposals for later comparison or synthesis

### What happened in practice

| Provider | Model | Result | Lines | Tests |
|----------|-------|--------|-------|-------|
| Codex | o3 (full-auto) | Compiled, 56/56 tests pass | 889 | 4 |
| Gemini | 3.1 Pro | Delivered (343 lines), archived | 343 | 2 |
| Codex | o4-mini (retry) | Failed (auth contention with parallel A² session) | — | — |
| OpenCode | Kimi k2.5 | Empty output (format issue with jq pipeline) | — | — |

Codex o3 won via mechanical gate. Committed with `Co-Authored-By: Codex o3`.

## Why This Matters

- **Avoids monoculture**: Three independent models producing proposals from the same NLSpec reduces shared-bias amplification
- **Mechanical selection**: `cargo test` is the fitness function, not "which code looks better to Claude"
- **Codex full-auto is powerful**: Writing directly to the repo means it can run its own `cargo test` during generation
- **2/4 success rate is acceptable**: Not every provider will succeed every time. The pattern works as long as at least one produces a passing proposal
- **Contention is real**: Multiple sessions competing for the same provider (A² also using Codex) can cause auth failures. Plan for retries or alternative providers.

## When to Apply

- Architectural decisions (metabolism, verification primitives, evolution strategy)
- Any component where the design shapes downstream implementation significantly
- When the first collision synthesis learning applies: "single-model design is monoculture"

**Not needed for:**
- Mechanical implementation of well-specified components (types, tests)
- Bug fixes with clear root cause
- Documentation or research writing

## Examples

### The NLSpec that drove this

`docs/plans/metabolism-nlspec.md` — 108-line Why/What/How/Done spec. The Done section's checkboxes became the mechanical acceptance criteria.

### The commit message pattern

```
Add metabolism module (Codex/o3 proposal, full-auto)

...passed mechanical fitness gate (56 tests, all passing) without
Claude review of the implementation.

Co-Authored-By: Codex o3 <noreply@openai.com>
Co-Authored-By: Claude Opus 4.6 (1M context) <noreply@anthropic.com>
```

## Related

- `docs/solutions/best-practices/collision-synthesis-cross-project-bootstrapping-2026-04-01.md` — methodology that discovered why monoculture is dangerous
- `research/02-refinery-patterns.md` — converge-refinery's controversy scoring as a more sophisticated selection mechanism
- `docs/plans/metabolism-nlspec.md` — the NLSpec that drove this specific dispatch
