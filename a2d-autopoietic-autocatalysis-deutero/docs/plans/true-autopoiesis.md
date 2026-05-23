# Plan: True Autopoiesis — Self-Modifying System Code

**Created:** 2026-04-04
**Status:** Phases 1-4 complete. Phase 5 (validation) pending.
**Depends on:** Stage 2 (complete)

## Problem

A²D claims to be autopoietic but the evolver can only modify enzyme definitions — JSON data describing graph topology and prompt templates. It cannot modify the Rust code that implements the metabolism, the sandbox, the fitness evaluation, or any other system component. This makes A²D self-configuring (Level 3), not self-producing (Level 6).

The system that writes chess engines cannot write itself.

## Insight

The infrastructure for safe self-modification already exists:
- **Sandbox** compiles and runs arbitrary Rust code in isolation
- **Fitness ratchet** rejects anything that makes outcomes worse
- **RAF gate** rejects anything that breaks catalytic closure
- **Lineage archive** makes every state recoverable via git
- **Constitution** defines what's immutable by automated actors

These are exactly the constraints needed to let the system modify its own source code safely. The coder already writes Rust. The sandbox already compiles Rust. Point the pipeline at itself.

## Architecture

### Current cycle (artifact-producing)

```
Requirements → Coder → Code → Sandbox → Fitness
                                          ↓
Test Results → Evolver → Enzyme Defs → Germline
```

### Self-modifying cycle (system-producing)

```
Failure Diagnostics     ─┐
Fitness History         ─┤
System Code (snapshot)  ─┴→ Architect → System Patch → Self-Sandbox → Accept/Reject
                                                            │
                                                     cargo test passes?
                                                     challenge fitness ≥ current?
                                                            │
                                                            ↓
                                                     Apply to source tree
                                                     Commit to lineage
```

### Constitutional boundary

The Constitution (Invariant 2, Invariant 4) defines what the system cannot modify about itself:

**Protected (the physics):**
- `germline.rs` — mutation gating logic
- `raf.rs` — closure detection algorithm
- `sandbox.rs` — execution environment
- `benchmark.rs` — fitness measurement
- `CONSTITUTION.md` — the invariants themselves

**Modifiable (the chemistry):**
- `metabolism.rs` — cycle orchestration, scheduling, artifact routing
- `types.rs` — data structures, artifact types
- `challenges.rs` — challenge definitions and acceptance tests
- `main.rs` — CLI, provider registry, seed germline
- `crates/a2d-providers/` — how models are invoked
- Enzyme prompt templates — behavioral definitions

The system can redesign how it orchestrates work, but not how it evaluates work or enforces invariants. It can rearrange its organelles but not rewrite its physics.

### Deutero-learning emerges

The evolver modifies enzyme definitions, including the architect's. The architect modifies system code. Therefore: the evolver evolves how the system modifies itself. This is learning to learn — deutero in the Batesonian sense — and it emerges from composition, not from a dedicated mechanism.

## Implementation

### Phase 1: Close the feedback loop (prerequisite)

The evolver and architect both need failure diagnostics. Currently, sandbox output goes nowhere.

1. Add `failure_report` artifact type
2. After `benchmark.evaluate()`, if fitness < 1.0, store `SandboxResult` details (compile errors, test failures, stdout/stderr) as a `failure_report` artifact
3. Wire `failure_report` into the coder's reactants — "Previous attempt failed: {error}"
4. Wire `failure_report` into the evolver's reactants

**Files:** `metabolism.rs`, `types.rs` (artifact type), seed germline in `main.rs`

### Phase 2: Self-modification infrastructure

1. Add `system_patch` artifact type — a proposed modification to a system file
2. Add `SystemPatch` struct: `{ file_path: String, new_content: String }`
3. Define `PROTECTED_FILES` constant — files the architect cannot touch
4. Add `self_sandbox` module:
   - Copy the crate tree to a temp directory
   - Apply the proposed patch (replace file content)
   - Run `cargo test` on the modified tree
   - If tests pass, build the binary
   - Run a challenge with the modified binary
   - Return pass/fail + fitness delta
5. Reject any patch targeting a protected file (mechanical, not behavioral)

**Files:** new `crates/a2d-core/src/self_sandbox.rs`, `types.rs`

### Phase 3: Architect enzyme

1. New enzyme definition:
   - **id:** `architect`
   - **reactants:** `failure_report`, `fitness_history`
   - **products:** `system_patch`
   - **catalysts:** `system_code` (read-only snapshot of modifiable files)
2. The architect's prompt includes:
   - Current system source code (modifiable files only)
   - Failure diagnostics from recent cycles
   - Fitness trajectory (improving? plateauing? regressing?)
   - Constitutional constraints (what it cannot touch and why)
3. The architect proposes targeted, minimal changes to system files
4. `self_sandbox` validates the proposal before acceptance

**Files:** seed germline in `main.rs`, `metabolism.rs` (architect invocation + self-sandbox gating)

### Phase 4: Wire into the metabolism

1. After a challenge cycle, if fitness is plateauing (no improvement for N cycles), fire the architect
2. Self-sandbox validates the proposed patch
3. If accepted: apply patch to real source tree, commit to lineage, rebuild
4. If rejected: record why (compile failure? test failure? fitness regression?)
5. Feed rejection reasons back to architect on next invocation

**Files:** `metabolism.rs`, `main.rs`

### Phase 5: Validate

1. Run sudoku challenge: single model baseline → A²D without architect → A²D with architect
2. The system should break through the 83% ceiling by modifying its own orchestration
3. If it doesn't, the failure diagnostics tell us (and the architect) why

## Success criteria

- The system proposes at least one modification to its own source code
- That modification passes cargo test (85 tests)
- That modification improves challenge fitness above baseline
- The evolver subsequently modifies the architect's enzyme definition
- The system cannot modify protected files (mechanical enforcement, tested)

## Risks

- **Fitness function gaming:** The architect could modify challenges.rs to make tests easier. Mitigated by: acceptance tests are holdout (hidden from the architect's prompt), and challenges.rs could be added to PROTECTED_FILES if this becomes an issue.
- **Cascading failures:** A bad patch could break the build. Mitigated by: self_sandbox tests in isolation before applying.
- **Infinite recursion:** The architect modifies metabolism.rs which changes how the architect is invoked. Mitigated by: fitness ratchet ensures each change is an improvement, and the architect's invocation is part of the protected metabolism loop.
