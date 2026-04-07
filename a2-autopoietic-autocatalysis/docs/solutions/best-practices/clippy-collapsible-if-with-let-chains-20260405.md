---
title: Collapsing nested if-let into if-let chains to satisfy clippy::collapsible_if
date: 2026-04-05
module: a2-governor
problem_type: best_practice
component: tooling
severity: low
applies_when:
  - Rust 1.88+ (let-chains stabilized)
  - You have nested `if let Some(x) = ... { if let Err(e) = ... { ... } }`
  - clippy::collapsible_if is firing
tags:
  - rust
  - clippy
  - let-chains
  - idiom
---

# Collapsing nested if-let into if-let chains to satisfy clippy::collapsible_if

## Context

When wiring lineage persistence into the Governor, the natural shape of the code was:

```rust
if let Some(store) = &self.lineage_store {
    if let Err(e) = store.persist(&outcome) {
        eprintln!("[lineage persist failed: {e}]");
    }
}
```

clippy now flags this with `collapsible_if` because let-chains let you fold both conditions into a single `if`.

## Guidance

The collapsed form:

```rust
if let Some(store) = &self.lineage_store
    && let Err(e) = store.persist(&outcome)
{
    eprintln!("[lineage persist failed: {e}]");
}
```

Notes:

- `&&` between two `let` patterns is the let-chain syntax. Both bindings are in scope inside the body.
- The opening brace goes on its own line — rustfmt prefers this when the condition spans multiple lines.
- This requires Rust 1.88+. On older toolchains, prefer `#[allow(clippy::collapsible_if)]` over awkward refactors.
- Order matters: a `let Some` that fails short-circuits before the second `let` is evaluated, just like nested ifs.

Do **not** "fix" this by extracting a closure or an early-return helper unless the body is already complex. The let-chain is the idiomatic answer.

## When to Apply

- Persistence hooks gated on optional dependencies (typical pattern: `if let Some(store) = ... && let Err(e) = store.write(...)`)
- Optional config + fallible action pairs
- Anywhere two `if let`s nest with no intervening logic

## Related

- `crates/a2-governor/src/lib.rs` — `run_task` lineage persistence call
- Rust 1.88 release notes — let-chains stabilization
