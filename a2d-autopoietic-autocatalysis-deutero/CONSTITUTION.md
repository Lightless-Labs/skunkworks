# A²D Constitution

Organizational invariants. Immutable by automated actors.
Human amendment requires explicit, signed change with rationale.

## Invariant 1: Catalytic Closure

The RAF detection algorithm must find a non-empty maxRAF after any
proposed mutation. If a mutation would break catalytic closure (e.g.,
removing an enzyme that is the sole catalyst for another enzyme's
production), the mutation is rejected.

**Enforcement:** Mechanical gate in the germline. No agent discretion.

## Invariant 2: Mechanical Verification Only

Every fitness signal must be mechanically verifiable. No component
may evaluate its own output. No agent self-report is accepted as
evidence of improvement.

Acceptable fitness signals:
- Test pass/fail (Bazel exit code)
- Benchmark delta (measured, not estimated)
- RAF coverage ratio (algorithmic)
- Build success/failure (compiler output)
- Behavioral health metrics (observer output)

Unacceptable fitness signals:
- Agent summary of improvement
- Self-assessed quality score
- Narrative description of progress

**Enforcement:** Observer produces numbers only. Germline gates on
mechanical deltas only.

## Invariant 3: Ephemeral Workcells

Workcells are ephemeral. No enzyme execution persists beyond its task.
State flows through the germline (persistent) and lineage archive
(append-only), never through long-running sessions.

**Enforcement:** Workcell lifecycle in code. Observer kills on
pre-failure state detection.

## Invariant 4: Information Barriers

Enzyme evaluation uses structural information barriers. The entity
that produces an artifact cannot be the sole evaluator of that
artifact. Cross-workcell state access is prevented at compile time
through typed contexts.

**Enforcement:** Type system (Foundry pattern). No runtime policy.

## Invariant 5: Irreversible Review Gates

Once any component (human or automated) requires human review of a
mutation, that requirement cannot be removed by automated actors.

**Enforcement:** Gate semantics in the mutation pipeline.

## Invariant 6: Lineage and Rollback

Every heritable change to the germline is recorded with provenance.
Rollback to any prior state must always be possible.

**Enforcement:** Git-backed germline. Every mutation is a commit.

---

*This constitution is part of the food set: it is consumed by the
system but not produced by it. Amending it is a human act.*
