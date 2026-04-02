---
title: "Autonomous diff generation and self-modification patterns for AI systems"
date: 2026-04-01
category: best-practices
module: "A² self-improvement governor / diff pipeline"
problem_type: best_practice
component: tooling
severity: high
applies_when:
  - "Building an autonomous AI system that modifies its own source code via diffs"
  - "Parsing LLM-generated unified diffs for programmatic application"
  - "Bootstrapping a self-improving system past its first working self-edit"
  - "Designing CLI/governor boundaries for patch auto-application"
tags:
  - autonomous-code-generation
  - self-modification
  - unified-diff
  - git-apply
  - compiler-bootstrap
  - llm-output-parsing
  - a2
  - self-improvement
---

# Autonomous diff generation and self-modification patterns for AI systems

## Context

The A² (Autopoietic Autocatalysis) system needs to modify its own Rust source code autonomously as part of a self-improvement loop. The governor orchestrates LLM calls that propose code changes as unified diffs, which must then pass `git apply --check` before being applied. Getting LLMs to produce structurally valid, machine-applicable diffs turned out to be a multi-layered problem spanning prompt engineering, output parsing, architectural boundaries, and a fundamental bootstrapping paradox. (auto memory [claude]: A² is a self-producing software factory research project using autopoiesis and autocatalytic set theory)

## Guidance

### 1. Parse the *last* diff in the response, not the first

LLMs self-correct mid-response. A model will often produce a first-draft diff, realize it has errors (wrong line counts, missing context), say something like "let me redo that", and emit a corrected version. The diff parser must use `rfind` (last match) rather than `find` (first match) to extract the final, corrected diff block.

```rust
// WRONG: grabs the first (possibly broken) diff
let start = response.find("diff --git");

// RIGHT: grabs the last (self-corrected) diff
let start = response.rfind("diff --git");
```

### 2. Diff format instructions must be exhaustively explicit

Vague prompts like "produce a unified diff" yield pseudo-diffs that look right to a human but fail `git apply --check`. The system prompt needs all three of:

- **(a) A concrete example** with real `@@` hunk headers, correct line numbers, and proper context lines
- **(b) Explicit formatting rules**: `a/` and `b/` path prefixes, accurate `@@ -old,count +new,count @@` arithmetic, unchanged context lines with a leading space (not blank)
- **(c) The validation gate stated upfront**: "your diff must pass `git apply --check`"

Without all three, models produce diffs that are structurally similar but not machine-parseable.

### 3. Bootstrap the diff fixer manually (the compiler bootstrap pattern)

A self-improving system cannot fix its own diff quality if the fix itself requires producing a valid diff. This is the "compiler bootstrap" problem: you need a working compiler to compile the compiler. In A²'s case, the diff-parsing and prompt improvements had to be applied by a human as the last manual intervention. Once the diff pipeline was reliable, the system could self-modify from that point forward.

This pattern is anticipated in the A² DESIGN.md but was confirmed in practice. Plan for it: identify the minimum set of capabilities that must work before the system can improve itself, and be prepared to bootstrap those manually.

### 4. Separate promotion decisions from application decisions

The `--apply` flag belongs on the CLI (`a2ctl`), not in the governor. The governor evaluates and promotes candidates; the CLI decides whether to act on a promotion. `try_apply_patch()` runs `git apply --check` as a dry run first, only applying if that passes.

```
Governor: "This candidate is promoted" (evaluation concern)
CLI:      "I will apply this patch"     (operational concern)

try_apply_patch():
  1. git apply --check candidate.diff   # dry run
  2. if ok: git apply candidate.diff    # real apply
  3. if fail: log error, don't apply    # separate quality gate
```

This separation means a failed apply is not an evaluator failure -- it is a distinct quality gate. The evaluator is deliberately lenient (structural check: non-empty diff + tests pass). The apply gate catches format issues the evaluator does not assess.

### 5. Expect ~50% promotion rates with multi-model pools

Across self-improvement rounds, roughly 50% of candidates were promoted overall. Claude produced higher-quality diffs; other models (via OpenCode backends) had lower promotion rates. This is expected and acceptable -- the system generates multiple candidates per round precisely because not all will succeed.

## Why This Matters

Without these patterns, an autonomous self-improvement loop silently produces diffs that look valid but cannot be applied, creating an invisible quality ceiling. The system appears to be working (candidates are generated, evaluated, promoted) but no actual code changes land. The `rfind` fix alone unblocked an entire class of self-corrections that were being discarded. The explicit prompt format converted model output from ~20% apply success to reliable application. And the bootstrap recognition prevented days of circular debugging where the system tried to fix itself with the broken tool it was trying to fix.

## When to Apply

- Building any system where an LLM generates patches or diffs for programmatic application
- Designing autonomous agents that modify their own codebases
- Parsing structured output from LLMs where the model may self-correct within a single response
- Architecting evaluation/promotion/application pipelines with distinct quality gates
- Bootstrapping self-referential systems past their initial capability threshold

## Examples

### Before: naive diff extraction

```rust
fn extract_diff(response: &str) -> Option<&str> {
    let start = response.find("diff --git")?;  // gets first (broken) draft
    let end = response[start..].find("\n\n").map(|i| start + i)?;
    Some(&response[start..end])
}
```

### After: self-correction-aware diff extraction

```rust
fn extract_diff(response: &str) -> Option<&str> {
    let start = response.rfind("diff --git")?;  // gets last (corrected) version
    Some(&response[start..])  // take everything from last diff header to end
}
```

### Before: vague diff prompt

```text
Please produce a unified diff for the changes.
```

### After: explicit diff prompt with example and validation gate

```text
Produce a unified diff that passes `git apply --check`. Format:

diff --git a/src/example.rs b/src/example.rs
--- a/src/example.rs
+++ b/src/example.rs
@@ -10,3 +10,4 @@
 unchanged context line (note leading space)
-removed line
+added line
+another added line

Rules:
- Paths must have a/ and b/ prefixes
- @@ counts must be arithmetically correct
- Context lines must have a leading space character, not be blank
- The diff must apply cleanly to the current file on disk
```

## Related

- A² DESIGN.md: compiler bootstrap pattern (theoretical prediction confirmed in practice)
- A² governor architecture: evaluation/promotion pipeline
- A² `a2ctl --apply`: CLI-level patch application
