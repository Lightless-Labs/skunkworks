# What the loop has learned: campaign 1 analysis (plan §1(b))

**Status:** final (2026-11-03). All 22 experiments and the pre-registered exp-0012r re-run are done. No variant was kept, so the sealed set stays untouched and this document is the plan §1(b) analysis.
**Reviewed:** 2026-11-03, by Codex `gpt-6-luna` (xhigh), with every per-task claim checked against the raw rows and the stage-B power simulated. Its corrections are applied. The sources are [`log.jsonl`](log.jsonl), [`archive/`](archive/) and [`noise-floor.md`](noise-floor.md).

## Campaign so far

| Exp | Hypothesis family | Failure mode targeted | Δ_A | Δ_B (p) | Full 44 | Tokens | Decision |
|---|---|---|---|---|---|---|---|
| 0001 | best-of-n by visible score | never visible-pass | −1.0 | — | — | 20 M | discard (A) |
| 0002 | continue from candidate, repair failures | never visible-pass | +4.0 | +3.5 (0.10) | +3 | 74 M | discard (B) |
| 0003 | repair once, then fresh failure-informed restart | never visible-pass | +2.0 | +0.5 (0.80) | +3 | 90 M | discard (B) |
| 0004 | repair the best partial candidate | never visible-pass | +1.67 | — | — | 17 M | discard (A) |
| 0005 | independent review and repair of a visible-pass candidate | visible-pass, hidden-fail | +1.67 | — | — | 17 M | discard (A) |
| 0006 | medium-effort independent 2nd attempt, then configured effort | never visible-pass | −0.83 | — | — | 13 M | discard (A) |
| 0007 | aggregate failures and regressions across fresh retries | never visible-pass | −1.18 | — | — | 18 M | discard (A) |
| 0008 | repair only partial-pass candidates, fresh restart after zero-pass | never visible-pass | +1.33 | +1.83 (0.17) | +2.33 | 86 M | discard (B) |
| 0009 | plan-only call, then implement from the plan | never visible-pass | −0.66 | — | — | 18 M | discard (A) |
| 0010 | reproduce-only first call, then fix | never visible-pass | +0.41 | — | — | 16 M | discard (A) |
| 0011 | low-effort fresh 2nd attempt, then configured effort | never visible-pass | −0.59 | — | — | 18 M | discard (A) |
| 0012 | continue only when the visible pass count strictly improves, else restart | never visible-pass | passed | — | — | 16 M | invalid (disk full in B) |
| 0012r | pre-registered re-run of 0012 from its archived patch | never visible-pass | +0.46 | — | — | 17 M | discard (A) |
| 0013 | contrast the two latest verifier runs (persistent, new, recovered) | never visible-pass | +1.18 | — | — | 17 M | discard (A) |
| 0014 | one repair call after an execution-error first run | never visible-pass | +0.50 | — | — | 16 M | discard (A) |
| 0015 | medium effort first, escalate to configured effort | never visible-pass | +0.50 | — | — | 16 M | discard (A) |
| 0016 | same rule as 0012 (continue only on a strict visible improvement), reworded | never visible-pass | +0.50 | — | — | 18 M | discard (A) |
| 0017 | pass the strongest candidate's passing checks to fresh retries as preservation targets | never visible-pass | −1.50 | — | — | 18 M | discard (A) |
| 0018 | continue when a candidate passes a check no earlier one did | never visible-pass | +0.50 | — | — | 19 M | discard (A) |
| 0019 | audit a candidate that passed visibly only after a failed attempt | visible-pass, hidden-fail | +0.50 | — | — | 21 M | discard (A) |
| 0020 | repair the strongest candidate only when it passes a majority of visible tests | never visible-pass | +2.39 | +1.89 (0.13) | +1.39 | 84 M | discard (B) |
| 0021 | two independent retries, then repair the strongest on the last call if it passed a visible check | never visible-pass | +3.39 | +0.89 (0.35) | +6.39 | 94 M | discard (B) |
| 0022 | omit verifier feedback after two identical fresh attempts | never visible-pass | −1.54 | — | — | 19 M | discard (A) |

## Observations

1. **G0 is nearly deterministic per task.** 40 of 44 dev tasks resolve either always or never in the noise-floor runs. A gain has to come from cracking never-solved tasks. The champion's failures split into:
   - 22 of 44 tasks never pass the visible tests (median 3 visible tests still failing);
   - 5 of 44 pass the visible tests but fail the hidden ones.
2. **Repairing from the candidate is the only family with a repeated positive signal.** exp-0002 and exp-0003 were each +3 on the full pool. exp-0002 reached p = 0.10 at stage B. That is consistent with a real effect smaller than the rule can see: P(keep) is 0.40 at +1 logit per task and 0.12 at +0.5. It is not evidence of an effect, and the rule stands. exp-0008, a third repair variant (repair only when the candidate passes at least one visible test), again landed in the same place: +2.33 on the full pool, p = 0.17 at stage B. Three repair variants, all positive on the full pool (+3, +3, +2.33) and none significant, is the clearest pattern so far. It is consistent with a real effect of about 2–3 tasks in 44, which the per-experiment rule cannot reliably detect, but it does not establish one. The rule stands for this campaign. Whether to pool across a family, or run a larger stage B for a family that keeps landing positive, is a design question for the next campaign. By the end of the campaign, five repair variants had reached stage B (0002, 0003, 0008, 0020, 0021). All five were positive on both fresh stage-B measures: on the screen reps (+3.5, +0.5, +1.83, +1.89, +0.89) and on the full pool (+3, +3, +2.33, +1.39, +6.39; mean +3.2 tasks, about 7 pp). None reached p ≤ 0.05. One caveat limits how much this pattern says: all five are compared with the same champion rates. If those rates are slightly low, every delta shifts up together, so the five are not independent evidence.
3. **The first move into the other failure mode did not clear the screen.** exp-0005 reviewed and repaired candidates that already passed the visible tests. That failure mode covers only 5 of 44 tasks, so even a perfect fix has little headroom on S.
4. **Selecting by visible score across fresh attempts did not help** (exp-0001, −1). Visible-test partial credit is a weak proxy for hidden resolution.
5. **Ten of the eleven hypotheses tune call sequencing or how feedback is routed between calls.** These include best-of-n, repair versus restart, effort mix and failure aggregation. None changes what the agent does inside a call. `program.md` lists plan-then-execute and reproduce-then-fix. exp-0009 tried plan-then-execute and lost ground (−0.66): giving a whole call to planning leaves only two calls to implement and repair. exp-0010 tried reproduce-then-fix under the fixed classifier and landed at +0.41, below the screen bar (item 8 explains why). This is the main input to the "next genome" section below.
6. **Lowering effort did not help, three times.** exp-0006 (medium-effort 2nd attempt) was −0.83, exp-0011 (low-effort 2nd attempt) was −0.59, and exp-0015 (medium first, escalate) was +0.50, below the screen bar. exp-0011 and exp-0015 are near-repeats of a refuted direction: the same call shape with a lower effort. The researcher may be running out of ideas inside the current search space. That is more evidence for widening the genome (below).
7. **Cost shape:**
   - a stage-A discard costs about 17–20 M tokens and 1–2 h;
   - an experiment that reaches stage B costs 75–90 M tokens and 6–13 h.

   The 22 experiments totalled 724 M tokens and about 116 h of loop time (2026-10-16 to 10-22). Five reached stage B.
8. **Reproduce-first stalls because the agent cannot run anything.** exp-0010's first call asked for a reproduction and no fix. Agents reported that they could not run one: "`ansible-test` requires Python 3.11–3.13, but only Python 3.9.6 is available; `pytest` and `pexpect` are also missing". In 34 of 36 first calls they made no edits at all. Under the fixed classifier the re-run scored those stalls as failures and still reached +0.41, because the two remaining calls recovered most tasks. This is the most direct evidence yet for the in-call verifier as the next genome (below).
9. **The loop's own evaluator had a scoring hole, which exp-0010 exposed.** a2ctl reports `tokens=0` for timeouts and no-edit calls. The evaluator read every zero-token call as a provider error, so these agent failures got a free retry, and were dropped from scoring when the retry also failed. A genome that timed out or stalled on hard tasks would have had those tasks excluded. exp-0010 hit the hole rather than exploiting it: 17 of its 19 screen rows were flagged and the infra guard stalled the loop. The classifier now reads the call logs (protocol-v2 `[addendum_2026_10_20]`, noise-floor addendum 2026-10-20). exp-0001 to exp-0009 were decided under the old classifier. Both sides got the free retries, and those decisions stand.
10. **exp-0012 passed stage A and was lost to a full disk in stage B** ([blocker](../../blockers/2026-10-20-disk-full-exp-0012.md)). It is the fourth repair-family variant, and the first to pass the screen since exp-0008. It is due a re-run from its archived patch. exp-0016 has since tested the same policy logic (only the repair prompt's wording differs) and scored +0.50 at stage A, below the bar. That is an independent replication, and it suggests exp-0012's stage A pass was partly screen noise. The pre-registered re-run, exp-0012r, ran on 2026-11-03 from fresh rows and scored +0.46 at stage A, below the bar of +1.46: discard. Two fresh runs of the same rule (0016 and 0012r) both fell short, so the original pass was most likely screen noise.

## Why nothing was kept: sparse per-task effects against a rule that averages

The per-task rows explain the campaign better than the per-experiment deltas. The counts below cover each variant's runs across stages A, B and the full pool, unless a stage is named. Champion rates include the control runs. This section was reviewed by Codex `gpt-6-luna` (xhigh) against the raw rows on 2026-11-03, and its corrections are applied.

- **Continuing from the candidate goes with solving one hard task.** The champion never solves e1-1661158753b6 (0 of 9 runs, including the last control on 2026-10-22). Every variant whose policy continues from the agent's own candidate solves it: 0002, 0003, 0004, 0008, 0010, 0012, 0016, 0020 and 0021, in 26 of 26 runs. Every other variant fails it (0 of 12 runs across 0001, 0005, 0007, 0009, 0011, 0013–0015, 0017–0019 and 0022). Those variants either restart fresh, or repair only under a condition that did not fire here. The pattern is strong, but it is not a controlled test. The variants differ in more than continuation (extra calls, feedback timing), so it points to a mechanism without isolating one.
- **A few more tasks move partly, again mostly under repair:**
  - e1-275cc04c8249: champion 4 of 10. 0008 resolves it 4 of 4; 0002, 0020 and 0021 each 3 of 4 (B plus full only: 3/3, 2/3, 3/3, 2/3).
  - e1-4b3bbfad2587: champion 2 of 9. 0002 and 0021 each 3 of 4 (B plus full: 2/3 each).
  - e1-9664d5ebf43e (not on the screen): champion 0 of 2. Resolved in the single full-pool run of 0002, 0008 and 0021.
  - Of the other 8 screen tasks the champion never solves, one moved: e1-fb8171e6ddfc, in 2 of exp-0020's 4 runs. A few unlisted tasks also flip in single full-pool runs (e1-d01a171a6d7e in 0002; e1-32a5795ee41f in 0003 and 0021). A one-run full-pool delta nets all of these against regressions. exp-0021's +6.39 is one run in which most of them landed together, not a stable estimate.
- **The rule has low power against one or two flips.** Δ sums, over the tasks, the mean outcome across reps minus the champion rate, so a deterministic flip of a never-solved task is worth about +1. The bar moved during the campaign, because each control run tightened the champion rates:
  - under the original G0 floor (2 runs per task), keeping a variant needed a Δ_B of 4.0, and stage B's power against 1, 2 and 3 deterministic flips was about 7.5%, 19% and 41%;
  - under the end-of-campaign rates, the 95th-percentile null is about 1.96, and the power is about 20%, 53% and 84%. These figures come from the review's simulation and cover stage B only.

  The power table in `noise-floor.md` assumed a uniform logit shift on every task. The genome space produced sparse flips instead. The repair variants' stage-B deltas, +0.5 to +3.5 and mostly about +2, are what one or two flips look like. This explains the discards. It does not prove these variants had real effects.
- **Each decision used the rates at its own time.** `state.json` keeps only the latest rates. Rescored against today's rates, the five stage-B row sets give about +2.96, −0.04, +1.96, +1.96 and +0.96, so exp-0008 and exp-0020 would sit near today's bar. That rescoring is post hoc and changes no decision. The next evaluator should snapshot the champion rates used for each decision, so every logged delta can be reproduced.
- **What not promoting does and does not show.** The sealed confirmation compares the variant with E1 a2-full on 41 sample tasks (46 gated, minus 5 for calibration), with one-sided exact McNemar. Five wins and no losses would clear it (p = 0.031); one or two net wins would not. A variant worth one or two tasks would very likely have spent the one sealed set without a clear answer. Zero keeps is not evidence that the rule "kept nothing spurious": at 1.7% per experiment, 20 independent experiments carry about a 29% chance of at least one false keep. The campaign shows that no variant tested met the rule. It does not measure the true size of the repair effect, or rule out larger effects from genomes it never tried.

## The next genome to try

The genome controls only the sequencing of up to three calls to a fixed harness, and the agent cannot run anything inside a call (observation 8). Its best move found so far, continuing from a candidate, appears to be worth one to a few tasks in 44. The next campaign should change what the agent can do inside a call, test with a rule that can see sparse effects, and stage its changes so that each gain can be attributed.

1. **Seed G1 = G0 plus repair-from-candidate, as in exp-0008.** Continue from the strongest candidate when it passes at least one visible check, and restart fresh otherwise. exp-0008 itself was not significant (p = 0.17), so this is a seed, chosen as the best lead, not a promotion claim. Measure G1's noise floor before the campaign, as for G0. Its first experiment should be a controlled test of continuation: identical calls with `base` set to the candidate versus fresh, on the hard tasks, so the e1-1661158753b6 pattern is isolated from the other differences.
2. **In-call visible verifier, as an evaluator change with its own gate.** The verifier is an evaluator-owned command the agent can run inside a call. It runs the task's visible tests in a fresh, isolated sandbox and returns the same scrubbed output the policy sees between calls.
   - It must never return hidden test names, gold-patch content or private paths. `assert_no_private_leak` covers both its output and its error paths.
   - Candidate code must not be able to alter verifier state or reach the private dir through it.
   - Calls per episode are capped, and so is their combined CPU and wall time.
   - Repeated runs can overfit the public checks, so the hidden-test resolution stays the only score.
   - The gate is leak regression tests, an escape test, and a dry run on 3 tasks that also measures the added cost.
   - It is added after the G1 floor, as its own step, so its effect is not confounded with the seed's.
3. **Widen the genome to the per-call instructions the harness lets it set.** This means the task-prompt framing and the working method the agent is told to follow (for example, run the verifier before finishing). Each prompt is versioned and hashed. Platform system instructions that the genome cannot control stay out of scope and are named as such.
4. **A decision rule for sparse effects,** pre-registered with the new noise floor:
   - paired champion controls at each stage B, on the same tasks at the same time, with randomized arm order, matched handling of clean runs and infra retries, and a paired analysis. Shared, possibly stale champion rates go, along with the correlated-baseline caveat of observation 2;
   - snapshot the champion rates used for every decision;
   - a per-task replication test next to the summed Δ, using a frequentist-valid statistic. One-sided Fisher exact on the champion's runs against the candidate's fresh runs: 0/9 against 3/3 gives p = 1/220. The error budget is set for the whole campaign, across planned variants as well as eligible tasks. For example, α = 0.05 split over 20 variants × 9 hard tasks needs about 0/12 against 5/5 (p = 1/6188). That fixes a minimum champion sample on the eligible tasks and the number of fresh runs, before any candidate runs. Simulate the rule's power against 1–3 flips at both the floor and end-of-campaign rates, and report the campaign-level false-keep rate, not only the per-experiment one;
   - in `program.md`, state the sealed bar: about five net wins in 41 paired tasks. A dev keep on a one-task flip is a lead for the next round, not a promotion candidate.
5. **Researcher input: the per-task matrix (variants × tasks), with a guard against adaptive overfitting.** The repair pattern was visible in that matrix after exp-0003, and the researcher spent 17 more experiments near it, including near-repeats (0011 and 0015 of 0006, 0016 and 0018 of 0012). The matrix covers dev tasks only. Every follow-up a hypothesis names is counted in the error budget above. The sealed set stays untouched until one pre-registered confirmation.
6. **Fairness of the sealed comparison.** With the verifier, the candidate sees visible results during a call, while E1 a2-full sees them only between attempts. That is not information parity. It is a valid package-level comparison only if the tool and its budgets are the treatment, resource ceilings are matched, and a protocol-v2 addendum records the tool, its caps and the frozen evaluator hash before any sealed run. Such a result would credit the package, not the repair seed or the prompts on their own.

Expected cost, at this campaign's rates:
- about 100 M tokens for the G1 noise floor;
- another about 100 M for the verifier gate and its own floor;
- 20–35 M per screen, plus the paired controls (roughly doubling stage B) and the verifier's added execution, which its dry run will measure.

## Design-change rule check

At exp-0007 (2026-10-19), I checked the goal rule "two attempts without movement, then a blocker note and a design change". It does not apply to the loop's experiments. That rule covers my own engineering attempts, as with qutebrowser. The researcher's hypotheses fall under "failures are data". The loop is healthy:
- none of the hypotheses repeats a discarded one;
- spend is on track, at about 250 M tokens for 7 experiments;
- there are no persistent provider errors.

Any widening of the genome's search space is held for the §1(b) recommendation, so the campaign's frozen setup is not changed mid-run.

