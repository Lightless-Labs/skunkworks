# What the loop has learned (living document)

**Status:** interim, as of exp-0022 (2026-11-03); the campaign's 22 experiments are done, and the pre-registered exp-0012r re-run is in progress. This becomes the plan §1(b) analysis if no variant is promoted. Update it after every few experiments. The sources are [`log.jsonl`](log.jsonl), [`archive/`](archive/) and [`noise-floor.md`](noise-floor.md).

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
2. **Repairing from the candidate is the only family with a repeated positive signal.** exp-0002 and exp-0003 were each +3 on the full pool. exp-0002 reached p = 0.10 at stage B. That is consistent with a real effect smaller than the rule can see: P(keep) is 0.40 at +1 logit per task and 0.12 at +0.5. It is not evidence of an effect, and the rule stands. exp-0008, a third repair variant (repair only when the candidate passes at least one visible test), again landed in the same place: +2.33 on the full pool, p = 0.17 at stage B. Three repair variants, all positive on the full pool (+3, +3, +2.33) and none significant, is the clearest pattern so far. It suggests a real effect of about 2–3 tasks in 44, which the per-experiment rule cannot detect. The rule stands for this campaign. Whether to pool across a family, or run a larger stage B for a family that keeps landing positive, is a design question for the next campaign. By the end of the campaign, five repair variants had reached stage B (0002, 0003, 0008, 0020, 0021). All five were positive on both fresh stage-B measures: on the screen reps (+3.5, +0.5, +1.83, +1.89, +0.89) and on the full pool (+3, +3, +2.33, +1.39, +6.39; mean +3.2 tasks, about 7 pp). None reached p ≤ 0.05. One caveat limits how much this pattern says: all five are compared with the same champion rates. If those rates are slightly low, every delta shifts up together, so the five are not independent evidence.
3. **The first move into the other failure mode did not clear the screen.** exp-0005 reviewed and repaired candidates that already passed the visible tests. That failure mode covers only 5 of 44 tasks, so even a perfect fix has little headroom on S.
4. **Selecting by visible score across fresh attempts did not help** (exp-0001, −1). Visible-test partial credit is a weak proxy for hidden resolution.
5. **Ten of the eleven hypotheses tune call sequencing or how feedback is routed between calls.** These include best-of-n, repair versus restart, effort mix and failure aggregation. None changes what the agent does inside a call. `program.md` lists plan-then-execute and reproduce-then-fix. exp-0009 tried plan-then-execute and lost ground (−0.66): giving a whole call to planning leaves only two calls to implement and repair. exp-0010 tried reproduce-then-fix under the fixed classifier and landed at +0.41, below the screen bar (item 8 explains why). This is the main input to the "next genome" section below.
6. **Lowering effort did not help, three times.** exp-0006 (medium-effort 2nd attempt) was −0.83, exp-0011 (low-effort 2nd attempt) was −0.59, and exp-0015 (medium first, escalate) was +0.50, below the screen bar. exp-0011 and exp-0015 are near-repeats of a refuted direction: the same call shape with a lower effort. The researcher may be running out of ideas inside the current search space. That is more evidence for widening the genome (below).
7. **Cost shape:**
   - a stage-A discard costs about 17–20 M tokens and 1–2 h;
   - an experiment that reaches stage B costs 75–90 M tokens and 6–13 h.

   Twenty-two experiments are likely to total 0.6–1 B tokens (about 390 M after 11).
8. **Reproduce-first stalls because the agent cannot run anything.** exp-0010's first call asked for a reproduction and no fix. Agents reported that they could not run one: "`ansible-test` requires Python 3.11–3.13, but only Python 3.9.6 is available; `pytest` and `pexpect` are also missing". In 34 of 36 first calls they made no edits at all. Under the fixed classifier the re-run scored those stalls as failures and still reached +0.41, because the two remaining calls recovered most tasks. This is the most direct evidence yet for the in-call verifier as the next genome (below).
9. **The loop's own evaluator had a scoring hole, which exp-0010 exposed.** a2ctl reports `tokens=0` for timeouts and no-edit calls. The evaluator read every zero-token call as a provider error, so these agent failures got a free retry, and were dropped from scoring when the retry also failed. A genome that timed out or stalled on hard tasks would have had those tasks excluded. exp-0010 hit the hole rather than exploiting it: 17 of its 19 screen rows were flagged and the infra guard stalled the loop. The classifier now reads the call logs (protocol-v2 `[addendum_2026_10_20]`, noise-floor addendum 2026-10-20). exp-0001 to exp-0009 were decided under the old classifier. Both sides got the free retries, and those decisions stand.
10. **exp-0012 passed stage A and was lost to a full disk in stage B** ([blocker](../../blockers/2026-10-20-disk-full-exp-0012.md)). It is the fourth repair-family variant, and the first to pass the screen since exp-0008. It is due a re-run from its archived patch. exp-0016 has since tested the same policy logic (only the repair prompt's wording differs) and scored +0.50 at stage A, below the bar. That is an independent replication, and it suggests exp-0012's stage A pass was partly screen noise. The pre-registered re-run still goes ahead as written.

## Why nothing was kept: the effects are sparse, and the rule was built for diffuse ones

The per-task rows explain the campaign better than the per-experiment deltas.

- **Continuing from the candidate cracks one task every time.** The champion never solves e1-1661158753b6 (0 of 9 runs, including the last control on 2026-10-22). Every variant whose policy continues from the agent's own candidate solves it: 0002, 0003, 0004, 0008, 0010, 0012, 0016, 0020 and 0021 resolve it in all of their runs (26 of 26). Every other variant fails it (0 of 12 runs across 0001, 0005, 0007, 0009, 0011, 0013–0015, 0017–0019 and 0022). Those variants either restart fresh, or repair only under a condition that did not fire here. This is a mechanism, a hard task whose fix needs a second pass over the same code. It is not noise.
- **Two or three more tasks move partly, again mostly under repair.** The champion's rates and the repair variants' runs across stage B and the full pool (champion rates include the control runs):
  - e1-275cc04c8249: champion 4 of 10; 0008 resolves it 4 of 4, and 0002, 0020 and 0021 each 3 of 4.
  - e1-4b3bbfad2587: champion 2 of 9; 0002 and 0021 each 3 of 4.
  - e1-9664d5ebf43e (not on the screen): champion 0 of 2; resolved in the single full-pool run of 0002, 0008 and 0021.
- **The rest of the hard pool barely moves.** Of the other 8 screen tasks the champion never solves, one moved, once (e1-fb8171e6ddfc, 2 of 4 runs of exp-0020). The full-pool gains (+1.39 to +6.39) come from the tasks above. exp-0021's +6.39 is the case where most of them landed in the same run.
- **The rule needs about four tasks.** Δ sums, over the tasks, the mean outcome across reps minus the champion rate. A deterministic flip of one never-solved task is therefore worth about +1. Keeping a variant needs p ≤ 0.05 at stage B, which was a Δ_B of about +4: a net gain of about four screen tasks. The rule's power table assumed a uniform logit shift on every task. The genome space instead produced sparse flips of one or two tasks, and against those the rule has almost no power. The stage-B deltas of the repair variants (+0.5 to +3.5, mostly about +2) are what one to two flips look like.
- **Not promoting was still right.** One or two tasks in 44 is 2–5 pp. The sealed confirmation compares the variant with the E1 a2-full config on 46 openlibrary tasks, and it could not detect an effect that small either. Promoting on this evidence would have spent the one sealed set on a test with little chance of a clear answer. The rule did its job: it kept nothing spurious, and it showed that this search space produces effects too small to claim.

## The next genome to try

The diagnosis is that the genome controls only the sequencing of up to three calls to a fixed harness, and the agent cannot run anything inside a call (observation 8). Its best move, continuing from a candidate, is worth about one task in 44. The next campaign should change what the agent can do inside a call, and should test with a rule that can see sparse effects.

1. **Seed:** G1 = G0 plus repair-from-candidate, as in exp-0008: continue from the strongest candidate when it passes at least one visible check, and restart fresh otherwise. This is the one mechanism the campaign established. Its noise floor gets measured before the campaign starts, as G0's was. It is a seed, not a promotion claim.
2. **New genome surface, an in-call visible verifier.** This is an evaluator-owned command the agent can run inside a call. It runs the task's visible tests in the evaluator's sandbox and returns the same scrubbed output the policy sees between calls. It never returns hidden test names, gold-patch content or private paths: the existing leak checks (`assert_no_private_leak`) apply to its output. The genome decides when and how the agent is told to use it, and how many times per call. The tool lives in the frozen evaluator and needs its own gate: leak regression tests, a check that a candidate cannot reach the private dir through it, and a dry run on 3 tasks. The E1 a2-full arm sees the same visible results, only later, so the comparison stays fair.
3. **Widen the genome to the agent's own instructions.** These are the per-call system instructions and the working method the agent is told to follow (for example, run the visible verifier before finishing, or keep a failing test list). The current genome edits only `policy.py`'s prompts and sequencing.
4. **Decision rule for sparse effects,** pre-registered with the new noise floor:
   - run a paired champion control at each stage B, on the same tasks at the same time, so all candidates are no longer measured against one shared, possibly stale, set of champion rates (the caveat in observation 2);
   - add a per-task replication test next to the summed Δ. A variant passes if it resolves a champion-never-solved task in all of k fresh runs (k = 3 gives p ≈ 0.001 per task against a 0-in-9 champion), with a Holm correction across the hard tasks and the same full-pool no-regression floor. This keeps the false-keep rate near the current 1.7% while giving power against one or two deterministic flips;
   - say plainly, in `program.md`, that the sealed confirmation needs an effect of roughly 5 tasks in 46. A dev keep on a one-task flip is a lead for the next round, not a promotion candidate.
5. **Researcher input:** give the researcher the per-task resolution matrix (variants × tasks), not only the per-experiment deltas. The repair mechanism was visible in that matrix after exp-0003, and the researcher spent 17 more experiments rediscovering it, including near-repeats (0011/0015 of 0006, 0016/0018 of 0012).

Expected cost, at this campaign's rates: about 100 M tokens for the G1 noise floor and gate, then 20–35 M per screen. The in-call verifier will raise per-episode cost by an amount to be measured in its dry run.

## Design-change rule check

At exp-0007 (2026-10-19), I checked the goal rule "two attempts without movement, then a blocker note and a design change". It does not apply to the loop's experiments. That rule covers my own engineering attempts, as with qutebrowser. The researcher's hypotheses fall under "failures are data". The loop is healthy:
- none of the hypotheses repeats a discarded one;
- spend is on track, at about 250 M tokens for 7 experiments;
- there are no persistent provider errors.

Any widening of the genome's search space is held for the §1(b) recommendation, so the campaign's frozen setup is not changed mid-run.

