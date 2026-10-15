# Plan: P4 — Autopoietic Lift (self-written lessons)

**Created:** 2026-10-15 (drafted while E07b ran)
**Status:** Draft v2. It awaits the S4 result and the P3 → P4 gate review.
**Addendum:** 2026-10-15 — P2 gate fixes (kept, now concrete below):
- train labels may reach the proposer, but test content never does;
- acceptance uses train metrics, with validation only at the end;
- a control proposer at equal budget;
- at least 2 searches;
- full budget accounting within ≤ 5k.

**Addendum:** 2026-10-15 — v2 rewrite after P3. It is grounded in:
- S1–S3 (self-signals within one task do not beat the repair loop);
- T0 (failures concentrate, and task-agnostic lessons exist);
- P4-0 (Luna-low diagnoses a known bug from labels alone: clause hit 0.56 vs chance 0.25);
- E08r (H0 ≈ D on train).

The H2 (archive) and H3 (co-evolved evaluator) hypotheses are dropped for budget and time; T7 is unaffected.

**Parent:** `docs/plans/2026-10-14-lift-first-roadmap.md` § P4. Rules: `docs/PROTOCOL.md`.

## Question

Does A²D's self-modification — its architect writing task-agnostic lessons from its own train failures — produce a harness H* that beats the frozen H0 and the stronger of B and D on fresh families (Confirmation-2), at the equal invocation cap?

The lever is the one thing a plain repair loop cannot do: **learn across tasks**. In P3, every within-task self-signal failed. Most remaining failures are systematic misreadings that recur across families (T0), and the same model can diagnose them once it knows a bug exists (P4-0).

## What evolves

- Only the harness `lessons` key: at most 8 task-agnostic lessons, rendered after the task input for every enzyme.
- `a2d distill` writes it (`crates/a2d-cli/src/lift_distill.rs`). The architect is `codex/gpt-6-luna` at low effort, the treatment model.
- Distill's inputs are the public-green, hidden-fail submissions of a scored train run: requirements plus public tests, the file, and the label.
- Test content and hidden output never reach a prompt (sentinel unit test). Lessons that name a train family or API identifier are dropped.
- Everything else is frozen at **H0 v3** (`harnesses/h0.json` + `stop_at_public_green`). The controller, oracle and ledger stay frozen.

## Data
- **Train** (visible): calibration-v2 (24) + dev-v2 (48) + c1-train (68) = 140 tasks, 35 families, split by family into halves T_a / T_b with a seeded split per search.
- **Validation:** c1-validation (80 tasks, aggregates only), with 3 looks left after S4.
- **Confirmation-2:** 144 tasks, 36 families, hash `cb7cb496…`. It is never used for search.

## Search (one run of H1)
1. **G0:** run C(H0) on all train, with one scored development run per suite.
2. **Generation g = 1…4:**
   - **Propose.** `a2d distill` reads the incumbent's failures on half X (alternating T_a / T_b) and writes candidate lessons.
   - **Evaluate.** Run C(candidate) on all train.
   - **Accept** if the candidate's hidden passes on the held-out half Y are at least the incumbent's + 2 tasks (fixed). Otherwise keep the incumbent. The incumbent's failures for the next generation come from its own latest run.
3. **Output:** the final incumbent H*_s.

The cost per generation is about 140 × 1.6 + about 20 distill calls ≈ 245. That makes about 1,000 per search with G0.

## Arms of the search experiment
- **H1 search × 2:** independent seeds and splits.
- **Control: uninformed lessons.** It uses the same loop, budget and acceptance rule, but the architect writes lessons from the requirements of failed tasks *without* the code or the failure label. It tests whether learning from failures matters more than generic advice.

## Validation (one look)
- C(H*_1), C(H*_2), C(control*) and C(H0), with D alongside, on c1-validation (about 600 invocations).
- The H* that enters Confirmation-2 is fixed by a pre-registered rule: the better validation rate of the two searches. Ties go to search 1.

## Confirmation-2 (one registered run, scored once)
- **Arms:** D, B, C(H0) and C(H*). k = 5, r = 1, seeded tuple and arm order. About 144 × 9.3 ≈ 1,340 invocations.
  - If the controller still takes one `--harness` per run, use two registered runs launched together and `a2d lift compare` across them. A multi-harness arm C is preferable and is a researcher change to the controller, registered beforehand.
- **Claims** (roadmap completion):
  - C(H*) vs C(H0): self-modification widens the margin;
  - C(H*) vs max(B, D): A²D beats its coder used directly.
  - Each uses the implemented classifier.
- **Power** (`docs/experiments/analysis/c2_power.py`):
  - P(SUPPORTED | +15 pp) = 0.86–0.90;
  - P(RULED OUT | 0) = 0.62 at r = 1, a recorded shortfall.

## Budget (≤ 5k for P4, Confirmation-2 separate)
- Two searches about 2,000, control about 1,000, validation look about 600: total ≈ 3,600.
- Spend (invocations and tokens) is reported beside every result.

## Risks
- **Author-style transfer.** Lessons learn what Sol's hidden tests probe. Confirmation-2 shares the author, so the claim is scoped to that task distribution.
- **Noisy acceptance.** About 70 tasks per held-out half; the control at equal budget calibrates how often noise is accepted.
- **Same-vendor models throughout.** Luna is the coder and architect; Sol is the author and auditor.
