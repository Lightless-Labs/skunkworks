# Phase 1 Implementation Plan

**Created:** 2026-04-01  
**Status:** Proposed  
**Scope:** Phase 1 of A² foundations, ending at the Stage 0 gate and establishing Stage 1 self-hosting under bootstrap profile B0.

## Goal

Build the smallest defensible A² seed described in `DESIGN.md`: a Rust/Bazel workspace with a sequential control plane, one generalist workcell runtime, a frozen constitutional semantic layer, a root-of-trust bundle, basic membrane enforcement, multi-provider model dispatch, and enough evaluation infrastructure to prove the seed can build and evaluate itself from a clean checkout.

## Phase 1 Exit Condition

Phase 1 is complete when all of the following are true from a clean checkout under bootstrap profile B0:

1. `//a2:self_host` passes.
2. `//bench:seed_sentinel` passes with an operator-provided hidden sentinel escrow bundle whose digest matches the root-of-trust manifest.
3. `//bench:mission_gate` passes on the frozen public mission battery.
4. Germline mutations are held for human approval and linearized through the promotion journal.
5. Membrane, lineage, evaluator, and budget controls exist in executable form even if later-phase invariants such as full repair coverage are still stubbed.

## Ordering Summary

| Step | Outcome | Depends on | Complexity |
|---|---|---|---|
| 1 | Bazel + Rust workspace compiles end-to-end | None | M |
| 2 | Shared protocol and trait surface is stable | 1 | M |
| 3 | Constitutional kernel loads semantic clauses and runs verifiers under B0 | 2 | L |
| 4 | Sequential workcell runtime can execute a generalist catalyst through MCP with budgets | 2, 3 | L |
| 5 | `a2d` can schedule, select, and stage promotions through a single-writer journal | 3, 4 | L |
| 6 | Membrane enforces basic ingress and tool policy | 2, 4, 5 | M |
| 7 | Broker can route to Claude, Gemini, Codex, and OpenCode through reused provider adapters | 2, 4, 6 | M |
| 8 | Evaluator can run mission battery, public suite, and seed sentinel suite | 2, 3, 5, 6, 7 | L |
| 9 | Stage 0 gate targets pass from clean checkout | 1-8 | M |

## Step 1: Project Scaffolding

**What to create**

- Root files:
  - `MODULE.bazel`
  - `BUILD.bazel`
  - `Cargo.lock`
- Extend root workspace manifest:
  - `Cargo.toml`
- Create `Cargo.toml`, `BUILD.bazel`, and minimal `src/lib.rs` or `src/main.rs` for:
  - `crates/a2_constitution`
  - `crates/a2_workcell`
  - `crates/a2d`
  - `crates/a2ctl`
  - `crates/a2_membrane`
  - `crates/a2_broker`
  - `crates/a2_eval`
  - `crates/a2_archive`
  - `crates/a2_raf`
  - `crates/a2_sensorium`
  - `crates/a2_mcp_fs`
  - `crates/a2_mcp_git`
  - `crates/a2_mcp_bazel`
- Seed directories:
  - `bench/mission/`
  - `bench/public/`
  - `bench/escrow/`
  - `constitution/spec/`
  - `constitution/verifiers/`
  - `policies/`
  - `prompts/generalist/`
  - `schemas/`
  - `lineage/`

**Key types and traits**

- `a2d::config::A2dConfig`
- `a2ctl::cli::CliArgs`
- `a2_archive::config::ArchiveConfig`
- `a2_workcell::config::WorkcellConfig`

**Implementation notes**

- Use the same Bazel pattern as the sibling A²D project:
  - `rules_rust`
  - Rust 2024 toolchain
  - `crate_universe.from_cargo()` against the workspace `Cargo.toml` and `Cargo.lock`
- Keep Cargo for local development and Bazel as the canonical build surface.
- `a2_raf` and `a2_sensorium` only need compiling stub libraries in Phase 1; they are not on the Stage 0 critical path.
- Create a top-level `//a2:all` aggregate target early so CI can gate on a single Bazel entrypoint.

**Dependencies on prior steps**

- None.

**Acceptance criteria**

- `cargo check --workspace` passes.
- `bazel test //...` resolves `rules_rust` and compiles every workspace member.
- Every crate named in the root `Cargo.toml` has a matching Bazel target.
- No crate uses ad hoc path dependencies outside the repo root.

**Estimated complexity**

- M

## Step 2: Core Types

**What to create**

- Extend `crates/a2_core`
- Add schema snapshots under `schemas/`
- Add protocol-focused tests in `crates/a2_core/tests/`

**Key types and traits**

- Protocol objects to finalize in `a2_core::protocol`:
  - `TaskContract`
  - `ContextPack`
  - `PatchBundle`
  - `EvidenceBundle`
  - `FitnessRecord`
  - `LineageRecord`
  - `CapabilityMap`
  - `BoundaryPolicy`
  - `PromotionDecision`
  - `PromotionJournalEntry`
  - `ConstitutionPatch`
  - `RepairCoverageReport`
  - `RAFReport`
  - `EvaluationGraphReport`
  - `EvolutionPolicy`
  - `WorkcellSlot`
  - `ParentSelection`
  - `MutationImpactReport`
- Supporting enums and value types:
  - `RiskTier`
  - `PrivilegeTier`
  - `GateId`
  - `InvariantId`
  - `StageProfileId`
  - `ModelRef`
  - `BudgetUsage`
  - `GateResult`
- Shared traits to stabilize in `a2_core::traits`:
  - `ModelProvider`
  - `Catalyst`
  - `Evaluator`
  - `Promoter`
  - `Membrane`
  - `ConstitutionalVerifier`
  - `LineageStore`
  - `PromotionJournal`

**Implementation notes**

- Keep all protocol objects in `a2_core`; Phase 1 should not scatter canonical types across crates.
- Normalize model identity as `provider/model` to match `tundish_core::ModelId`.
- Add serde round-trip tests for every protocol object.
- Generate JSON Schema snapshots for externalized objects that cross crate or process boundaries:
  - `TaskContract`
  - `PatchBundle`
  - `EvidenceBundle`
  - `FitnessRecord`
  - `PromotionJournalEntry`
  - `ConstitutionPatch`

**Dependencies on prior steps**

- Step 1.

**Acceptance criteria**

- All protocol objects compile and serialize deterministically.
- Schema snapshots exist for the objects used across crate boundaries.
- No crate outside `a2_core` defines duplicate versions of protocol objects.
- `cargo test -p a2_core` passes with protocol round-trip coverage.

**Estimated complexity**

- M

## Step 3: Constitutional Kernel

**What to create**

- `crates/a2_constitution`
- `crates/a2_archive` for verifier and attestation persistence
- Files under:
  - `constitution/spec/clauses.json`
  - `constitution/spec/stage_profiles/b0.json`
  - `constitution/spec/mission_battery_manifest.json`
  - `constitution/spec/mutation_corpus_manifest.json`
  - `constitution/spec/root_of_trust_bundle.json`
  - `constitution/verifiers/BUILD.bazel`

**Key types and traits**

- `InvariantClause`
- `StageProfile`
- `VerifierDescriptor`
- `VerifierRun`
- `VerifierRunner`
- `SpecDigest`
- `RootOfTrustBundle`
- `EscrowHandle`
- `AttestationRecord`
- `KernelSnapshot`

**Implementation notes**

- Encode all eight organizational invariants as semantic clauses now.
- Only hard-enable the verifiers required for Phase 1:
  - `INV-1 Self-Hosting`
  - `INV-3 Evaluation Integrity`
  - `INV-4 Lineage and Provenance`
  - `INV-5 Boundary Integrity`
  - `INV-6 External-Value Preservation`
  - `INV-7 Promotion Consistency`
  - `INV-8 Operational Discipline`
- Register `INV-2 Constitutive Repair Coverage` in the semantic layer but implement it as a deferred verifier target that returns `NotImplementedForPhase1`; do not pretend Phase 1 satisfies it.
- The root-of-trust bundle must contain only digests, public keys, escrow handles, and operator-required paths. Do not store real approval secrets or hidden sentinel contents in the repo.
- The verifier runner must always execute the previously attested verifier set against constitutional patches. Candidate verifiers never approve themselves.
- Persist verifier runs and attestation decisions in SQLite via `a2_archive`.

**Dependencies on prior steps**

- Step 2.

**Acceptance criteria**

- `a2_constitution` can load semantic clauses and the B0 profile from disk.
- The verifier runner executes a deterministic ordered verifier list and emits machine-readable results.
- A malformed or digest-mismatched root-of-trust bundle fails closed.
- `//constitution:patch_controls` exists and rejects self-approval attempts.
- Verifier results are recorded in the archive with stable identifiers.

**Estimated complexity**

- L

## Step 4: Workcell Runtime

**What to create**

- `crates/a2_workcell`
- `crates/a2_mcp_fs`
- `crates/a2_mcp_git`
- `crates/a2_mcp_bazel`
- Extend `prompts/generalist/`

**Key types and traits**

- `WorkcellRuntime`
- `CatalystLoop`
- `GeneralistCatalyst`
- `McpClient`
- `ToolInvocation`
- `ToolResult`
- `BudgetLedger`
- `BudgetEnforcer`
- `WorkcellTrace`
- `WorkcellOutcome`
- `PrivilegeTier`
- `ToolTransport`

**Implementation notes**

- Keep the Phase 1 runtime sequential and single-catalyst.
- The runtime loop should be:
  - load `TaskContract`
  - load `ContextPack`
  - acquire `CapabilityMap`
  - invoke generalist catalyst through the shared `ModelProvider` trait
  - execute file/git/bazel tool calls through MCP
  - emit `PatchBundle`, `WorkcellTrace`, and `BudgetUsage`
- Implement MCP transport as stdio JSON-RPC only.
- Start with three MCP tool adapters:
  - filesystem read/write constrained to the worktree
  - git status/diff/apply
  - bazel build/test/query for allowlisted targets
- Budget enforcement must cut off by:
  - token count
  - wall-clock deadline
  - model call count
  - tool call count
- Persist trace artifacts to `lineage/` and normalized records to SQLite.
- Keep the runtime broker-agnostic in this step; Step 7 provides the production `ModelProvider` implementation via `a2_broker`.

**Dependencies on prior steps**

- Step 2
- Step 3

**Acceptance criteria**

- A workcell can run a no-op or trivial internal task from a clean worktree and emit a valid `PatchBundle`.
- Tool calls flow only through the MCP client, not through direct shell escapes in runtime code.
- Budget exhaustion produces a typed failure and a complete partial trace.
- `//a2:spawn_workcell` exists and proves a descendant workcell can be materialized and torn down cleanly.

**Estimated complexity**

- L

## Step 5: Control Plane (`a2d`)

**What to create**

- `crates/a2d`
- `crates/a2ctl`
- Extend `crates/a2_archive`

**Key types and traits**

- `Scheduler`
- `ScheduleRequest`
- `QueueState`
- `Selector`
- `SelectionContext`
- `Promoter`
- `PromotionCandidate`
- `PromotionOutcome`
- `HumanApprovalState`
- `ArchiveStore`
- `SchedulerStrategy`
- `ParentSelector`
- `PromotionPolicy`

**Implementation notes**

- Implement the Governor in the order required by the design:
  1. `Scheduler`: single-queue, single-worker, FIFO plus explicit priority override
  2. `Selector`: choose current head by default, or the best prior germline variant if multiple candidates exist
  3. `Promoter`: run hard gates, require human approval under B0, then append one journal entry
- Do not implement Analyst or Strategist behavior in Phase 1 beyond typed no-op placeholders.
- `a2d` owns workcell construction, queueing, archive writes, and evaluation orchestration.
- Wire `a2d` against trait objects from `a2_core` so it can run first with a deterministic test provider and then with `a2_broker` in Step 7 without reworking control-plane APIs.
- `a2ctl` must provide the minimum human control surface:
  - submit internal task
  - inspect candidate
  - approve candidate
  - reject candidate
  - rollback to prior germline
- Promotion semantics must be single-writer. Parallel workcells can wait until later phases.

**Dependencies on prior steps**

- Step 3
- Step 4

**Acceptance criteria**

- `a2d run-task` can schedule one task, select a parent germline, run one workcell, evaluate the result, and stage a promotion decision.
- B0 human approval is enforced before any germline mutation becomes current head.
- Every decision produces a `PromotionJournalEntry`.
- `//lineage:promotion_consistency` exists and proves linearizable admission order.

**Estimated complexity**

- L

## Step 6: Membrane

**What to create**

- `crates/a2_membrane`
- Extend `crates/a2_sensorium` with minimal local ingress
- Policy files under `policies/`

**Key types and traits**

- `PolicySet`
- `ToolAclRule`
- `SecretScope`
- `IngressPartition`
- `PolicyDecision`
- `CapabilityGrant`
- `QuarantinedInput`
- `TypedClaim`
- `IngressNormalizer`
- `PolicyEngine`

**Implementation notes**

- Start with a small policy language backed by checked Rust structs plus on-disk JSON or TOML files.
- Enforce the core B0 membrane rules from the design:
  - raw external text enters only through quarantine
  - privileged workcells consume typed contracts, not raw issue text
  - no workcell that sees raw untrusted text also gets secret-bearing tool scopes
  - no direct access from workcell to root-of-trust bundle
- `a2_sensorium` only needs one Phase 1 ingress mode:
  - local task/evidence files under a repo-controlled directory, converted into `EvidenceBundle` and then `TaskContract`
- Build tool ACLs per workcell slot rather than one global allowlist.

**Dependencies on prior steps**

- Step 2
- Step 4
- Step 5

**Acceptance criteria**

- Policy evaluation is deterministic and machine-testable.
- Workcells receive only the tools and scopes granted in their `CapabilityMap`.
- A synthetic prompt-injection payload is quarantined and cannot reach a privileged workcell verbatim.
- `//policy:membrane_checks` and `//policy:ingress_partition` exist and pass.

**Estimated complexity**

- M

## Step 7: Model Broker

**What to create**

- `crates/a2_broker`
- Vendor or pin `tundish_core` and `tundish_providers` from `converge-refinery` under a repo-local dependency strategy that Bazel can build hermetically.

**Key types and traits**

- `BrokerRequest`
- `BrokerResponse`
- `ProviderHandle`
- `ProviderHealth`
- `DiversityPolicy`
- `ReviewAssignment`
- `Broker`
- `ProviderAdapter`
- `ReviewOrchestrator`

**Implementation notes**

- Do not reimplement provider CLIs from scratch.
- Reuse `tundish_providers::build_provider()` and the provider modules for:
  - `claude-code`
  - `codex-cli`
  - `gemini-cli`
  - `opencode`
- Map A² model references directly onto `tundish_core::ModelId`.
- `a2_broker` should wrap provider creation with A² concerns:
  - allowed tool list
  - timeouts
  - provider health and retry policy
  - minimum diversity requirement for critical artifacts
  - cross-model review assignment for promotion-critical outputs
- Make the first broker mode simple:
  - one primary model for generation
  - optional different-provider reviewer for critical artifacts
  - no colony-level exploration yet

**Dependencies on prior steps**

- Step 2
- Step 4
- Step 6

**Acceptance criteria**

- A broker request can dispatch to each of Claude, Gemini, Codex, and OpenCode through the reused provider layer.
- Provider identity uses the canonical `provider/model` form.
- A critical artifact can require a reviewer from a different provider than the producer.
- Broker failures are typed as retryable vs permanent, matching provider error semantics.

**Estimated complexity**

- M

## Step 8: Seed Evaluation

**What to create**

- `crates/a2_eval`
- Extend `crates/a2_archive`
- Bench assets:
  - `bench/mission/*.json`
  - `bench/public/*.json`
  - `bench/escrow/seed_sentinel_manifest.json`
  - `bench/mutation_corpus/*.json`

**Key types and traits**

- `MissionCase`
- `MissionBattery`
- `PublicSuite`
- `SentinelCase`
- `SentinelSuite`
- `EvaluationRun`
- `GateOutcome`
- `MutationCorpusCase`
- `EvaluationHarness`
- `MissionRunner`
- `SentinelRunner`

**Implementation notes**

- Freeze a small mission battery that reflects real software-factory work from Stage 0:
  - prompt cleanup
  - test expansion
  - tool adapter hardening
  - documentation or schema cleanup
- Keep the public suite in-repo and visible.
- Keep seed hidden sentinels out of the repo contents:
  - store only manifest metadata, digests, and escrow handles in-repo
  - require an operator-provided sentinel bundle path or OCI artifact at runtime
  - fail closed on missing bundle or digest mismatch
- Add evaluator meta-tests with known-bad mutations so the evaluator is itself exercised.
- Record every evaluation run in SQLite and materialize human-readable reports in `lineage/evaluations/`.

**Dependencies on prior steps**

- Step 2
- Step 3
- Step 5
- Step 6
- Step 7

**Acceptance criteria**

- `//bench:public_suite` runs deterministically from the repo.
- `//bench:mission_gate` enforces minimum score floors by mission category.
- `//bench:seed_sentinel` consumes the hidden sentinel escrow bundle and fails closed if the bundle is missing or altered.
- `//bench:evaluator_meta` catches seeded known-bad mutations.

**Estimated complexity**

- L

## Step 9: Stage 0 Gate

**What to create**

- Bazel targets:
  - `//a2:self_host`
  - `//bench:seed_sentinel`
  - `//bench:mission_gate`
- Supporting targets that should already exist by this point:
  - `//policy:membrane_checks`
  - `//policy:ingress_partition`
  - `//lineage:promotion_consistency`
  - `//bench:evaluator_meta`
  - `//ops:budget_ceiling`

**Key types and traits**

- `SelfHostReport`
- `GateReport`
- `SeedBundleDescriptor`
- `ApprovalCheckpoint`

**Implementation notes**

- `//a2:self_host` should be more than a compile check. It must:
  - start from a clean checkout
  - build `a2d`, `a2ctl`, and the runtime crates through Bazel
  - materialize a descendant workcell
  - run a trivial internal task through the full workcell loop
  - confirm the descendant can re-run the required build and evaluation entrypoints
- The Stage 0 gate should run under B0 only:
  - frozen evaluator semantics
  - frozen membrane semantics except bug-fix queue
  - mandatory human approval on germline promotion
- The final gate runner should emit one machine-readable report with the status of all required targets and the root-of-trust digest set used for the run.

**Dependencies on prior steps**

- Steps 1-8.

**Acceptance criteria**

- `bazel test //a2:self_host //bench:seed_sentinel //bench:mission_gate` passes from a clean checkout.
- The run emits a complete `GateReport`.
- The gate fails if the root-of-trust bundle, sentinel escrow digest, or promotion journal ordering is invalid.
- The seed can be handed to a human operator and reproduced without repository-local hacks.

**Estimated complexity**

- M

## Recommended Delivery Sequence

1. Land Step 1 and Step 2 together so all later crates have stable build and type surfaces.
2. Land Step 3 before Step 4 so the runtime is born under B0 rules instead of retrofitted later.
3. Land Step 4 and Step 5 together; they define the executable seed loop.
4. Land Step 6 immediately after to avoid building unsafe direct-path tooling into the runtime.
5. Land Step 7 before Step 8 so the evaluator and workcell both use the same provider abstraction.
6. Land Step 8 before Step 9; the Stage 0 gate should be assembled, not invented, at the end.

## Explicit Phase 1 Non-Goals

- No parallel workcells.
- No evolutionary canaries.
- No full `//a2:repair_coverage` implementation.
- No generated sentinels.
- No production telemetry or external repository coupling.
- No mutable constitutional semantics.

These remain later-phase work. Phase 1 should produce a narrow, testable seed that satisfies the Stage 0 gate honestly rather than simulating later-stage capabilities.
