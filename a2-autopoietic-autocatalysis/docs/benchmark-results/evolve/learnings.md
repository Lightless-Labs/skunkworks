# What the loop has learned (living document)

**Status:** interim, as of exp-0005 (2026-10-17). This becomes the plan §1(b) analysis if no variant is promoted. Update it after every few experiments. The sources are [`log.jsonl`](log.jsonl), [`archive/`](archive/) and [`noise-floor.md`](noise-floor.md).

## Campaign so far

| Exp | Hypothesis family | Failure mode targeted | Δ_A | Δ_B (p) | Full 44 | Tokens | Decision |
|---|---|---|---|---|---|---|---|
| 0001 | best-of-n by visible score | never visible-pass | −1.0 | — | — | 20 M | discard (A) |
| 0002 | continue from candidate, repair failures | never visible-pass | +4.0 | +3.5 (0.10) | +3 | 74 M | discard (B) |
| 0003 | repair once, then fresh failure-informed restart | never visible-pass | +2.0 | +0.5 (0.80) | +3 | 90 M | discard (B) |
| 0004 | repair the best partial candidate | never visible-pass | +1.67 | — | — | 17 M | discard (A) |
| 0005 | independent review and repair of a visible-pass candidate | visible-pass, hidden-fail | +1.67 | — | — | 17 M | discard (A) |

## Observations

1. **G0 is nearly deterministic per task.** 40 of 44 dev tasks resolve either always or never in the noise-floor runs. A gain has to come from cracking never-solved tasks. The champion's failures split into:
   - 22 of 44 tasks never pass the visible tests (median 3 visible tests still failing);
   - 5 of 44 pass the visible tests but fail the hidden ones.
2. **Repairing from the candidate is the only family with a repeated positive signal.** exp-0002 and exp-0003 were each +3 on the full pool. exp-0002 reached p = 0.10 at stage B. That is consistent with a real effect smaller than the rule can see: P(keep) is 0.40 at +1 logit per task and 0.12 at +0.5. It is not evidence of an effect, and the rule stands.
3. **The first move into the other failure mode did not clear the screen.** exp-0005 reviewed and repaired candidates that already passed the visible tests. That failure mode covers only 5 of 44 tasks, so even a perfect fix has little headroom on S.
4. **Selecting by visible score across fresh attempts did not help** (exp-0001, −1). Visible-test partial credit is a weak proxy for hidden resolution.
5. **Cost shape:**
   - a stage-A discard costs about 17–20 M tokens and 1–2 h;
   - an experiment that reaches stage B costs 75–90 M tokens and 6–13 h.

   Twenty-two experiments are likely to total 0.6–1 B tokens.

## Working hypotheses for the next genome, if nothing is promoted

- **The search space may be too narrow to clear the bar.** The genome controls only the prompt, the base and the effort of up to 3 calls to a fixed `a2ctl` harness. Detection needs about +1 logit per task. Changes inside this space, such as call sequencing or feedback wording, may all be smaller than that. A wider genome would let the researcher vary the agent's own instructions and tool use, not only the call sequence.
- **Target where the headroom is.** Half the pool never passes the visible tests. Approaches that change what the agent does inside a call, such as reproducing the failure first or running the visible tests before finishing, are more likely to move these tasks than approaches that only reorder calls.

To be revised as experiments 0005–0022 land.
