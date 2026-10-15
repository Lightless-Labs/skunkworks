# Plan: P4 — Autopoietic Lift

**Created:** 2026-10-15 (drafted while E07b runs; executes after P3)
**Status:** Draft
**Addendum:** 2026-10-15 — P2 gate fixes:
- **Train-label rule:** train-family aggregate pass/fail labels may reach the proposer; test content never does; validation and confirmation outcomes reach no enzyme.
- **Acceptance** uses train mechanism metrics with a fixed threshold. Validation is used only at the end.
- **A random-edit proposer** over the same grammar, at equal search budget, is the control.
- **At least 2 independent searches** from H0 (≤ 5 generations each), and every search is reported.
- **New slots:** a post-green action and a capped 'lessons' list the architect can append to.
- **Budget** (full accounting): ≤ 5k invocations.
**Parent:** `docs/plans/2026-10-14-lift-first-roadmap.md` § P4
**Grounding:** `docs/research/2026-10-14-dgm-rqgm-relevance.md`; E07: the seed metabolism ≈ the plain repair loop (+2.7 pp, CI [−4.7, +11.5]) at 3.7× the tokens.

## Question

Does A²D's self-modification produce a harness that beats the frozen P3 harness and the stronger of B and D on fresh families (Confirmation-2), at the equal invocation cap?

## What may evolve: the harness as data

P4 hypothesis 1 (roadmap) is "the architect edits the coder harness". Today the architect's only actuator is a `SystemPatch` to Rust source, which is rejected in lift mode. Self-modification must not touch the controller, oracles or ledger (CONSTITUTION Invariant 4), so the evolvable surface is a **harness file** that arm C loads per run:

- coder system prompt and repair-prompt template;
- which artefacts feed the coder (failure report, tester review, previous attempt), and in what order;
- the per-task schedule within k: the number of code attempts versus review calls;
- the candidate-ranking rule inside C, on public signals only.

The harness file sits outside the protected paths. Its hash is recorded in the registration, and the controller and oracle remain frozen.

## Data

A **development set authored in-repo** (visible), from the same v2 spec: 12 new families × 4 levels = 48 tasks, split by family into **train** (6 families) and **validation** (6 families). calibration-v2 can be added to train. The confirmation sets are never used for search.

## Search loop (one "evolution run")

1. Start from the frozen P3 harness H0.
2. For each generation g = 1…G:
   - **Propose.** The architect (Luna, so the treatment model is unchanged) proposes a harness edit, given aggregated public-test outcomes from train only. It never sees hidden results.
   - **Evaluate.** The candidate is evaluated on train by hidden pass rate. Train hidden results are visible to the *search*, but the confirmation sets never are.
   - **Accept.** The candidate is kept if validation improves (ratchet) or, for H2, enters the archive.
3. Output: the best harness by validation.

Search cost (invocations and tokens) is accounted for and reported beside the claim. Budget: about 240 invocations per generation (train plus validation at k = 5); G ≤ 10, so ≤ 2,400 per run.

## Hypotheses (T7: at most 5 in total)

1. **H1 — harness self-modification with a ratchet:** evolved H* vs frozen H0.
2. **H2 — archive vs ratchet:** DGM-style parent selection from the lineage archive.
3. **H3 — co-evolved evaluator** (RQGM): the tester enzyme is frozen per epoch and swapped only when it beats the incumbent on a labelled anchor set. This runs only if H1 or H2 shows fixed-oracle lift.

## Confirmation

- H* and H0 are each scored once on **Confirmation-2**, against max(B, D) and against each other.
- Confirmation-2 has 48 tasks (12 families), which is underpowered.
  - Before P4 registration, extend it blind (Codex-authored and validated, independent-solve audit as for Confirmation-1) towards the 150-task cap.
  - Power-plan by simulating the implemented classifier (`docs/solutions/best-practices/power-plan-the-implemented-decision-rule-2026-10-14.md`).
- Completion (roadmap): success needs H* SUPPORTED against H0 on Confirmation-2 **and** a SUPPORTED A²D configuration against max(B, D).

## Risks

- **Overfitting the search to train.** Mitigated by the family-split validation and by Confirmation-2.
- **Search cost dominating any lift.** The cost is reported as a first-class result.
- **Same-vendor model throughout** (Luna as coder and architect, Sol as author and auditor): recorded.
