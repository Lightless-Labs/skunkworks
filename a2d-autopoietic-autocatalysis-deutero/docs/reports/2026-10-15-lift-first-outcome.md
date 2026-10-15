# Lift-First Roadmap — Outcome Report

**Date:** 2026-10-15
**Plan:** `docs/plans/2026-10-14-lift-first-roadmap.md`
**Outcome:** **Completed (falsified: seed RULED OUT on Confirmation-1; self-modification not supported at the development screen, unconfirmed).**
- Under the registered analysis, the frozen seed's ≥ 10 pp lift was ruled out on confirmation, and bounded development found no promotable successor.
- P4 failed its development screen and was not tested on Confirmation-2.
- This closes the roadmap. It is not a verdict on the A²D programme or on autoresearch in general.

**Reviewed:** 2026-10-15, closing gate. Codex `gpt-6.1-sol` (high) and a fresh-context Claude subagent, each in a copies-only temp dir. Both approved the label subject to the wording corrections made here.
**Evidence:** `docs/experiments/LOG.md` (every entry pre-registered before its run; adaptive decisions are listed in § Process diagnostics).

## The question

Does A²D build working software more reliably than its own coder model used directly (one-shot, best-of-k, a plain repair loop) at the same invocation budget? And does A²D's self-modification widen that margin?

- **Regime:** single-file Rust tasks from numbered specifications, with deliberately weak public tests and 20+ hidden tests per task; `gpt-6-luna` at low effort, no tools; k = 5 model calls per task.
- **Data:**
  - development sets;
  - two blind confirmation sets authored by Codex `gpt-6.1-sol`, validated, and audited by an independent solver.

## Answer

1. **Seed threshold — RULED OUT under the registered analysis** (confirmation-backed).
   - The seed A²D metabolism, as built, does not deliver ≥ 10 pp over its coder used as a plain repair loop at an equal invocation cap.
     - "As built" means with the defects found later: no previous-code routing, input rendered three times, JSON rendering, and an evolver rewriting prompts.
   - Confirmation-1: E07 + E07b, 296 tuples, 37 families. C − D = **+1.4 pp, 95% CI [−5.4, +8.8]**, at 3.7× D's tokens.
   - Sensitivity (LOG E07b): the two-look 97.5% upper bound is +9.8 pp, and the stacked worst case is about 10.5 pp. The registered rule governs. The result establishes neither equivalence nor zero benefit.
2. **Improved harness (P3 and the P4 base) — none promoted.**
   - The tested within-task self-signal configurations (Luna-low spec tests, clause review, agreement selection) failed their registered screens. Some showed small development gains over weak bases; S2, for example, had an offline point estimate of +2.2 pp.
   - **The P4 base, H0 v6,** dropped every A²D-specific component except the metabolism carrier.
     - What is left is the seed coder enzyme with D's prompts plus a `main()` clause, running on the metabolism's own sandbox (it compiles a program) and feedback wrapper (`TESTS:`).
     - It passed the train parity screen: +2.9 pp, never-green 1 vs 1.
     - In the validation run it scored −12.5 pp, CI [−26.2, 0.0]: discordant pairs 5 vs 15, family sign-flip p = 0.12 (the task-level p of about 0.04 ignores clustering), and never-green 4 vs 0. D's own draw varies too: it scored 52/80 in S4 and 56/80 in E12 on the same tasks.
     - **Parity on fresh families is not established, and neither is the size of any deficit.**
   - On validation, never-green was 4, 1, 1, 2 and 3 across the five C arms, against 0 for D. On train it does not hold up: pooled over E10, E10b and E13, D's own never-green is 7/420 = 1.7%, against 1.8% across the E11 C-arm submissions. **No carrier-wide never-green excess is established** (corrected by E13; see the addendum).
3. **Self-improvement process (P4) — failed its development screen; not tested on Confirmation-2; not ruled out.**
   - **Against D:** lessons written by A²D's architect from its own train failures did not lift it above D on validation. The best informed search, i1, scored −1.2 pp, CI [−12.5, +10.0], with selection biased in its favour. That was enough for the operational no-go (≥ +10 pp required), not for a confirmatory conclusion.
   - **Against its own base (exploratory, selection-biased):**
     - C:i1 − C:h0 = +11.2 pp, CI [+1.2, +22.5];
     - C:u1 − C:h0 = +11.2 pp, CI [+3.8, +20.0].

     Every lessons arm beat C:h0 and none beat D. As in S4b, lessons show gains only against an under-performing A²D carrier, never against D on held-out data.
   - **Control:** the best informed and uninformed searches tied (55/80 each, differing on 18 tasks; exploratory CI about ±10 pp). The pre-registered consequence followed: **the "learning from failures" attribution is withdrawn.** An advantage from failure information was not demonstrated. The uninformed lessons were requirements-derived and selected by the same label-based ratchet, so they are not "generic advice".

**Completion condition.** The roadmap's falsified condition is that the seed or minimal configuration is RULED OUT on confirmation (LOG E07 + E07b) and that no P4 hypothesis is SUPPORTED within T7. That condition is met, with this disclosure:
- P4 used 1 of T7's 5 hypothesis slots, on a narrowed form of roadmap § P4 hypothesis 1. Only `lessons` could evolve; prompt templates and candidate generation or ranking were not tried.
- Hypotheses 2 (archive vs ratchet) and 3 (co-evolved evaluator) were never run.
- Nothing reached confirmation.

## Evidence

| Phase | Experiment (LOG) | Result |
|---|---|---|
| P2 | E07 + E07b: seed C vs D, Confirmation-1 (296 tuples, 37 families) | **+1.4 pp [−5.4, +8.8], RULED OUT**; C 3.7× tokens |
| P3 | S1 / S1b: blind spec tests; S2: clause review; S3: agreement selection | failed their screens (AUC 0.63 / 0.64; conversion 0.19; 0.73 vs D 0.74) |
| P3 | T0 (Sol taxonomy); P4-0 (Luna label-only diagnosis) | failures concentrate; Luna names the violated clause from labels at 0.56 (chance 0.25) |
| P3 | S4: researcher lessons, validation | +2.5 pp, below its +5 pp rule |
| P3 | S4b: researcher lessons on a repaired base, clean train | +11.1 pp vs its base (net +8 of 72); **+5.6 pp vs D**; development screen only |
| P4 | E10 / E10b (G0) | H0 v5 failed never-green (5 vs 2). H0 v6 passed: +2.9 pp, 1 vs 1 |
| P4 | E11: 4 searches × 3 generations (2 informed, 2 uninformed) | 4/12 candidates accepted (2 informed, 2 control). Under a null at flip 0.229: P(net ≥ 4) = 0.19 per comparison; P(≥ 4 of 12) = 0.18 |
| P4 | E12: validation look (80 tasks) | D 70.0%; C:h0 57.5%; i1 68.8%; i2 62.5%; u1 68.8%; u2 61.3%. **NO-GO** |

**Addendum (2026-10-15, after close): E13** (D vs D + `main()` clause, `tasks/train-all`). This is a diagnostic only; it does not affect the label.
- D 74.3% vs D:main 73.6%: −0.7 pp, CI [−7.1, +5.7], discordant 14 vs 15. **The clause is not the cause.**
- D's own never-green varied from 1 to 4 across three train runs, so the train-side never-green excess claimed in the first version of this report does not hold. It has been corrected in § Answer.
- What remains of the carrier question is C:h0's exploratory −12.5 pp on validation, which is within the range of run-to-run variation at this size.

**Instrument fix found at the gate:** the classifier compared boundaries in floating point, so a CI upper bound of exactly 10.0 pp (8/80) was classed RuledOut. It now uses a tolerance (`stats::classify`, unit-tested). Two exploratory E12 comparisons change to Inconclusive. No registered decision depended on them.

## What was learned (durable)

1. **A plain repair loop is a strong baseline.** At k = 5 it reaches public-green on about 99% of tasks. Its remaining failures are almost all public-green but hidden-fail: wrong behaviour on clauses the public tests do not exercise.
2. **The tested within-task self-signal configurations failed their screens.** In those Luna-low configurations, the model's own tests, reviews and majority votes shared its misreadings (`docs/solutions/architectural-insights/self-signals-dont-beat-repair-loop-2026-10-15.md`). This is not a claim about every self-signal.
3. **Plumbing dominated the readings:**
   - a missing `main()` clause;
   - the task input rendered three times;
   - JSON-escaped repair inputs;
   - an evolver rewriting the coder's prompt mid-task;
   - same-model escalation calls costing repairs.

   Each looked like a mechanism result until the first prompts were read (`docs/solutions/best-practices/inspect-first-prompts-of-a-new-harness-2026-10-15.md`).
4. **Equal-budget controls stop gains against one's own base from being read as learning.**
   - The go/no-go against D was already a no-go, so the uninformed control did not change that decision.
   - What it prevented was reading the four E11 acceptances, and the +11 pp of lessons over C:h0, as learning from failures. The control matched both.
   - Any self-improvement search needs a control at the same budget, with the same acceptance rule. Better still, add an A/A arm that runs the incumbent as its own candidate, to measure false acceptance directly.
5. **The decision noise is larger than the effects sought.**
   - D's test-retest flip rate on 140 train tasks is 0.229. Two runs of one identical harness on the same 68-task half differed by 5 tasks.
   - At that flip rate, even 280 paired tuples give a half-width of about 5.6 pp. Power 0.8 at +5 pp needs about 700, before family clustering.
   - Replicates reduce sampling noise, not task-sampling variance.
6. **Train parity did not carry over to fresh families** in this case (H0 v6: +2.9 pp on train, −12.5 pp on validation; exploratory). Check a base on representative development families before freezing it, at a size able to detect the gap that matters. Outcome-driven repair on evaluation data would consume that data's independence.

## Process diagnostics (deutero)

- **Spend (invocations, about 15k envelope at the P2 gate):**
  - P0–P2 about 4.4k, plus about 200 audit;
  - P3 about 1.45k, plus 64 probe calls;
  - P4 about 3.8k (G0 and retry 0.73k, searches 2.18k plus 224 distill calls, validation 0.65k);
  - E13 capped at 0.4k from the reserve (the realised spend is in its addendum);
  - Confirmation-2 extension: 25 families authored, 104 audit invocations, 1 adjudication.
- **Results:** P3 10 entries (1 aborted); P4 7 (P4-0, E09 stopped, E10, E10b, E11, E12, and E13 pending).
- **Decisions made by a prospectively registered rule** (11):
  - the kills of S1, S1b, S2 and S3;
  - T0 and P4-0 by their use rules;
  - the E08r sanity check;
  - the E10 fail and E10b pass;
  - the 12 E11 ratchet decisions, counted as one rule (all match net ≥ 4 and 0 voids);
  - the E12 no-go and the control consequence (one rule).
- **Adaptive decisions and forking paths** (all logged; not fixed before experimentation began):
  - **S4's rule was not honoured as written.** It said the lessons slot would be deprioritised in P4. Instead S4 was relabelled as P3's mechanism 5 after its result, and its revision S4b made lessons P4's only lever.
  - The S4b clarification and stricter amendment came before the primary comparison existed, but during generation; one arm's scores existed on disk unread.
  - The E12 no-look amendment was interim-informed: it was made after all four generation-1 rejections were visible. It ended up inactive.
  - The base chain H0 v2 → v6 was a series of registered adaptations, each after a diagnosed defect or a failed G0. H0 v6 is not the originally frozen base.
  - Also: the choice of mechanisms, dropping the evolver (gate), and the E13 diagnostic.
- **Asymmetry, disclosed:** defect hunts ran after disappointing results (S4, E10), never after favourable ones (S4b, E10b).
- **How defects were found.** 5 were identified by inspecting prompts or code (`main()`, triplication, JSON rendering, evolver rewrites, the variant-arm path). Each investigation was prompted by something else:
  - an outcome in three cases: S4's arm-level breakdown (triplication), E10's FAIL (the repair-path parity), and a crash (the variant-arm path);
  - a review in one: the gate review, for the evolver;
  - an interim public-green read in one: E08's `main()`.
- **Deviations:**
  - the shared-account condition during E07 and E07b, later made a standing condition;
  - the Confirmation-2 argv exposure (`spreadsheet-eval` excluded before hashing and before any scoring);
  - aggregate counts read from validation prompts during the S4 diagnosis.

## What is not shown
- Nothing here rules out:
  - the A²D architecture in general;
  - other models or efforts, or larger budgets;
  - tasks whose public tests carry more signal;
  - the untested P4 hypotheses: evolving prompt templates or candidate ranking, archive vs ratchet, and a co-evolved evaluator.
- P4 was not tested on confirmation. Its validation look (i1 vs D −1.2 pp [−12.5, +10.0], with selection favouring i1) supported the operational no-go, not a confirmatory conclusion.

## Assets left for future work
- **Confirmation-2:** 144 tasks, 36 families, hash `cb7cb496…`, unspent.
  - One family (`spreadsheet-eval`) was excluded after an argv exposure, before the hash was taken.
  - The remaining 36 families were never opened.
- **c1-validation:** held out from P4 training; looked at twice (S4 in P3, E12 in P4); 2 of 4 looks remain. It is no longer untouched.
- **Train:** `tasks/train-all` (140 tasks, 35 families) and its symlink halves.
- **The instrument** (`docs/PROTOCOL.md`):
  - `a2d lift` (multi-harness, `--claim`, deferred scoring, hash-chained ledger, boundary-tolerant classifier);
  - the blind holdout pipeline;
  - `a2d distill`;
  - harnesses H0 v6, S4b and `p4-*`.

## Recommended pivot (in order)
1. **Check the carrier at zero model calls first,** on visible train artefacts (E13 has already shown that the `main()` clause is not the cause):
   - byte-diff the coder-visible repair prompts of C:h0 and D for identical (task, code, test-output) states;
   - mine the 24 E11 C-arm runs (1,696 submissions) against D in E10 and E10b for never-green and repair conversion, by failure type;
   - read E13 (the `main()` clause) with its limits: a train run cannot explain a validation gap, and E10b already measured "D + clause + carrier" at +2.9 pp on train;
   - only if still needed, a pre-registered C:h0 vs D look on c1-validation (2 looks remain), aggregates only.

   Ship a library-file sandbox fix either way, so the carrier needs no prompt difference. Future harness changes are best run as "D + X vs D", since the carrier adds nothing.
2. **Calibrate measurement before any further search.**
   - Simulate the implemented acceptance rule and classifier with the family structure (35 train families) and repeated-run discordance (flip 0.229).
   - Pre-register the target effect, error rates, sample size (expect about 700 tuples for 0.8 power at +5 pp), budget and an inconclusive outcome.
   - Include an A/A arm and an uninformed control at equal budget.
3. **Name one next hypothesis as "D + X vs D", with X's information source stated.**
   - The frontier is spec misreadings that same-model signals cannot see.
   - A different-family reviewer runs into the available lanes (Codex and Claude only) and the equal-resource rule: D must be offered the same signal.
   - Higher effort changes cost per call.
   - Tasks with richer public tests change the task distribution, so Confirmation-2, authored in the current regime, could not serve as the claim set there.
   - **Keep Confirmation-2 unspent** until a frozen candidate meets a prospectively specified development gate in its own regime.
4. **The instrument as a product** (the research-instrument direction in `docs/research/2026-10-14-direction-audit.md`) needs a success criterion and an owner decision. The classifier fix above is its first hardening step.
