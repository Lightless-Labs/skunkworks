# Plan: A²D Bootstrap Seed (Stage 0)

**Created:** 2026-04-01
**Status:** Complete (Stage 1 in progress)
**Progress:** All 8 steps complete. 56 tests. First real catalytic cycle ran successfully (3 enzymes, 3 invocations, 0 failures). Evolver mutation rejected by RAF gate (parse/closure). Structured evolver prompt added for cycle 2.

## Goal

The minimal self-describing system that can run one catalytic reaction, observe it structurally, and verify RAF closure over a trivially small network. The compiler-bootstrap analogy: the crudest thing that compiles itself.

## Empirical Corrections (from collision synthesis)

These findings from Sawdust/Third Thoughts shape every design decision:

| A² Assumption | Empirical Correction | Seed Implication |
|---|---|---|
| Cortex self-reports health | 85% risk suppression | Cortex reads tool patterns + build outcomes, never agent summaries |
| Evolver evaluates own changes | Optimism feedback loop | Fitness = mechanical signals only (test pass rate, build output) |
| Multi-model review verifies | Consensus = hallucination multiplier | Verification through structural isolation (Foundry pattern), not review |
| Persistent enzyme sessions | 7.24x session degradation | Ephemeral workcells; spawn fresh, die after task |
| Agents explore adequately | MVT violation (4x under-explore) | Minimum iteration counts before evaluation |
| Bootstrap self-verifies | Mimetic performativity | Every stage verified by external artifact (diff, binary, benchmark delta) |

## Architecture: Minimal irrRAF

The smallest self-sustaining autocatalytic set. Three enzymes in a tight cycle:

```
Coder  --produces-->  Code  --catalyzes-->  Tester
Tester --produces-->  Test Results  --catalyzes-->  Evolver
Evolver --produces--> Improved Enzyme Defs --catalyzes--> Coder
```

Plus the structural observer (not an enzyme — a mechanical process):

```
Observer --reads-->  Tool patterns, build outcomes, thinking blocks
Observer --writes--> Health metrics (RAF closure ratio, entropy, motif signatures)
```

## Components

### 1. Constitution (`CONSTITUTION.md`)

Organizational invariants. Immutable by automated actors.

- Catalytic closure must be maintained (RAF detection pass)
- Every fitness signal must be mechanically verifiable (no self-report)
- Workcells are ephemeral; germline is persistent
- Information barriers between enzyme evaluation (red) and enzyme execution (green)
- Human review gates are irreversible once set

### 2. Enzyme Trait (`src/enzyme.rs`)

```rust
trait Enzyme {
    fn id(&self) -> EnzymeId;
    fn reactants(&self) -> Vec<ArtifactType>;
    fn products(&self) -> Vec<ArtifactType>;
    fn catalysts(&self) -> Vec<ArtifactType>;
    async fn invoke(&self, ctx: WorkcellContext) -> Result<Vec<Artifact>>;
}
```

Key difference from A²: `WorkcellContext` is typed and scoped. An enzyme cannot access another enzyme's internal state (Foundry's typed-context pattern).

### 3. RAF Detection (`src/raf.rs`)

Hordijk-Steel iterative pruning algorithm. Polynomial time.

Input: set of enzymes + food set
Output: maxRAF (largest self-sustaining subset) + coverage ratio + orphan list

This is the primary organizational health metric. If coverage < 1.0 after a mutation, the mutation is rejected. Mechanical gate, no agent discretion.

### 4. Structural Observer (`src/observer.rs`)

Reads:
- Tool call sequences (from workcell execution logs)
- Build outcomes (Bazel exit codes, test results)
- Thinking block content (when available via model API)
- Entropy rate of tool sequences

Produces:
- RAF closure ratio
- Behavioral state classification (healthy / pre-failure / degraded)
- Deliberation motif presence (R-Ak-Tb signature)
- Entropy rate anomalies

Does NOT produce: natural language health summaries, self-assessments, or recommendations. Numbers only.

### 5. Workcell (`src/workcell.rs`)

Ephemeral execution context for one enzyme invocation.

- Created from germline snapshot
- Scoped typed context (cannot access other workcells)
- Killed after task completion or on pre-failure state detection
- Execution log preserved in lineage archive

### 6. Germline (`src/germline.rs`)

Persistent store of enzyme definitions + constitution + observer config.

- Git-backed (every mutation is a commit)
- Mutations gated by: RAF closure check + mechanical fitness delta
- Rollback always available

## Build Setup

- `MODULE.bazel` — Bazel workspace with rules_rust
- `BUILD.bazel` files per crate
- `Cargo.toml` for local dev (Bazel canonical, Cargo convenience)

## Test Strategy (TDD)

### Unit tests
- RAF detection: known RAF sets, known non-RAF sets, edge cases (empty, single enzyme, food-only)
- Observer: mock tool sequences → correct state classification, entropy calculation, motif detection
- Enzyme trait: typed context prevents cross-access (compile-time test)

### Integration tests
- Minimal cycle: Coder → Tester → Evolver → Coder produces a measurable fitness delta
- RAF check gates mutation: introduce a closure-breaking mutation, verify rejection
- Observer detects degradation: simulate State 3 tool pattern, verify kill signal

### Bootstrap verification
- Stage 0 complete when: the three-enzyme cycle runs, RAF detection confirms closure, observer reports health mechanically, and the system can describe its own enzyme definitions (the genome contains itself)

## Implementation Order

1. **Bazel + Rust workspace setup** — MODULE.bazel, rules_rust, hello-world build
2. **Core types** — ArtifactType, EnzymeId, Artifact, WorkcellContext
3. **Enzyme trait + RAF detection** — trait definition, Hordijk-Steel algorithm, unit tests
4. **Structural observer** — tool sequence parsing, entropy calculation, motif detection, state classification
5. **Workcell** — ephemeral context, scoped access, execution logging
6. **Germline** — git-backed enzyme store, mutation gating
7. **Three-enzyme seed** — hardcoded Coder/Tester/Evolver definitions, minimal but functional
8. **Bootstrap verification** — end-to-end cycle, RAF closure, observer health report

## What This Doesn't Include (Yet)

- Membrane (boundary production) — Stage 1
- Multi-model enzyme backends — Stage 1
- Adversarial verification (Foundry-style red/green) — Stage 1
- Thinking-block cortex (requires model API access) — Stage 2
- Distributed observation (Middens integration) — Stage 3+
- Predictive kill-respawn (requires enough execution data) — Stage 2

## Success Criteria

The seed is complete when:
1. Three enzymes form a verified RAF (closure ratio = 1.0)
2. The observer reports health from mechanical signals only
3. A mutation that breaks closure is mechanically rejected
4. A mutation that improves fitness is mechanically accepted
5. The germline contains the enzyme definitions that produced it (self-description)
