# E1 decision record: NO GO

**Date:** 2026-10-15
**Run:** `e1-v1`, 36 sampled tasks × 3 arms = 108 rows, completed 2026-10-15T06:36Z.
**Protocol:** `bench/external-value/protocol-v1.toml`, committed in `1033811e` before any sample call. The protocol file's sha256 is `7c28d47def22912b0304fd26daa4f4ded463f37a0d0610c267780679e7146080`.
**Plan:** `docs/plans/2026-10-14-value-test-first.md`, with its addenda.
**Analysis:** `bench/e1/decide.py`, committed in `77cef01d` before the first result row. Its output is `analysis.json` beside this file, and the sha256 of the result rows is `f5cdd785…d683d4aa`.
**Benchmark:** an issue-only variant of SWE-Bench Pro V2 (public split): the ansible subset with |F2P| ≥ 2, graded natively. These are not SWE-Bench Pro scores.

## Decision

**NO GO.** The pre-registered rule is GO iff a2-full beats stateless-retry by at least +15 pp and the exact one-sided McNemar test gives p ≤ 0.10.

| | raw | stateless-retry | a2-full |
|---|---|---|---|
| resolved (official rule) | 5/36 (13.9%) | 10/36 (27.8%) | 12/36 (33.3%) |
| hidden-F2P + P2P only (descriptive) | 6 | 12 | 12 |
| tokens in+out | 15.2 M | 34.2 M | 33.7 M |
| agent wall time | 3.0 h | 7.4 h | 5.9 h |

- **Primary (a2-full vs stateless-retry):** a2-full won 3 discordant pairs and stateless-retry won 1. The difference is **+5.6 pp** with **p = 0.3125**, below the effect threshold, so the outcome is NO GO under the protocol's `no_go` clause.
- **Infrastructure exclusions:** 0 of 36 tasks, so no reserve task was used.
- **Secondary:**
  - a2-full vs raw: +19.4 pp, 7 wins, 0 losses, p = 0.0078.
  - stateless-retry vs raw: +13.9 pp, 6 wins, 1 loss, p = 0.0625.

## What the result says

- **Retrying with visible-test feedback is what helps.** Both retry arms roughly double raw's resolve rate. That effect does not depend on anything A²-specific: the stateless arm is a plain loop that feeds back verifier output.
- **The lineage-bearing loop adds no measurable value beyond that.** Of its 3 wins over stateless-retry, only one came from a later attempt: `e1-9664d5ebf43e`, solved on attempt 2. The data do not show whether lineage was consulted. The other two look like single-attempt sampling variance:
  - `e1-40102f94748d` was solved on a2-full's first attempt, which is equivalent to a raw run.
  - `e1-7502f821bb9f` was also solved by raw in one shot.
- **Hidden tests show no edge.** On the hidden fail-to-pass tests plus pass-to-pass, the two retry arms tie at 12–12.
- **Cost is the same.** a2-full matched stateless-retry's token spend within 2%.

## Deviations during the run (harness-only, recorded as plan addenda)

1. **Leak-guard fixes.** The driver's leak guard aborted shards twice:
   - once on the private directory's *path* appearing in pytest tracebacks;
   - once on hidden node ids that are strict prefixes of visible ids.

   Neither was a leak. A scan of the verifier output tails recorded in all 188 attempt rows, across every arm, found no hidden id or gold-patch text. The visible verifier only ever selects visible node ids. The guard was narrowed in `33fecc34` and `bbc4626c`.

   The protocol's `leak_check` line still lists "private path"; the 2026-10-14 addendum supersedes it. Agent-visible text was never changed. Shards resumed from completed rows, and an interrupted (task, arm) re-ran from scratch, with no grade recorded for the aborted try.

   **Re-run selection effect.** A stop could fire only after a failed stateless attempt 1, so five stateless-retry arms got a fresh draw after a discarded failure:
   - first stop: `e1-1fd6bb17b447`, `e1-a04429d13db2`, `e1-d01a171a6d7e`;
   - second stop: `e1-57d4935630ef`, `e1-59b1486201bd`.

   The bias favours the control arm, and so runs against GO. All five final outcomes are concordant with a2-full: four unresolved in both arms, and `e1-59b1486201bd` resolved in both. No discordant pair was affected.
2. **Lineage reset.** `7cbd946c` makes the a2-full arm start from an empty lineage DB on retry or resume. It landed before the first resume. Every crash traceback points at the stateless-retry branch (`run.py:199` at `0cb744b4`, `run.py:217` at `7cbd946c`), so no a2-full row inherited stale lineage.
3. **Harness versions by row range:**
   - rows 1–22 ran on `0cb744b4`;
   - rows 23–96 ran on `7cbd946c`;
   - rows 97–108 ran on `bbc4626c`.

   None of these changes altered prompts, feedback, budgets, grading or analysis.

## Re-decision (plan table, row "E1 NO GO")

**Stop A² as a research runtime.** Of the table's two options, this record takes **Option E: archive with a post-mortem.**

Option D, packaging A² as an eval harness, is set aside for two reasons:
- Its kill criterion is consumption by a sibling project, which this work does not evaluate.
- E1 itself shows that the valuable part is a generic verifier-feedback retry loop, not A²'s runtime.

The reusable pieces stay in the repository as-is:
- the E1 harness (`bench/e1/`), including native grading, the Seatbelt read boundary, the loopback egress proxy and paired arms;
- this record.

Promoting them under Option D remains open to the owner.

Consequences:
- Remediation Phases 1–8 and the roadmap stay frozen permanently.
- Live `--apply` stays locked.
- E2 is not run, because it is gated on E1 GO.

## Addendum: 2026-10-15 — re-decision withdrawn (owner direction)

The result above stands as recorded. The re-decision section does not: archiving A² (Option E) is withdrawn.

A² exists to be an agent that iterates on agent harnesses and development workflows, in the spirit of autoresearch. In that framing, E1 is one outer-loop experiment: a candidate harness, evaluated against a baseline with a frozen grader, was discarded. That outcome does not end the project.

What E1 settles:
- Lineage-as-memory in the current fixed loop is not worth its keep over stateless retry with visible-test feedback.
- The E1 apparatus works as a fitness function, with known noise. Single-attempt variance alone produced 2 of a2-full's 3 wins.

The next direction is to make that loop the system: an agent proposes harness variants and E1-style evaluation keeps or discards them.
