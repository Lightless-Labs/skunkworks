# A² Remediation Roadmap

**Created:** 2026-07-10  
**Status:** In progress — Phase 0 (items 1–6 landed 2026-07-10/11; boundary gate addressed 2026-10-14)  
**Addendum:** 2026-10-14 — child-agent containment satisfied by inherited `sandbox-exec` process-tree confinement instead of per-spawn-site wrapping (see Addenda)  
**Reviewed:** 2026-07-10 (independent roadmap review)  
**Source:** 2026-07-10 goal/implementation audit  
**Scope:** Recover A² from a pre-Stage-0 mutation/retry prototype into the smallest defensible self-producing software-factory seed described by `DESIGN.md`.

## Executive Decision

A² currently has useful provider, worktree, patch-capture, verifier-evidence, and retry-lineage plumbing. It does not yet have safe heredity, constitutional admission, a real Stage-0 gate, a sealed mission oracle, measurable external value, or resident autonomy.

Execution must proceed in the order below. Later phases may not be used to route around failed earlier gates. In particular:

- stop expanding archived-demo contracts, provider-coverage matrices, and synthetic fixtures until the deterministic safety and Stage-0 gaps are closed;
- do not call the current six public Cargo checks hidden sentinels;
- do not call a working-tree `git apply` a germline promotion;
- do not enable mutation-capable resident operation before an external-value go decision;
- do not claim RAF closure or differentiation before validated repair evidence exists.

## Remediation Definition of Done

This roadmap is complete only when one current-HEAD objective can be traced through:

```text
trusted objective + independent acceptance plan
  -> isolated candidate
  -> candidate-bound deterministic gates
  -> B0 human approval
  -> atomic Git admission + promotion journal
  -> restarted/reloaded descendant from admitted commit
  -> independently verified external-value result
  -> tested rollback
```

Every arrow must have a durable artifact tied to the exact source commit. Passing internal unit tests or replaying a historical JSONL file is insufficient.

## Phase 0 — Stop the Bleeding and Establish Truth

**Goal:** Make the current maturity, unsafe entrypoints, and verification-liveness state explicit before further mutation work.

### Work

1. Add a machine-readable maturity status with at least:
   - `PreStage0`
   - `Stage0GatePassed`
   - `Stage1B0`
   - `Stage2B0`
2. Report the current implementation as `PreStage0`; list missing Bazel, mission, escrow, promotion, constitution, membrane, and external-value evidence.
3. Rename/relabel the current six-check suite as public developer health checks. Keep compatibility aliases if needed, but do not describe it as hidden or as the Stage-0 sentinel gate.
4. Fail closed on live-germline `task --apply`, `run --apply`, and mutation-capable `autopilot-resident --apply` through the Phase-4 Stage-0 gate and Phase-5 end-to-end trace. Phase-1 transaction tests may use disposable runtime repositories, but no operator germline mutation is re-enabled early.
5. Investigate the current verification liveness failure before accepting any green gate:
   - reproduce one focused Cargo test serially;
   - impose a hard wall-clock bound;
   - capture command, cwd, PID/process group, lock/target directory, child process tree, stdout/stderr tail, and timeout cleanup result;
   - prove no orphaned Cargo, rustc, test, Python, or provider process remains;
   - distinguish build-lock contention, filesystem/target pathology, harness deadlock, and test deadlock.
6. Bootstrap Bazel as the canonical build surface immediately after the liveness diagnosis:
   - add pinned `MODULE.bazel`, `.bazelrc`, root `BUILD.bazel`, and one matching Bazel target for every Cargo crate;
   - establish Cargo/Bazel target-parity checks;
   - require every production change in Phases 1–3 to pass its affected Bazel targets as well as focused Cargo feedback;
   - reserve Phase 4 for completing the real Stage-0 gate targets, mission battery, escrow integration, and independent clean-checkout proof.
7. Reconcile stale plans: `docs/plans/phase-1-implementation.md` must no longer remain merely `Proposed` while later docs imply higher maturity; `docs/plans/continuous-self-iteration.md` must record resident mutation as blocked.

### Primary files

- `README.md`
- `docs/HANDOFF.md`
- `docs/MATURITY.md` (new)
- `docs/plans/phase-1-implementation.md`
- `docs/plans/continuous-self-iteration.md`
- `crates/a2ctl/src/main.rs` (prefer extracting `maturity.rs`)
- process-launch/test helpers in `crates/a2_workcell/` and `crates/a2_eval/` as diagnosis requires

### Acceptance criteria

- User-facing status does not claim Bazel self-hosting, hidden escrow, mission gating, constitutional closure, or measurable value-add.
- Mutation-capable live-tree paths fail before modifying Git state.
- A focused verification command either completes or emits bounded, actionable timeout diagnostics and cleans its complete process group.
- No verification command can hang indefinitely or disappear without a retained failure artifact.

### Evidence and verification

```bash
cargo run -p a2ctl -- status --json
cargo run -p a2ctl -- autopilot-resident --max-runs 1 --interval-secs 0 --apply
python3 bench/agent_network_boundary_check.py --require-sandbox-runtime --json
```

Archive the maturity and liveness reports under `docs/evidence/remediation/phase-0/<HEAD>/`.

### Stop/go gate

Do not run paid providers, add benchmark fixtures, or enable promotion while maturity is overstated, apply paths remain unsafe, Bazel target parity is absent, or verification can hang without bounded diagnostics.

### Authoritative maturity transitions

- `PreStage0`: current state and the only permitted state through Phases 0–3.
- `Stage0GatePassed`: the exact Phase-4 Bazel suite passes from clean developer and independent CI checkouts with valid root-of-trust and escrow inputs.
- `Stage1B0`: Phase 5 proves one current-HEAD objective through approved atomic admission, descendant inheritance, independent acceptance, and rollback while all Stage-0 gates remain green.
- `Stage2B0`: after `Stage1B0`, at least **10 predeclared, distinct, substantive human-assigned internal tasks** are independently accepted, B0-approved, journaled, inherited by descendants, and completed with zero invariant or mission regressions. Documentation-only changes, benchmark-fixture repairs, retries of the same task, and provider-coverage runs do not count toward 10.

No single run or phase implicitly advances maturity beyond the transition it explicitly satisfies.

## Phase 1 — Safe, Atomic, Heritable Promotion

**Goal:** Replace working-tree patch application with a crash-consistent admission transaction.

### Work

1. Introduce typed source and admission records:
   - parent and candidate Git revisions;
   - candidate/patch digest;
   - touched paths and derived mutation scope;
   - gate-report digest;
   - B0 approval receipt;
   - prepared/committed/aborted promotion state;
   - rollback receipt.
2. Remove unconditional `MutationScope::Prompt`; derive scope from protected-path classification.
3. Replace `try_apply_patch()` / `apply_and_verify_patch()` with a single-writer transaction:
   1. require a clean tracked germline and capture `HEAD`;
   2. materialize a candidate from that commit;
   3. apply the exact patch without fuzzy fallback;
   4. commit in the candidate;
   5. run all gates against that candidate commit;
   6. require explicit B0 approval bound to the candidate and gate digests;
   7. recheck clean state and unchanged parent `HEAD`;
   8. admit the candidate commit and finalize exactly one journal entry.
4. Make archive/journal failure fatal to admission.
5. Make interrupted transactions recoverable and idempotent.
6. Implement rollback as a forward, journaled compensating commit or an isolated runtime germline-ref transition. Explicitly prohibit `git reset --hard`, `git clean`, forced checkout, branch-ref rewind, and other destructive tree/history rewrites. Prove rollback preserves unrelated tracked modifications and untracked files while removing only the admitted mutation's effects.
7. Restart or reload from the admitted binary and prove the next workcell uses the admitted commit rather than the old `HEAD` or an uncommitted tree.
8. Return nonzero/structured failure when a requested task is discarded, unverified, unapplied, or not admitted.

### Primary files

- `crates/a2_core/src/{id,protocol,traits}.rs`
- `crates/a2d/src/lib.rs`
- `crates/a2d/src/promotion.rs` (new)
- `crates/a2ctl/src/main.rs`
- `crates/a2_archive/src/{schema,journal,store}.rs`
- `crates/a2d/tests/promotion_transaction.rs` (new)

### Acceptance criteria

- Every admitted commit has exactly one finalized journal entry.
- Every finalized journal entry names the exact admitted commit and parent.
- Failures at every transaction boundary leave the original germline unchanged.
- A subsequent descendant contains and executes the admitted change.
- `--apply` alone is never equivalent to approval.

### Verification

```bash
cargo test -p a2_archive journal -- --nocapture
cargo test -p a2d promotion_transaction -- --nocapture
cargo test -p a2ctl promotion -- --nocapture
```

Use the bounded liveness harness from Phase 0 for every command. Store transaction-failure evidence under `docs/evidence/remediation/phase-1/<HEAD>/`.

### Stop/go gate

Promotion remains disabled outside disposable transaction tests unless failure atomicity, journal/Git reconciliation, descendant inheritance, and rollback are all proved. Live-germline mutation remains disabled until the Phase-4 Stage-0 gate and Phase-5 trace also pass.

## Phase 2 — Authoritative Task Verification and Boundary Enforcement

**Goal:** Make an unrelated non-empty diff with zero tests impossible to promote.

### Work

1. Replace task-provided shell command strings with root-authorized verifier IDs resolved through an operator-owned registry.
2. Represent verifier execution as fixed program/argument vectors with bounded cwd, sanitized environment, timeout, output limits, and candidate digest binding. Do not use `sh -c` for untrusted task metadata.
3. Reject verifier definitions supplied through raw text, untrusted JSONL, model output, or quarantined evidence.
4. Require profile-defined mandatory verifiers; an empty verifier set is a failure, not `0 passed / 0 failed` success.
5. Treat spawn failure, timeout, signal, malformed output, missing executable, stale candidate, and archive failure as gate failures.
6. Remove the broad structural evaluator rule that equates non-empty diff + zero failures with task completion.
7. Limit evaluator override/backstop behavior to root-authorized independent verifiers bound to the same candidate.
8. Run verifiers under the same filesystem, process, secret, and network boundary as candidate execution.
9. Make restricted provider launch enforcement cover the actual process tree. The existing boundary audit must pass, including external child-agent launch surfaces, before fresh evidence is accepted.

### Primary files

- `crates/a2_core/src/protocol.rs`
- `crates/a2_workcell/src/{runtime,worktree_catalyst,sandbox_profile}.rs`
- `crates/a2_eval/src/seed.rs`
- `crates/a2d/src/lib.rs`
- `crates/a2ctl/src/main.rs`
- `constitution/verifier_registry.toml` (new)
- `bench/{self_correction,generate_tasks,agent_network_boundary_check,network_policy_smoke}.py`

### Acceptance criteria

- No authoritative verifier means no promotion.
- Malicious verifier strings are rejected before execution.
- Verification evidence binds task, candidate commit, patch digest, command identity, result, and timeout state.
- The required sandbox runtime/boundary audit passes against the actual launched process tree.

### Verification

```bash
cargo test -p a2_workcell task_verification -- --nocapture
cargo test -p a2_eval -- --nocapture
cargo test -p a2d verifier -- --nocapture
python3 bench/agent_network_boundary_check.py --require-sandbox-runtime --json
python3 bench/network_policy_smoke.py --allowlist-smoke --json
```

### Stop/go gate

Do not enable the Phase-1 promoter even in a disposable end-to-end test until the negative verifier corpus and process-boundary gate pass. Live-germline mutation remains disabled until Phases 4 and 5 pass.

## Phase 3 — Integrate Constitution, Membrane, Sensorium, and Lineage

**Goal:** Make the architectural controls govern the executable path rather than exist as disconnected crates.

### Work

1. Encode all eight organizational invariants and B0 semantics.
2. Replace unconditional repair-coverage success with explicit `NotImplemented` until Phase 8; an unimplemented invariant cannot pass.
3. Load digest-checked constitutional semantics and stage profiles at startup. Missing or mismatched semantics block mutation.
4. Construct and consult `ConstitutionalKernel` and `PolicyMembrane` in the real CLI/governor/workcell path.
5. Replace raw-ingress prompt copying with quarantined `EvidenceBundle`s, typed claims, provenance, risk tier, and privilege tier.
6. Ensure privileged builders consume typed contracts and evidence references, not raw untrusted text.
7. Enforce constitution-derived capability maps at provider, verifier, filesystem, secret, tool, and network boundaries.
8. Make lineage append-only and admission-critical. Query/persistence failure must not silently fall back to empty/no store for mutation-capable paths.
9. Protect evaluator, promoter, membrane, constitution, and escrow paths with stricter mutation classification and approval.

### Primary files

- `constitution/spec/**` (new)
- `crates/a2_constitution/src/{invariants,kernel,profile,verifiers}.rs`
- `crates/a2_membrane/src/policy.rs`
- `crates/a2_sensorium/src/ingest.rs`
- `crates/a2_workcell/src/{runtime,worktree_catalyst}.rs`
- `crates/a2_archive/src/{schema,store,journal}.rs`
- `crates/a2d/src/lib.rs`

### Acceptance criteria

- All enabled invariants have executable, non-vacuous verifiers.
- Unsupported invariants are reported as blocking, not passing.
- Prompt-injection fixtures cannot acquire germline write, approval, escrow, or secret-bearing capabilities.
- Every attempt, verification, approval, promotion, and rollback is linked through immutable lineage.

### Stop/go gate

No Stage-0 claim while any enabled invariant lacks an executable verifier or a privileged path bypasses the membrane.

## Phase 4 — Complete the Actual Bazel Stage-0 Gate

**Goal:** Complete the Bazel foundation bootstrapped in Phase 0 and satisfy the original Phase-1 exit condition from a clean checkout.

### Work

1. Re-audit the pinned `rules_rust` / `Cargo.lock` workspace and Cargo/Bazel target parity established in Phase 0.
2. Make `bazel test //...` compile and test every workspace member before adding Stage-0-specific labels.
3. Implement real targets:
   - `//a2:self_host`
   - `//a2:spawn_workcell`
   - `//bench:public_suite`
   - `//bench:mission_gate`
   - `//bench:seed_sentinel`
   - `//bench:evaluator_meta`
   - `//constitution:patch_controls`
   - `//policy:membrane_checks`
   - `//policy:ingress_partition`
   - `//lineage:integrity`
   - `//lineage:promotion_consistency`
   - `//ops:budget_ceiling`
   - `//ops:diversity_floor`
4. Make self-hosting build Bazel binaries, instantiate a descendant, and execute a deterministic no-network workcell. A Cargo compile alias is insufficient.
5. Create a frozen public mission battery with category floors for useful software-factory work.
6. Create a known-bad mutation corpus and prove evaluator meta-tests reject every mutation.
7. Keep hidden sentinel and root-of-trust contents outside the mutable repository. Commit only handles/digests; fail closed when either operator-mounted bundle is missing, digest-mismatched, altered, or candidate-accessible.
8. Add explicit negative tests for absent and digest-mismatched root-of-trust and escrow bundles.
9. Emit a machine-readable Stage-0 gate report binding source, toolchain, semantics, mission, mutation corpus, root-of-trust digest, escrow digest, and target results.

### Primary files

- `MODULE.bazel`, `.bazelrc`, root `BUILD.bazel` (new)
- `crates/*/BUILD.bazel` (new)
- `a2/`, `bench/`, `constitution/`, `policy/`, `lineage/`, and `ops/` Bazel packages
- `crates/a2_eval/src/{mission,escrow,gate}.rs` (new)
- `bench/mission/**`, `bench/mutation_corpus/**`, `bench/escrow/*manifest*`

### Acceptance criteria

- Required labels exist under `bazel query`.
- The exact Stage-0 suite passes in clean developer and independent CI checkouts.
- Public-suite and mission-category floors pass without hidden-data access.
- Root-of-trust and hidden-sentinel checks fail on absent, digest-mismatched, altered, or exposed bundles.
- Public-suite, mission, mutation-corpus, root-of-trust, and escrow results are recorded in a current-HEAD gate report.

### Verification

```bash
bazel query '//a2:* + //bench:* + //constitution:* + //policy:* + //lineage:* + //ops:*'
A2_ROOT_OF_TRUST_BUNDLE=/operator/root.bundle \
A2_SEED_SENTINEL_BUNDLE=/operator/escrow.bundle bazel test \
  //a2:self_host \
  //bench:public_suite \
  //bench:mission_gate \
  //bench:seed_sentinel \
  //bench:evaluator_meta \
  //constitution:patch_controls \
  //policy:membrane_checks \
  //policy:ingress_partition \
  //lineage:integrity \
  //lineage:promotion_consistency \
  //ops:budget_ceiling \
  //ops:diversity_floor
```

### Stop/go gate

Advance to `Stage0GatePassed` only after the exact suite passes independently. Do not infer Stage 1 from Cargo tests or the old six-check suite.

## Phase 5 — Prove One Current-HEAD End-to-End Objective

**Goal:** Demonstrate the remediation definition of done before broad benchmarking.

### Work

1. Select one small but substantive internal code objective with externally authored acceptance criteria.
2. Run it from a clean committed current HEAD under B0.
3. Require an initial candidate, independent verification, approval, atomic admission, journal entry, descendant inheritance, and rollback assay.
4. Archive concise artifacts for every transition; do not substitute console prose or a generated summary for source evidence.
5. Confirm no sibling project paths are touched and no operator-owned changes are lost.

### Acceptance criteria

- One admitted code change—not documentation-only or test-count-only—survives into the next descendant.
- Its acceptance test fails before and passes after.
- Promotion and rollback receipts match Git and lineage exactly.
- Re-running from the admitted descendant exercises the changed behavior.

### Stop/go gate

Do not begin the external-value campaign until this trace passes. Failure means returning to the owning earlier phase, not adding wrappers around the failed artifact.

## Phase 6 — Run One Sealed External-Value Campaign

**Goal:** Determine whether A² adds value over matched simpler baselines.

### Work

1. Pre-register one contamination-resistant held-out campaign before model calls:
   - dataset/export digest and fixed sample;
   - a minimum sample size justified by a prospective power or precision analysis;
   - equal aggregate token, call, and wall-clock budgets;
   - randomized cohort order;
   - primary metric, confidence method, and effect-size threshold;
   - an objective infrastructure-failure taxonomy plus reserve-task/exclusion rules fixed before outcomes are observed;
   - immutable stop conditions.
2. Use a shared audited sandbox launcher for A² and raw baselines.
3. Run paired cohorts:
   - raw provider baseline;
   - stateless retry with verifier feedback;
   - A² with persistent lineage disabled;
   - full A²;
   - anti-repeat disabled only after the primary comparison.
4. Safety controls—sandboxing, sealed verification, provenance, and promotion consistency—are never ablated.
5. Keep private source locators, solutions, patches, and hidden tests outside prompts, lineage, and checked-in artifacts.
6. Report `GO`, `NO GO`, or `INCONCLUSIVE` against pre-registered thresholds. Do not tune on the sealed set and rerun it.

### Primary files

- `bench/external_value.py` (new)
- `bench/external_value_score.py` (new)
- `bench/external-value/protocol-v1.toml` (new)
- shared provider sandbox code
- `docs/benchmark-results/external-value/<campaign-id>/`

### Go criterion

Full A² must:

- beat the stateless retry baseline on the pre-registered primary metric with a positive lower confidence bound;
- beat the otherwise-identical A² lineage-disabled cohort with a positive lower confidence bound, establishing value from persistent lineage rather than only aggregate scaffold behavior;
- remain within the pre-registered non-inferiority margin against the raw matched-budget baseline;
- have zero boundary, lineage, and promotion violations.

Otherwise resident mutation remains disabled.

## Phase 7 — Enable Bounded Resident Operation Under B0

**Goal:** Permit continuous operation only after safety, heredity, Stage-0, and external-value gates pass.

### Work

1. Require current Stage-0 and external-value gate artifacts at startup.
2. Add leases, bounded run/cost limits, health checks before each cycle, emergency stop, and circuit breakers.
3. Keep per-candidate human approval under B0.
4. Forbid unbounded `--max-runs 0` when mutation is enabled.
5. Stop on stale HEAD, archive failure, sandbox loss, budget breach, invariant failure, or journal inconsistency.
6. Make resident child status authoritative: a failed/no-op/discarded child run cannot be summarized as successful completion.
7. Run ten bounded canary cycles and kill the resident at every transaction boundary to prove recovery.

### Acceptance criteria

- Ten canary cycles complete without boundary, lineage, promotion, or invariant violations.
- Every non-success produces a nonzero/structured failure and stops or advances only under explicit policy.
- No candidate self-approves or changes the approval/gate semantics governing itself.

### Stop/go gate

Any canary violation returns operation to manual B0. Do not weaken gates to preserve residency.

## Phase 8 — Repair Coverage, RAF Evidence, and Differentiation

**Goal:** Pursue the autocatalytic thesis only after real admitted trajectories exist.

### Work

1. Replace generic weighted graph edges with typed `reactant_of`, `produces`, `catalyzes`, `repairs`, and `governs` edges carrying candidate/empirical/validated evidence.
2. Implement constitutive repair coverage over the named mutable kernel components, requiring:
   - a repair path produced by a different component;
   - repeated evidence across task families;
   - independent evaluator agreement;
   - a tested rollback path.
3. Make `//a2:repair_coverage` consume durable evidence. Graph connectivity alone cannot satisfy it.
4. Apply the DESIGN anti-triviality filter before reporting a constitutive core.
5. Seed specifier, builder, critic, and mutator templates manually, then compare specialists with the generalist on held-out mission tasks.

### Acceptance criteria

- `RepairCoverageVerifier` no longer returns unconditional success.
- No causal edge becomes validated without repeated ablation evidence.
- Specialists have distinct tool signatures and mutual non-orphan dependencies.
- Specialists outperform the generalist baseline on the pre-registered held-out comparison.

### Stop/go gate

If specialists do not outperform the generalist or repair coverage remains unvalidated, stay at Stage 2. Do not reinterpret graph density, prompt churn, or evaluator self-modification as differentiation.

## Anti-Wheel-Spinning Rules

These rules apply across every phase:

1. No paid provider run while an earlier deterministic gate is red.
2. No new fixture or provider-coverage sweep before the sealed external-value campaign.
3. Public tests, docs guards, refusal tests, and historical replay are safety/reproducibility evidence—not external value.
4. No claim may advance maturity without current-HEAD command output and durable artifacts.
5. A verifier cannot approve a patch that modifies that verifier in the same transaction.
6. Two implementation attempts without movement in the phase acceptance metric require a blocker report and design change, not another prompt tweak.
7. A failed or inconclusive sealed campaign remains failed/inconclusive; do not erase it through task/provider substitution.
8. HANDOFF records concise current facts and links to evidence. It must not duplicate entire test logs or become the primary implementation artifact.
9. Test counts are inventory, not progress metrics. Track admitted useful changes, externally held-out success, regression escapes, cost, rollback success, and boundary violations.
10. Resident autonomy and RAF work remain disabled on `NO GO` or `INCONCLUSIVE`.

## Immediate Execution Queue

Execute these items in order:

1. Phase-0 maturity relabel and mutation-entrypoint lockout.
2. Diagnose and bound the Cargo/test liveness failure.
3. Bootstrap Bazel and establish Cargo/Bazel target parity for all existing crates.
4. Write failing promotion-transaction tests and add matching Bazel test targets.
5. Implement candidate-bound commit/journal admission without live-tree `git apply`.
6. Replace task-provided shell verification with a trusted verifier registry.
7. Wire constitution, membrane, and fail-closed lineage into the real governor path.
8. Complete the real Bazel Stage-0 public/mission/escrow/meta gate suite.
9. Produce one current-HEAD end-to-end admitted descendant trace.
10. Run one sealed external-value campaign, then decide whether resident autonomy and RAF research receive a `GO`.

## Roadmap Maintenance

- Update the header when reviewed, enhanced, or completed using the repository conventions.
- Add an addendum when execution discovers a gap; do not silently rewrite acceptance criteria after observing results.
- Mark a phase complete only when every acceptance criterion has an authoritative evidence path.
- Keep phase evidence compact and immutable; keep detailed operational logs outside `docs/HANDOFF.md`.

## Addenda

### 2026-10-14 — Child-agent containment via inherited process-tree sandbox

**Gap.** The Phase-0 stop/go boundary gate (`agent_network_boundary_check.py --require-sandbox-runtime`) failed because `@anthropic-ai/sandbox-runtime` was not installed and two external spawn sites — Foundry's `pi-foundry-team` `runDispatch` (loaded globally through the operator's Pi `settings.json` packages) and Pi's example `subagent` extension — start child `pi` processes without their own sandbox wrapper. The gate assumed containment had to be applied at each spawn site.

**Mechanism change.** Phase 2 item 9 asks that enforcement "cover the actual process tree". On macOS, a Seatbelt profile applied by `sandbox-exec` is inherited by every descendant, so a restricted-policy A²-launched `pi` already confines anything it spawns. The gate now accepts either mechanism:

1. `launch_site_sandbox` — the original requirement (runtime installed and each child spawn site wrapped); or
2. `inherited_process_tree_sandbox` — all of:
   - every A²-owned `pi` launch is `sandbox-exec`-materialized for `Isolated`/`AllowList` (existing check);
   - every A²-owned `pi` launch passes `--no-extensions --no-skills --no-prompt-templates` through the shared `pi_provider_args` builder, so settings-package extensions such as Foundry never load (verified against Pi 0.80.6 `resource-loader.js`: `noExtensions` drops all non-CLI extension paths, including settings packages);
   - a live, provider-free probe in the audit using A²'s exact `Isolated` profile text: an unsandboxed control grandchild reaches a loopback listener; a `sandbox-exec` grandchild is denied with `EPERM`. A Rust unit test pins the same property in `a2_workcell::sandbox_profile`.

**Why this is a tightening, not a relaxation.** Under the old mechanism, A²'s Pi children loaded the operator's ambient extensions and the gate depended on code in other repositories. Under the new one, those extensions do not load at all, and containment is a property of A²'s own launch path proven by execution rather than by static inspection of third-party source. The spawn-site findings remain in the report as informational.

**Unchanged blockers.** `usable_sandbox_provider_allowlist_enforced` stays false and fresh provider-backed evidence remains blocked until a provider-backed run records its policy and allowlist evidence. Phase 2 item 9 is not complete: the other providers (Claude, Codex, Gemini, OpenCode) still load their own ambient user configuration, and Seatbelt does not confine work handed to launchd/LaunchServices (`open`, `launchctl submit`); the profiles do not yet deny those mach services.
