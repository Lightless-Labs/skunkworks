# Plan: Cycle 2 — Autoresearch on a Minimal Coding Harness

**Created:** 2026-10-15
**Status:** Active
**Goal:** the session `/goal` of 2026-10-15 ("A²D cycle 2"). This plan is its working form.
**Reviewed:** 2026-10-16, proposer-comparison protocol (Codex `gpt-6.1-sol` high + fresh Claude subagent; dispositions in the 2026-10-16 Addendum). 2026-10-15, design review before the first live run (Codex `gpt-6.1-sol` high, fresh Claude subagent; copies-only dirs). Dispositions in the Addendum.
**Addendum:** 2026-10-15 — **design review dispositions and definitions** (all accepted unless noted):
- **Evaluator integrity** (`nano/eval.py` v2.1):
  - experiments are immutable, with a manifest of code, controller and suite hashes, and the frozen snapshot is what gets promoted;
  - every scored run needs exactly one score per expected task and arm, or the run is invalid;
  - the A/A hash must match the current incumbent, and the A/A is re-run after every keep;
  - the screen half is rotated per experiment id;
  - structured fields: stage, n tasks, n families, calls by lane, tokens.
- **Keep rule:**
  - provisional if Δfrac > max(A/A threshold, 1.645 · cluster SE), and the all-pass Δ is not significantly negative;
  - **promotion only after a fresh confirmation replicate with Δfrac > threshold / 2**.
  - Keeps are exploratory; the reviewers' estimates are about 5% false keeps per test before confirmation and about 1% after.
- **D parity:**
  - a zero-call fixture test (native D vs external v0 under a deterministic mock: identical prompts and identical submission) is required before any scored run;
  - the A/A is a 3-arm live run (D, X:a, X:b) whose D-vs-X deltas must lie within ±5 pp all-pass, with byte-identical first prompts.
- **E002 controls:**
  - **E003**: the same 4-sample policy with a random-green selector. E002 − E003 attributes the gain to the judge.
  - Claim wording: **"Luna generation plus Sol selection"**. Sol is a stronger model from the same vendor, not a different family.
  - Reported per arm: calls and tokens by lane, branch frequencies and fallbacks.
  - A cost-matched control (D plus one Sol call) comes before any resource-efficiency claim.
- **Gate metric** (to the confirmation set):
  - the frozen incumbent's **all-pass** lift vs D on the dev-holdout is ≥ +10 pp as a point estimate, at r = 2, with a paired family-cluster CI lower bound > 0.
  - Dev-holdout looks: one after each confirmed keep (at most 4), plus a final frozen-candidate look. Proposers learn only pass or fail.
  - This replaces "every 10 keeps", which a 25k budget would almost never reach.
- **Proposer comparison** (A²D's architect vs uninformed): primary metric = the mean paired Δ per proposal against the same frozen incumbent, in interleaved blocked rounds; keep rate is secondary. The protocol is frozen and pre-registered before it runs, including an inconclusive outcome.
- **Authorship confound:** the train and dev-holdout tasks are Sol-authored, and Sol is also the live judge. This is disclosed. A Claude-lane judge replication is needed for a different-family claim (the CLI is unauthenticated from subprocesses).
- **Not adopted:** O'Brien–Fleming group-sequential bounds. The confirmation replicate plus exploratory labelling covers the same risk more cheaply.

**Lineage:** the lift-first roadmap (`docs/plans/2026-10-14-lift-first-roadmap.md`, Completed (falsified)), report `docs/reports/2026-10-15-lift-first-outcome.md`. That closed one plan, not the project.

**Addendum:** 2026-10-16 — **after the first keep: A/A method, budget envelope, and the proposer-comparison protocol (v2, after review)**
- **A/A (eval v2.3; LOG "Eval v2.3").** "The A/A is re-run after every keep" is replaced.
  - A promoted keep's two independent full runs (full stage and confirm) are paired by task and give its threshold at no cost.
  - Calibration of the method on v0: in-run 10 net tasks vs cross-run 7 net. The full-stage 1.645·SE guard binds at about 7 net.
- **Daily cap enforced in code:** every eval stage and look counts today's Luna calls and defers itself rather than exceed 3k.
- **Budget envelope** (about 21.8k Luna left on 2026-10-16):

  | Use | Luna | Notes |
  |---|---|---|
  | Search | ≈ 10.8k | until the phase-B trigger (11k left) |
  | Proposer comparison | ≈ 6.5k | reserved (phase B, protocol v2) |
  | Confirmation-2 | ≈ 3.2k | reserved; r = 2, one replicate per day |
  | Dev-holdout looks | ≈ 1.3k | final look |

  - With E002-class arms, a screen costs about 0.6k, the full stage about 1.25k and the confirm stage about 1.25k.
- **Proposer comparison, protocol v2** (frozen 2026-10-16, after the design review below; the tooling is built before the trigger and smoke-tested on dev only).
  - **Claim, narrowed:** "A²D's architect role, fed A²D's own mechanical observations, vs the same model and role without them". Information is the only designed difference.
    - Both arms use `gpt-6.1-sol` at high effort, one call per round, the same `ARCHITECT_SYSTEM` role (`crates/a2d-cli/src/lift_distill.rs`), the same output limit, and one retry on API failure.
    - Prompts are built from two pre-registered templates that differ only in the information sections. Token usage is recorded per call.
    - **Tool audit:** Codex runs with `--json`. Any event other than reasoning or agent messages voids the call, and it is re-asked once.
  - **Informed context (mechanical only; no operator prose and no LOG text):**
    - the incumbent;
    - v0 (= D);
    - the protocol, guard and rules;
    - `results.tsv` rows: everything before phase B, plus the informed arm's own earlier rounds, never the other arm's;
    - the incumbent's failure aggregates from a frozen stats script (C2-FA-style: branches, any-pass curve, failure kinds);
    - up to 8 failure dossiers (train-label rule, as in `a2d distill`): what the coder saw (requirements and public tests), the submitted file, and the label "fails at least one hidden test". Never hidden content or counts.
      - Dossiers come only from families **outside** that round's evaluation set, drawn from the incumbent's two full dev runs by a seeded rule (sha256 of the round id), not by the operator.
  - **Uninformed context:** the incumbent, v0, the protocol, guard and rules, and the titles of its own earlier proposals (to avoid repeats), without outcomes.
  - **Neither arm sees** `seeds.md` (a separate human-insight proposer), any holdout metadata, look results, or this protocol's existence.
  - **Phase B rounds:**
    - The incumbent is **frozen for all of phase B**, and promotions are deferred to its end.
    - R = 6 rounds. Each round is ONE `a2d lift` run with arms X:inc, X:inf and X:uninf on a seeded quarter of the dev families (about 35 tasks). The quarters are drawn with seed `A2D-C2-PC-<r>`.
    - Both proposals are generated, guarded, lane-checked, smoke-tested on 1 dev task and hash-registered (committed) before either is evaluated.
    - Feedback is released only between rounds.
    - Cost: about 0.43k Luna per round, about 2.6k in total.
  - **Mechanical validity rules** (symmetric across arms):
    - it passes the guard;
    - lanes may only name `codex/gpt-6.1-sol`, so the coder stays `gpt-6-luna` at low effort;
    - the 1-task smoke must complete;
    - post-run, every submitted file must equal the extract of a default-lane (Luna) reply in that tuple, so Sol writes no submitted code.
    - On failure, one re-ask with the error message.
    - A proposal that is still invalid scores Δ = 0 (no change) and is not a keep. It stays in every denominator.
    - A harness crash or timeout on a task counts as a fail for that arm, never as a void.
  - **Endpoints:**
    - **(1) The goal's keep comparison.** In each arm, the proposal with the best round Δall-pass vs the incumbent continues if it is ≥ +2 net tasks. Continuation runs the remaining families, then the fresh confirmation replicate, under eval v2.3 rules; at most 1 continuation per arm, about 2k each.
      - Reported: confirmed keeps per arm out of 6 proposals, and the **screen-pass rate** (round Δ ≥ +2 net). Descriptive only: at this budget the keep comparison cannot discriminate (a null proposal clears +2 net about 35% of the time), and that is disclosed as the main limitation.
    - **(2) Precision metric:** pooled over rounds, the paired all-pass difference X:inf − X:uninf on the round tasks. The incumbent arm cancels here; it is kept for (1).
      - It gets a (round, family)-cluster bootstrap 90% CI.
      - Sensitivity checks: a sign test over the 6 round-level differences, and Δfrac.
      - Upside secondary: the maximum Δ vs incumbent and the count of proposals with Δ > thr, per arm.
    - **Expected precision:** sampling alone over about 210 paired tasks gives a 90% half-width of about 4 pp, before between-proposal heterogeneity. **"Inconclusive" is the expected outcome unless the arms differ by more than about 5 pp**, and is reported as such.
  - **Descriptive analysis:** a blinded idea-category label per proposal (a fresh Claude subagent sees proposals without arm labels), so same-idea rounds are visible.
  - **Trigger:** after ≥ 3 keeps (the goal's condition), or when the remaining total budget falls to 11k Luna, whichever comes first.
    - The reserve (about 4.5k for confirmation-2 and the final look, plus about 6.5k for phase B) is why the budget trigger exists.
    - Running before 3 keeps tests an earlier system. It is a disclosed deviation, and the achieved keep count is reported prominently.
  - **Budget:** phase B ≤ 6.6k Luna, plus about 0.1k Sol for proposals and the rest for judge lanes. If the budget ends early, rounds already run are analysed as is; the minimum is R = 4 for the precision metric. An invalid round (run failure) is re-run once with the same proposals and a new seed.
- **Design review of the draft** (Codex `gpt-6.1-sol` high, no tools, everything inline; fresh Claude subagent, copies only). Dispositions:
  - **Accepted:**
    - the claim is narrowed;
    - mechanical informed context, with no LOG or operator prose;
    - one candidate per call, arm-specific prompts, no seeds;
    - frozen incumbent, deferred promotions;
    - feedback only between rounds, submissions frozen before evaluation;
    - equal budget and usage recording, with the tool audit;
    - mechanical validity rules, invalid scored as Δ = 0, crashes as fails;
    - dossiers family-disjoint from the round's tasks, with seeded selection;
    - "screen-pass rate" naming;
    - the keep comparison is descriptive and underpowered, disclosed;
    - more and smaller rounds (6 × quarter-sets instead of 5 × halves);
    - upside secondary, sign-test and Δfrac sensitivity, idea categories;
    - the early-trigger disclosure.
  - **Isolation finding (both reviewers): fixed before any further run.** Harnesses now run under an OS sandbox (LOG C2-ISO).
  - **Partly accepted:**
    - "Omit the incumbent arm" (the incumbent cancels in the precision metric). It is kept for the goal's keep endpoint; the shared arm makes it cheap.
    - "Use A²D's actual architect entrypoint". The metabolism's architect edits Rust system code, and the nanoharness is outside it. The role prompt and the dossier rule are reused, and the claim is narrowed accordingly.
  - **Accepted as reporting** (Sol #14, the A/A was calibrated on a selected keep): every keep decision is also reported under the v0 in-run threshold (0.0714) as a sensitivity check.

**Addendum:** 2026-10-18 — **confirmation-2 ended in IntegrityReview; a SUPPORTED claim now needs a confirmation-3** (LOG, 10-18 03:59 UTC)
- **What happened:** the replicate-1 web audit counted 2 web-search events in arm B (0 in D and X:frozen). By the registered C2-WEB amendment, the outcome is IntegrityReview: the set is spent with no claim.
  - Replicate 2 and `decision.json` still run as registered; their numbers are descriptive only.
  - The freeze holds until `decision.json`.
- **Gap:** confirmation sets are spent once, so the goal's done-path (a frozen harness SUPPORTED vs D and B on its confirmation set) is unreachable on confirmation-2. A new blind set is needed: **confirmation-3**.
  - The budget and deadline exits are unchanged.
- **Fixed now (requirements for any confirmation-3 call):**
  - The provider fix (`c2-web-provider`, 4b80fe67) is merged after `decision.json` **and** after the batch confirmation step, before phase-B round 1 (the 10-17 review rule: no provider change between an experiment's stages). It sets `-c web_search="disabled"`, and `codex_tool_calls` counts `web search:` lines, so `integrity_ok` fails on any search.
  - The fix is verified live: a dev smoke with 0 search events and no deprecation notice.
  - The count-only web audit stays as a second check.
  - **No confirmation-3 call runs on an instrument with a known open channel.** That is this addendum's lesson.
- **Authoring:** I am a proposer, so I don't author or see it. It follows the confirmation-2 pipeline (LOG "Holdouts — Confirmation-2"):
  - written blind by Codex `gpt-6.1-sol` (high) into `$A2D_HOLDOUT_DIR`;
  - checked with `a2d lift validate`;
  - an independent-solve audit (Sol medium, arm D, k = 3), with blind adjudication that repairs tasks and never drops them;
  - families disjoint from train, dev-holdout and confirmation-2;
  - only the suite hash and aggregates are recorded.
  - Authoring and audit use Sol, not Luna.
- **Budget:** confirmation-3 takes confirmation-2's slot (about 3k Luna at r = 2). The phase-B trigger at 11k left still covers confirmation-3, the final look and phase B (≈ 3.0k + 1.3k + 6.6k).
- **Decided later, with a multi-model design review** (Codex Sol plus a fresh Claude, copies only, before registration):
  - which frozen candidate it tests: E002, which passed look 1, or a later promoted keep, which needs its own look;
  - when it runs: now on E002, or after phase B on the best frozen harness;
  - size and r, with power planned against the realised discordance;
  - whether D, B and X stay the arms.

## Outcome sought

Harness changes that make our coder (`codex/gpt-6-luna`, low effort) produce working software more reliably than its plain repair loop D, at the same per-task invocation budget. Then: does A²D's own proposer find such changes better than an uninformed proposer?

## Why this shape
- In the current regime the public tests' information is spent. D reaches public-green on about 99% of tasks, and its remaining failures are spec misreadings no public test exposes. A harness can only add information or compute (cycle-1 report § Answer).
- Cycle 1 iterated slowly (about 200 calls and 45 minutes per decision) against noise larger than its effects (D's retest flip rate is 0.229).
- **Hidden-test pass fraction** is about 2.8× more sample-efficient than all-pass:
  - per-task A/A spread 0.12 vs 0.47;
  - it tracks all-pass at 0.42 per point, with correlation 0.75 over 51 arms;
  - about 250 paired tuples resolve +5 pp at 0.8 power, against about 700 for all-pass.

## Pieces

1. **Instrument.** The controller, oracle, ledger and holdouts are unchanged in their semantics. One extension: **external arms** `X:<name>`.
   - The controller runs a Python harness file per task over JSON lines.
   - Its model calls are metered, its public tests go through D's oracle, and it never sees hidden tests.
2. **Nanoharness** (`nano/harness.py`): v0 = D, byte for byte. Each experiment is one edit to this file. Everything A²D had before (metabolism, enzymes, lessons, evolver) comes back only as a kept experiment.
3. **Eval v2** (`nano/eval.py`, with results in `nano/results.tsv`):
   - Dev set: `tasks/train-all` (140 tasks, 35 families). Dev-holdout: 25 new blind families (`$A2D_HOLDOUT_DIR/dev-holdout`), metric-only.
   - Incumbent and candidate run in ONE run (`--arms X:inc,X:cand`), paired by task.
   - Metric: mean paired difference in hidden-test fraction. All-pass is also reported.
   - **Screen** on half the dev families. A candidate that clears half the threshold is run on the full dev set.
   - **Keep** if the full-set delta is above the 95th percentile of the A/A distribution (incumbent vs itself, same protocol).
   - One line per experiment in `results.tsv`: id, hypothesis, delta, SE, keep/discard, invocations.
4. **Information source** (experiment 1, C2-E1): a cross-family judge over Luna's candidates decides whether cross-model review is the first lever, or whether the loop moves to a tool-using multi-file regime.

## Loop
- One change per experiment; the hypothesis is logged before the run; discards are logged too.
- After every 10 keeps, re-check the incumbent vs D on the dev-holdout.
- **Proposers:** me, Codex Sol, and the human-insight seed list. After ≥ 3 keeps, A²D's own architect proposes, against an uninformed proposer at equal budget. The self-improvement result is their keep-rate comparison.
- **Claims:** confirmation sets are spent once, only on a frozen harness with a dev-holdout lift vs D of ≥ +10 pp, with D and B as comparators, after one Codex Sol plus fresh-Claude review. Confirmation-2 is valid only in the current regime.

## Budget
At most 3k Luna invocations a day, and at most 25k in total. Deadline 2026-11-30. Spend is tracked in `results.tsv` and the LOG.
