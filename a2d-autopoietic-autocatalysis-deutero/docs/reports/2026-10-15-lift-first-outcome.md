# Lift-First Roadmap — Outcome Report

**Date:** 2026-10-15
**Plan:** `docs/plans/2026-10-14-lift-first-roadmap.md`
**Outcome:** Completed (falsified), subject to the closing gate review. The three outcomes are kept separate, as the roadmap requires.
**Evidence:** `docs/experiments/LOG.md` (every entry pre-registered before its run).

## The question

Does A²D build working software more reliably than its own coder model used directly (one-shot, best-of-k, a plain repair loop) at the same invocation budget? And does A²D's self-modification widen that margin?

- **Regime:** single-file Rust tasks from numbered specifications, with deliberately weak public tests and 20+ hidden tests per task; `gpt-6-luna` at low effort, no tools; k = 5 model calls per task.
- **Data:**
  - development sets;
  - two blind confirmation sets authored by Codex `gpt-6.1-sol`, validated, and audited by an independent solver.

## Answer

1. **Seed threshold — RULED OUT** (confirmation-backed).
   - The seed A²D metabolism does not deliver ≥ 10 pp over its coder used as a plain repair loop at an equal invocation cap.
   - Confirmation-1: E07 + E07b, 296 tuples, 37 families. C − D = **+1.4 pp, 95% CI [−5.4, +8.8]**, at 3.7× D's tokens.
2. **Improved harness (P3 and the P4 base) — none found.**
   - Within-task self-signals (spec tests, clause review, agreement selection) failed their pre-registered rules.
   - A repaired base, H0 v6, is the seed coder enzyme with D's exact prompts. It matched D on train (+2.9 pp; never-green 1 vs 1).
   - **On fresh validation families it fell below D: −12.5 pp, CI [−26.2, 0.0], discordant 5 vs 15, never-green 4 vs 0.**
   - **So the configured A²D carrier is not reliably at its coder's level, even with D's prompts.** The cause is open (see E13).
3. **Self-improvement process (P4) — not supported at the development screen; not tested on Confirmation-2; not ruled out.**
   - Lessons written by A²D's architect from its own train failures did not lift it above D on validation: the best informed search, i1, scored −1.2 pp, CI [−12.5, +10.0].
   - That look cannot tell a −10 pp effect from a +10 pp one.
   - **Failure-informed lessons did no better than generic, requirements-derived lessons** (i1 = u1 = 68.8%), so the wording "learning from failures" is withdrawn.
   - By the pre-registered go/no-go (≥ +10 pp over D on validation), Confirmation-2 was not spent.

The roadmap's falsified condition is: the seed or minimal configuration is RULED OUT on confirmation, and no P4 hypothesis is SUPPORTED within T7. That condition is met. The confirmation backing is LOG E07 + E07b.

## Evidence

| Phase | Experiment (LOG) | Result |
|---|---|---|
| P2 | E07 + E07b: seed C vs D, Confirmation-1 (296 tuples, 37 families) | **+1.4 pp [−5.4, +8.8], RULED OUT**; C 3.7× tokens |
| P3 | S1 / S1b: blind spec tests; S2: clause review; S3: agreement selection | killed offline (AUC 0.63 / 0.64; conversion 0.19; 0.73 vs D 0.74) |
| P3 | T0 (Sol taxonomy); P4-0 (Luna label-only diagnosis) | failures concentrate; Luna names the violated clause from labels at 0.56 (chance 0.25) |
| P3 | S4: researcher lessons, validation | +2.5 pp, below its +5 pp rule |
| P3 | S4b: researcher lessons on a repaired base, clean train | +11.1 pp vs its base (net +8 of 72); **+5.6 pp vs D**; development screen only |
| P4 | E10 / E10b (G0) | H0 v5 failed never-green (5 vs 2). **H0 v6 (parity with D) passed: +2.9 pp, 1 vs 1** |
| P4 | E11: 4 searches × 3 generations (2 informed, 2 uninformed control) | 4/12 candidates accepted (2 informed, 2 control), about the noise rate (D retest flip 0.229) |
| P4 | E12: validation look (80 tasks) | D 70.0%; C:h0 57.5%; i1 68.8%; i2 62.5%; u1 68.8%; u2 61.3%. **NO-GO** |
| close | E13: D vs D + `main()` clause, train | {{E13}} |

## What was learned (durable)

1. **A plain repair loop is a strong baseline.** At k = 5 it reaches public-green on about 99% of tasks. Its remaining failures are almost all public-green but hidden-fail: wrong behaviour on clauses the public tests do not exercise.
2. **Within-task self-signals do not beat it.** The model's own tests, reviews and majority votes share its misreadings (`docs/solutions/architectural-insights/self-signals-dont-beat-repair-loop-2026-10-15.md`).
3. **Plumbing dominated the readings:**
   - a missing `main()` clause;
   - the task input rendered three times;
   - JSON-escaped repair inputs;
   - an evolver rewriting the coder's prompt mid-task;
   - same-model escalation calls costing repairs.

   Each looked like a mechanism result until the first prompts were read (`docs/solutions/best-practices/inspect-first-prompts-of-a-new-harness-2026-10-15.md`).
4. **The equal-budget uninformed control prevented a false positive.**
   - The two informed searches each accepted a candidate. Read alone, that looks like learning.
   - The paired control accepted as often, and scored as well on validation.
   - Any self-improvement search needs a control at the same budget, with the same acceptance rule.
5. **The decision noise is larger than the effects sought.**
   - D's test-retest flip rate on 140 train tasks is 0.229. Two runs of one identical harness on the same 68-task half differed by 5 tasks.
   - A ±5 pp effect cannot be resolved with 70-task decisions. Effects of that size need about 4× larger evaluations per decision, or variance reduction.
6. **Train parity does not guarantee parity on fresh families.** H0 v6 matched D on 140 train tasks and fell 12.5 pp below it on 80 validation tasks. A base must be checked on the data it will be judged on.

## Process diagnostics (deutero)

- **Spend (invocations, about 15k envelope at the P2 gate):**
  - P0–P2 about 4.4k, plus about 200 audit;
  - P3 about 1.45k, plus 64 probe calls;
  - P4 about 3.8k (G0 and retry 0.73k, searches 2.18k plus 224 distill calls, validation 0.65k);
  - E13 about 0.4k from the reserve;
  - Confirmation-2 extension authoring and audit: 25 families, 104 audit invocations, 1 adjudication.
- **Results:** P3 10 entries (1 aborted); P4 7 (1 stopped).
- **Decisions by pre-registered rule:** 13. These are the kills of S1, S1b, S2 and S3; S4 deprioritised; S4b survived; E10 fail; E10b pass; 12 ratchet decisions counted as one rule; E12 no-go; the control consequence; T0; P4-0; E08r sanity.
- **Decisions by judgement (all logged):**
  - the choice of mechanisms;
  - S4 relabelled as mechanism 5 after its result (forking path, disclosed);
  - the H0 chain v2 → v3 → v4 → v5 → v6 (each step with a disclosed defect or diagnosis);
  - dropping the evolver (gate);
  - the E13 diagnostic.
- **Pre-result amendments:** 3. The S4b clarification and the stricter S4b amendment were made before the primary comparison existed. The E12 no-look clause was made before any search finished.
- **Defects found:** 5 by inspecting prompts or code (`main()`, triplication, JSON rendering, evolver rewrites, the variant-arm path), and 0 by noticing an outcome first.
- **Deviations:** 2. The Confirmation-2 exposure led to `spreadsheet-eval` being excluded. Aggregate counts were read from validation prompts during the S4 diagnosis.

## What is not shown
- Nothing here rules out the A²D architecture in general, other models or efforts, larger budgets, or tasks whose public tests carry more signal.
- P4 was not tested on confirmation. Its development-screen result is uninformative at ±10 pp.

## Assets left for future work
- **Confirmation-2:** 144 tasks, 36 families, hash `cb7cb496…`. **Never opened, unspent.**
- **c1-validation:** 2 of 4 looks used (S4 and E12).
- **Train:** `tasks/train-all` (140 tasks, 35 families) and its symlink halves.
- **The instrument** (`docs/PROTOCOL.md`):
  - `a2d lift` (multi-harness, `--claim`, deferred scoring, hash-chained ledger);
  - the blind holdout pipeline;
  - `a2d distill`;
  - harnesses H0 v6, S4b and `p4-*`.

## Recommended pivot (each step can be pre-registered)
1. **Close the carrier question first** (cheap).
   - Use E13 to settle whether the `main()` clause costs hidden passes.
   - If it does not, run C(H0 v6) vs D at r = 2 on `tasks/train-all` (about 750 invocations) to tell noise from structure.
   - Then make the metabolism's sandbox accept library files, so the carrier needs no prompt difference at all.
2. **Fix the measurement before searching again.** A harness search needs about 280 tuples per decision (r = 2 on full train), or paired-seed variance reduction, to resolve ±5 pp. Calibrate the acceptance rule against the uninformed control and a no-change control at that size.
3. **Move to a regime where the repair loop is not saturated.** The frontier here is spec misreadings that same-model signals cannot see. Candidates are a signal from beyond the coder (a different-family reviewer, a portfolio claim), higher effort, or tasks with richer public tests. Confirmation-2 is reserved for one registered claim in that regime.
4. **Treat the instrument as a product** (the research-instrument direction from `docs/research/2026-10-14-direction-audit.md`). The controller, blind holdouts, gates and pre-registration discipline are reusable for evaluating any coding-agent harness.
