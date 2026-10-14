# A² Value-Test-First Plan (E1 / E2)

**Created:** 2026-10-14
**Status:** Active — supersedes the execution order of `2026-07-10-a2-remediation-roadmap.md`
**Decision basis:** `reviews/2026-10-14-direction-audit.md` (Option B). The owner delegated the decision on 2026-10-14.
**Decision deadline:** 2026-11-30. No E1 decision record by then = **NO GO**. No extensions.

## Decision

Stop building gates before any value datum exists. Run one cheap, pre-registered external-value experiment (E1). Run the heredity experiment (E2) only if E1 is GO. Remediation Phases 1–8 are frozen and conditional on E1 GO. Phase 0 is frozen with one known gap: `//crates/a2_workcell:a2_workcell_test` is red under Bazel because one test spawns `cargo`.

## E1 — Does the A² loop beat plain retry?

**Arms** (same model, same task, matched token/wall-clock/attempt budgets, randomized order):

1. `raw` — a single attempt.
2. `stateless-retry` — up to k attempts with identical verifier feedback, a fresh context each attempt, no lineage, no ContextPack.
3. `a2-no-lineage` — A² loop with persistent lineage disabled.
4. `a2-full` — the full A² loop.

**Primary metric:** resolved rate, `a2-full` vs `stateless-retry`, on paired tasks. **GO** requires ≥ +15 pp, using an exact McNemar test or paired bootstrap, one-sided α = 0.10. **INCONCLUSIVE = NO GO.**

**Oracle separation (mandatory).** All arms receive feedback only from public/visible checks (pass-to-pass tests, or the agent's own test runs). Hidden fail-to-pass tests score the final artifact once, after the run. Hidden-test output never reaches any arm. (A²D's audit found exactly this leak.)

**Boundaries.**
- Agent arms run under the existing `sandbox-exec` `AllowList` profile, which permits model API endpoints only (`cd88992`). Do not rebuild this.
- Evaluation runs in the task's container with `--network none`.
- The task checkout is created without upstream history: a fresh single commit at the base, so neither the fix nor an injected bug is visible in `git log`.

**Task source.**
1. First verify the public SWE-Bench Pro split directly: dataset card, harness repo, image availability and per-image size. Record what was verified.
2. If it does not hold, fall back to the Senior SWE Bench export, using the existing sanitized-provenance tooling (`a030380`, `682e8c0`, `5af7602`).
3. Choose 30–60 tasks and record the export digest and the sample.

**Calibration.** Run about 5 tasks on `raw` first. If raw pass@1 falls outside roughly 20–60%, change the model or task subset, then re-calibrate. Calibration tasks are excluded from the E1 sample.

**Host constraints.**
- About 58 GB of disk is free, so each task is pull → evaluate → evict.
- Resize Colima before use; it is currently 2 CPU / 2 GiB.
- Budget wall-clock for image pulls.

**Pre-registration.** Commit `bench/external-value/protocol-v1.toml` **before the first non-calibration provider call**. It records:
- dataset digest, sample, model, k, budgets and arm order seed;
- metric, test and threshold;
- the infrastructure-failure taxonomy and reserve-task rule;
- the stop conditions.

Never edit it after outcomes are observed. Corrections go in a dated addendum.

**Output:** `docs/benchmark-results/external-value/e1/` holding the per-task rows and a decision record (GO / NO GO / INCONCLUSIVE) bound to the protocol digest and source commit.

## E2 — Do self-authored changes compound? (only if E1 is GO)

1. Make 3 B0-reviewed, A²-authored changes to prompts, policies or retry strategy. Admit them manually, as reviewed commits on `main`; the live `--apply` paths stay locked.
2. Re-run the E1 battery, or a fresh held-out split, at generations N and N+3.
3. **GO** requires ≥ +15 pp, using the same test. INCONCLUSIVE = NO GO.

## Re-decision table (from the audit)

| Result | Decision |
|---|---|
| E1 NO GO / INCONCLUSIVE | Stop A² as a research runtime. Package it as an eval harness (Option D) or archive it with a post-mortem (Option E). Offer the protocol to A²D as its entry gate. |
| E1 GO, E2 NO GO | A² is a better retry loop, not a self-producing system. Take Option D, or keep it as DESIGN.md's Stage-2 reflexive patching tool. Do not resume Phases 3/4/8. |
| E1 GO, E2 GO | Resume the roadmap in dependency order (1 → 2 → slim 4). Run Phase 3 and RAF only against admitted trajectories. Then consider consolidating with A²D. |
| No decision by 2026-11-30 | NO GO. |

## Work rules

- **Zero-spend fixes come first:**
  - zero executed tests = failure in `crates/a2_eval/src/seed.rs`;
  - history-free task checkouts;
  - `--arm raw` and `--arm stateless-retry`.
- **Frozen until E1 reports:** no commits to `bench/self_correction_demo.py`, sandbox attestation/validators, or the HANDOFF verification paragraphs.
- **Stalled steps:** two attempts without movement on a blocking step → write a one-page blocker report in `docs/blockers/`, then change the design. Do not make a third tweak.
- **Shared checkout:** follow `AGENTS.md` (work on `main`, no worktrees, no stash, no destructive git, scoped commits, never touch `a2d-*`).
- **Long Cargo runs:** use the bounded liveness harness (`bench/phase0_liveness.py`).
