---
title: "Architect Patches Need a Relevance Gate"
date: 2026-05-01
category: runtime-bugs
module: self_sandbox
problem_type: bug_fix
component: autopoiesis
symptoms:
  - "Architect accepted a patch to an unrelated domain/demo file during sudoku benchmark"
  - "Self-sandbox cargo test passed, but the patch did not improve A²D's mechanism"
  - "Passing tests were treated as relevance proof"
root_cause: self_sandbox_allowed_every_non_protected_rust_file
resolution_type: code_fix
severity: high
tags:
  - architect
  - self-sandbox
  - autopoiesis
  - relevance-gate
  - system-patch
  - live-validation
---

# Architect Patches Need a Relevance Gate

## Problem

A traced `sudoku 5` live run after OpenCode parser hardening reached 100% best fitness, but the architect accepted and applied a patch to `crates/a2d-core/src/prime.rs`.

The patch passed `cargo test`, but it was not an A²D mechanism improvement. It rewrote an incidental library/demo module while the active benchmark was sudoku. The change was reverted manually.

## Root cause

The self-sandbox had two categories only:

1. protected files — mechanically rejected
2. every other Rust file under `crates/` — visible to the architect and patchable if `cargo test` passed

That conflated "not constitutional physics" with "useful target for automated self-modification." Passing tests prove only non-breakage, not relevance. A model can rewrite unrelated code and still pass self-sandbox.

## Solution

Add a narrow `AUTOMATED_MODIFIABLE_FILES` allowlist for mechanism files the architect may see and patch:

- orchestration/runtime: `metabolism.rs`, `provider.rs`, `observer.rs`, `workcell.rs`, `lineage.rs`, `types.rs`
- challenge contract wiring: `challenges.rs`
- CLI/provider integration: `crates/a2d-cli/src/main.rs`, `crates/a2d-providers/src/*.rs`

Exclude incidental domain/demo modules such as `prime.rs`, `email.rs`, and broad exports in `lib.rs`.

`validate_patch()` now gates in this order:

1. protected file check
2. automated-modifiable eligibility check
3. target existence check
4. copy/apply in temp tree
5. `cargo test`

`read_modifiable_files()` now includes only eligible mechanism files, so the architect's context excludes ineligible files rather than tempting irrelevant edits.

## Tests

Added unit coverage for:

- mechanism files are eligible
- `prime.rs`/`email.rs` are not protected but are not automated-modifiable
- ineligible patches reject before filesystem access
- nonexistent eligible target still reports `does not exist`
- architect context file reader excludes ineligible domain files and protected files

`cargo test` passes: 135 tests passing, 2 ignored.

## Related

- `docs/solutions/runtime-bugs/opencode-write-tool-output-recovery-2026-05-01.md`
- `docs/solutions/runtime-bugs/cli-providers-must-run-in-isolated-cwd-2026-04-28.md`
