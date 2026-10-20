# What the loop has learned (living document)

**Status:** interim, as of exp-0009 (2026-10-20). This becomes the plan §1(b) analysis if no variant is promoted. Update it after every few experiments. The sources are [`log.jsonl`](log.jsonl), [`archive/`](archive/) and [`noise-floor.md`](noise-floor.md).

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

## Observations

1. **G0 is nearly deterministic per task.** 40 of 44 dev tasks resolve either always or never in the noise-floor runs. A gain has to come from cracking never-solved tasks. The champion's failures split into:
   - 22 of 44 tasks never pass the visible tests (median 3 visible tests still failing);
   - 5 of 44 pass the visible tests but fail the hidden ones.
2. **Repairing from the candidate is the only family with a repeated positive signal.** exp-0002 and exp-0003 were each +3 on the full pool. exp-0002 reached p = 0.10 at stage B. That is consistent with a real effect smaller than the rule can see: P(keep) is 0.40 at +1 logit per task and 0.12 at +0.5. It is not evidence of an effect, and the rule stands. exp-0008, a third repair variant (repair only when the candidate passes at least one visible test), again landed in the same place: +2.33 on the full pool, p = 0.17 at stage B. Three repair variants, all positive on the full pool (+3, +3, +2.33) and none significant, is the clearest pattern so far. It suggests a real effect of about 2–3 tasks in 44, which the per-experiment rule cannot detect. The rule stands for this campaign. Whether to pool across a family, or run a larger stage B for a family that keeps landing positive, is a design question for the next campaign.
3. **The first move into the other failure mode did not clear the screen.** exp-0005 reviewed and repaired candidates that already passed the visible tests. That failure mode covers only 5 of 44 tasks, so even a perfect fix has little headroom on S.
4. **Selecting by visible score across fresh attempts did not help** (exp-0001, −1). Visible-test partial credit is a weak proxy for hidden resolution.
5. **Eight of the nine hypotheses tune call sequencing or how feedback is routed between calls.** These include best-of-n, repair versus restart, effort mix and failure aggregation. None changes what the agent does inside a call. `program.md` lists plan-then-execute and reproduce-then-fix. exp-0009 tried plan-then-execute and lost ground (−0.66): giving a whole call to planning leaves only two calls to implement and repair. Reproduce-then-fix has not been tried. This is the main input to the "next genome" section below.
6. **Lowering effort hurt** (exp-0006, −0.83). Effort diversity is not a cheap source of variance worth exploiting.
7. **Cost shape:**
   - a stage-A discard costs about 17–20 M tokens and 1–2 h;
   - an experiment that reaches stage B costs 75–90 M tokens and 6–13 h.

   Twenty-two experiments are likely to total 0.6–1 B tokens (337 M after 8).

## Working hypotheses for the next genome, if nothing is promoted

- **The search space may be too narrow to clear the bar.** The genome controls only the prompt, the base and the effort of up to 3 calls to a fixed `a2ctl` harness. Detection needs about +1 logit per task. Changes inside this space, such as call sequencing or feedback wording, may all be smaller than that. A wider genome would let the researcher vary the agent's own instructions and tool use, not only the call sequence.
- **The agent cannot run tests inside a call** (verified 2026-10-19 from `bench/evolve/episode.py` and `bench/e1/tasks.py`):
  - each task's venv lives under the evaluator's private dir, which every agent call is denied (`--deny-read`);
  - network egress is allowlisted to the model provider only, so the agent cannot install dependencies.

  All test feedback therefore arrives between calls, through the policy, and the 3-call budget allows at most 2 rounds of it. The E1 a2-full arm has the same limit. This may be why changing the call sequence moves so little. The strongest candidate for the next genome is an **in-call visible-verifier tool**: a sandboxed, evaluator-owned command the agent can run while it works, returning the same scrubbed visible results the policy sees. That is an evaluator change, so it would need its own gate, and it applies to the next campaign, not to this one.
- **Target where the headroom is.** Half the pool never passes the visible tests. Approaches that change what the agent does inside a call, such as reproducing the failure first or running the visible tests before finishing, are more likely to move these tasks than approaches that only reorder calls.

## Design-change rule check

At exp-0007 (2026-10-19), I checked the goal rule "two attempts without movement, then a blocker note and a design change". It does not apply to the loop's experiments. That rule covers my own engineering attempts, as with qutebrowser. The researcher's hypotheses fall under "failures are data". The loop is healthy:
- none of the hypotheses repeats a discarded one;
- spend is on track, at about 250 M tokens for 7 experiments;
- there are no persistent provider errors.

Any widening of the genome's search space is held for the §1(b) recommendation, so the campaign's frozen setup is not changed mid-run.

To be revised as experiments 0010–0022 land.
