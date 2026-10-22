# Cycle 2: Autoresearch on a Minimal Coding Harness

**Status: DRAFT (updated 2026-10-22).** Sections marked **[pending]** are filled at close:
- the confirmation-3 result (replicates on 10-24 and 10-25, then manifest and score);
- confirmation-2's sealed numbers, descriptive only, released after confirmation-3's decision;
- look-1 numbers, released at close;
- the final claims wording (pre-written per outcome on 10-22; the decision selects one).

Look-1 numbers stay in `~/.a2d-holdout/runs/c2-look1/report.json` until close, per its read discipline.

**Goal:** the session `/goal` of 2026-10-15 (quoted in the plan).
**Plan:** `docs/plans/2026-10-15-cycle2-nanoharness.md`.
**Evidence:** `docs/experiments/LOG.md`, section "Cycle 2" (every entry registered before its run), and `nano/results.tsv` (one row per experiment stage, including discards).
**Outcome:** **[pending]**. Known so far (10-22):
- Confirmation-2 ended in **IntegrityReview** (2 web searches in arm B, replicate 1). The set is spent with no claim.
- Confirmation-3 (36 new families × 4) was **registered on 10-21 at `fb5110e7`** with candidate E002. Its replicates run on 10-24 and 10-25.
- **Phase B (proposer comparison) is inconclusive by construction.** Keeps were 0/4 in each arm. The incumbent failed exactly one task in each round's quarter, so no proposal could reach the +2 screen. The dev gym saturated; see Phase B.

## The question

1. Can harness changes make our coder produce working software more reliably than its plain repair loop D, with the same coder model and the same per-task invocation cap?
2. Does A²D's own architect, fed A²D's own observations, propose such changes better than the same model without them?

- **Regime:**
  - Single-file Rust tasks from numbered specifications, with weak public tests and 20+ hidden tests per task.
  - Coder: `codex/gpt-6-luna` at low effort, no tools, k = 5 model calls per task, on any lane.
  - Hidden tests never reach a harness or a proposer.
- **Data:**
  - **dev:** `tasks/train-all`, 140 tasks in 35 families;
  - **dev-holdout:** 100 tasks in 25 families, blind, outside the repo, aggregates only;
  - **confirmation-2:** 144 tasks in 36 families, blind, spent once.
  - All were authored by Codex `gpt-6.1-sol`, validated, and independently solved.

## Setup

- **Nanoharness:** `nano/harness.py` is one editable Python file, run by the controller as an external arm (JSON lines: model, lane, extract, public_test, done).
  - v0 reproduces D: first prompts byte-identical on 280/280 task-arm pairs, all-pass within 1.4 pp (C2-AA).
  - Metabolism, enzymes, lessons and the evolver re-enter only as kept experiments. None has so far.
- **Eval** (`nano/eval.py`): paired runs on the same tasks; screen on half the families, then the full set, then a fresh confirmation replicate; cluster (family) SEs.
  - **v2.1** used the goal's hidden-test fraction.
  - **v2.2** switched the primary metric to all-pass (working software), with fraction as a guard (disclosed deviation; see Deviations).
  - **v2.3** takes a promoted keep's A/A from its own two replicates: thr_all = 3.57 pp for E002, 7.14 pp for v0.
- **Budget:** ≤ 3k Luna calls per day (shared account), ≤ 25k in total, deadline 2026-11-30.
  - Envelope: search ≈ 10.8k, proposer comparison ≈ 6.5k, confirmation-2 ≈ 3.2k, final look ≈ 1.3k.

## Experiment 1: the information source (C2-E1, offline, train)

- **Setup:** on the frozen S0 pool (136 tuples), a judge reads only the spec and picks among Luna's public-green candidates. The equal-budget arm is 4 samples + 1 judge call.
- **Results:**
  - **Claude (Sonnet) judge: 37/39** decisive tuples, pick rate 0.831, against D's 0.743. The rule was ≥ 0.793: **met.**
  - Sol judge: 39/39 (0.846, the ceiling).
- **Consequence:** cross-model review became the first lever. The Claude CLI was unavailable to subprocesses, so the live judge is Sol: a stronger model from the same vendor, not a different family.
- **C2-E2** (offline, E002's 68 decisive tuples): Claude Opus 5.5 got 66/68, Sol 68/68. The selection effect is not vendor-specific.

## Search (dev set, exploratory)

| Exp. | Proposer | Change vs incumbent | Result |
|---|---|---|---|
| E002 / E002b | claude | v0 → 4 Luna samples + 1 Sol judge over distinct public-green files | **KEEP:** screen +11.8, full +10.7 (SE 2.9), fresh confirm +7.1 (SE 3.5) pp all-pass vs v0 = D |
| E003 / E003b | claude | same 4 samples, random green pick (attribution control) | discard (full): −1.4 pp. **The judge is the active ingredient.** |
| E004 | claude | repair to green, then sample + judge | screen +4.4 pp; superseded by E002's promotion |
| E005 | claude | samples 2–4 get different "Approach" angles | provisional keep (full): +5.0 pp (SE 2.8) vs E002; not confirmed (E007 promoted first; at most 2 confirmations) |
| E006 | claude | Sol spec notes before 3 samples | discard (full): screen +2.9 pp, full +2.9 pp (SE 2.4), below the 3.6 pp threshold |
| E007 | sol | 3 drafts → Sol pick + defect list → Luna repair | **KEEP, promoted 10-20:** full +7.1 pp (SE 3.2), fresh confirm **+17.1 pp (SE 4.0)**, 137/140 vs 113/140 (discordant 24 vs 0) vs E002, with fewer Luna calls (3.3–3.4 vs 4.0–4.1 per task) |
| E008 | sol | 4th sample becomes a synthesis | discard (screen): +0.0 pp (SE 2.9) |
| E009 | claude-c1 | task-agnostic failure-class checklist | discard (screen): −2.9 pp (SE 2.9); hidden fraction −1.8 pp |
| E010 | claude-c1 | two readings per Luna call (A = most likely, B = hedge on the least certain points) | provisional keep (full): +5.0 pp (SE 2.6) vs E002, same calls; the judge picked a B file in 5 of the first 11 tasks; not confirmed (E007 promoted first) |

- **Where E002 still fails (C2-FA1):** the judge never picked a failing file when a passing green existed (0 misses in 249). All-pass equals its own any-pass ceiling (0.857), so **the bottleneck moved from selection to generation:** shared misreadings defeat any selector.
  - The C2-FA2 taxonomy (Claude, train labels only) guided E005, E006, E009 and E010.
- **E005 decomposition:** of its +7 tasks, +5 came from generation (more tasks with a right reading in the pool), +2 from selection noise in the incumbent arm, and 0 from repair. The incumbent arm drew low (116/140, against 123 and 117 in its earlier replicates).
- **Cost:** E002-class arms are **not cost-matched**: about 4.0 Luna + 0.9 Sol calls and 6.4M tokens per 140 tasks, against about 1.3 Luna calls and 1.4M tokens for D. Every claim is "at an equal invocation cap".

## Dev-holdout look 1 (C2-LOOK1)

- **PASS, valid.** The rule: frozen E002 vs D, 100 tasks × 2 replicates, lift ≥ +10 pp with CI low > 0.
- Numbers: **[pending: released at close from `report.json`]**.

## Confirmation-2 (C2-CONFIRM2): frozen E002 vs D and B

- **Registration:** `78727f29` (registration file `runs/c2-confirm2/registration.json`). r = 2 replicates on separate UTC days. SUPPORTED requires the lift ≥ +10 pp with CI low > 0 against **both** D and B.
  - It was reviewed by Codex Sol and a fresh Claude before spending.
  - Power at a true +10 pp is about 0.47, with a recorded exception for P(RULED OUT | 0) = 0.73.
- **C2-WEB amendment (strictly tightening):** any web-search event in any arm makes the outcome IntegrityReview.
- **Result: IntegrityReview, so no claim.** The confirmation-2 set is spent.
  - Replicate 1 (10-18, 1,486 Luna and 132 Sol calls): its web audit counted **2 web-search events in arm B** (720 transcripts), against 0 in D (185) and 0 in X:frozen (713). There were no invocations without a transcript and no other codex version.
  - By the registered amendment, this fixes the outcome whatever `decision.json` says. Replicate 2 still runs, as registered.
  - Replicate 2 (10-19, 1,617 invocations, 0 void submissions) ran as registered. Its web audit counted 0 searches, but 11 invocations without a transcript (a failed-call recording gap, fixed for confirmation-3).
  - Scored once (10-19 06:11 UTC). `decision.json` and `pooled-summary.json` are **sealed unread** in the holdout dir (sha256 in `runs/c2-confirm2/sealed-sha256.txt`) until confirmation-3's decision. Then they are reported here, **descriptive only**: **[pending]**.
  - The cause is not examined: that would mean opening the set. The counts are the whole permitted view.

## Confirmation-3 (C2-CONFIRM3): a new blind set after confirmation-2's IntegrityReview

- **Pre-commitment** (plan 2026-10-18-confirmation-3 §0, fixed before confirmation-2's replicate 2 and before any batch confirmation):
  - 36 new families × 4 = 144 tasks, r = 2, k = 5;
  - arms D, B and X:frozen, under the same claim rule as confirmation-2;
  - tuple-level voids, imputed worst-case for SUPPORTED and best-case for RULED OUT, on both the point and the whole-family bootstrap CI;
  - IntegrityReview above 0.5% tool events per replicate.
- **Candidate rule (§0.1, no operator choice):** the batch winner iff it is promoted and passes one dev-holdout look by 2026-10-23 23:59 UTC; otherwise E002.
- **Suite, built 10-19:**
  - Sol picked 40 briefs, 36 OK after a namecheck.
  - Authoring ran under a seatbelt profile with a Mach-service allowlist and sandbox-membership reaping, and all 36 families were verified.
  - Similarity against 1,035 prior tasks: max 0.04, 0 conflicts.
  - Validation 144/144, after one repair.
  - An independent blind Sol solve (k = 3) solved 139/144 in round 1. One failure was adjudicated UNDERDETERMINED and repaired, requirements only; round 2 solved the remaining 5. **Certified: 144 tasks, 0 excluded.**
- **Instrument, hardened before registration** (three early review rounds, Codex Sol plus a fresh Claude, every blocker fixed; plan §0.13):
  - web search and every tool feature off;
  - an auth-only CODEX_HOME per call, under the holdout dir;
  - a `--json` allowlist detector that fails closed, with live positive controls per item type on each lane;
  - failed-call transcripts;
  - the codex binary fingerprinted around each replicate;
  - a canary preflight through the real provider;
  - registration bound to the commit that added it.
  - **Disclosed:**
    - three mid-authoring pipeline fixes;
    - a provider fix after the audit (cwd collision, §0.14), which the certification's ancestry check covers;
    - the audit ran from the provider branch before main's merge;
    - codex 0.162 injects about 10k chars of bundled skills and multi-agent text that no switch removes, the same in every arm.
- **Registered** on 10-21, about 07:30 UTC, at `fb5110e7285b37b8125ca6fb17c717fb871f3809`, after a final review (Codex Sol and a fresh Claude, 7 blockers fixed; plan §0.15/§0.16).
  - **Candidate: E002** (sha e772bd6e…). Look 2 (C2-LOOK2, the promoted batch keep E007) was **invalid**: one hidden-test name, coincidental, was echoed from a Luna draft into a judge prompt. Under §0.1 the candidate is fixed at the look's result.
  - Arms D, B and X:frozen; 144 tasks; k = 5; Luna low, with a Sol-medium judge lane.
- **Spend:** replicate 1 on 10-24 and replicate 2 on 10-25, from 00:05 UTC, with deferrals retried for up to 5 days. Then the manifests are committed, and the result is scored once.
- **Toolchain disclosure:** Homebrew moved the global codex to 0.163.0 at 10:32 UTC on 10-21. This session did not do it. The registered 0.162.0 runs from a pinned copy, built from the official release package and byte-identical to the registered hashes. The runner's own fingerprint check decides before each replicate.
- **Result: [pending]**

## Proposer comparison (phase B)

- **Design:** protocol v2, frozen 2026-10-16. A²D's architect role (`gpt-6.1-sol`, high) fed A²D's mechanical observations, against the same role without them.
  - 6 rounds, each a 3-arm run on a seeded quarter of the dev families.
  - Endpoints: confirmed keeps per arm, screen-pass rate, and the pooled paired precision difference. "Inconclusive" is the expected outcome at this budget.
- **Trigger:** when the remaining budget reached 11k, it started after the batch's confirmation step (pre-registered 10-17). The keep count reached by then is reported prominently.
  - Phase B started on 10-20 from incumbent E007, after two confirmed keeps (E002, then E007) and two provisional ones (E005, E010).
- **Stopped at R = 4.** This deviation from R = 6 was pre-committed on 10-21, before round 4 reported, as a budget reallocation. R = 4 is the protocol's minimum for the precision metric.
- **The finding: the search's ceiling became the gym's ceiling.**
  - After two keeps, the incumbent E007 solves about 95% of the dev gym: 130/140 and 137/140 on its two replicates. By level, l1 is 70/70 and l4 63/70, and 28 of the 35 families are fully solved.
  - **Measured headroom: one task per round.**
    - In every round the incumbent failed exactly one task of its quarter: 35/36 in rounds 1–3, 31/32 in round 4.
    - So the largest net gain any proposal could make was +1, against a +2-net screen. One task (2.8–3.1 pp) is also below the A/A keep threshold of 3.57 pp.
    - No proposal could have screen-passed in any round, whatever its merit.
    - Three proposals reached the +1 ceiling (round 1 informed, rounds 2 and 4 uninformed). Each was a single task.
  - The proposers were productive. Ideas included repair contracts and blueprints, two-reading drafts, deeper Sol review, contrastive review and completion audits.
  - The gym could not rank them.
  - **Consequence:** the next cycle starts with gym v3, a harder dev gym (plan 2026-10-21-gym-v3), where the proposer comparison can be re-run with headroom.
- **Result** (registered endpoints at R = 4, `runs/c2-pc/analysis.json`): 4 rounds, 140 paired task-runs, all 8 proposals valid.
  - **Keep rate (the registered self-improvement comparison):** 0/4 informed vs 0/4 uninformed.
    - Screen-pass was 0/4 in each arm, and no proposal was above the A/A threshold.
    - No difference is detectable, and on this gym none could have been (above).
  - **Paired precision** (informed − uninformed, each against the shared incumbent):
    - All-pass: −1.4 pp, 90% cluster-bootstrap CI [−3.6, +0.7]. By round: +2.8, −2.8, 0.0, −6.3 pp. Sign test 1+/2−/1 tie, p = 1.0.
    - Hidden fraction: +0.6 pp [−1.4, +2.8]. Sign test 2+/2−, p = 1.0.
  - **Blinded idea-category labels:**
    - The labels were assigned before round 4 reported. The join was pre-specified at e485c262b; output in `runs/c2-pc/labels-join.json`, descriptive only.
    - Each arm proposed 4 distinct categories, and 2 categories were shared: judge-effort and reviewer-feedback-enrichment.
    - In round 4 the arms crossed over:
      - The informed proposal is **byte-identical** to the uninformed arm's round-1 candidate (judge-effort, sha 48339373).
      - The uninformed proposal returns to the informed arm's round-1 category (reviewer-feedback-enrichment).
    - The identical bytes scored 0.0 pp on the round-1 quarter and −3.1 pp on the round-4 quarter: one task's difference between quarters.
  - **Reading:**
    - A²D's mechanical observations did not measurably steer the architect's exploration. By round 4, the informed arm proposed bytes the uninformed arm had already proposed.
    - This does not test whether observations help find keeps, because on this gym nothing could be kept. The comparison re-runs on gym v3.

## Instrument integrity in cycle 2

- **C2-ISO** (10-16): the static harness guard was porous (attribute routes to the filesystem). Harnesses now run under `sandbox-exec`, with no network and no file access under `$HOME` or the temp roots.
  - A mechanical scan of every harness that ever ran found nothing.
- **C2-ISO2** (10-17): the oracle's compile and test steps now also run sandboxed. The run step gets no processes, no Mach lookups and a cleared environment.
  - Parity: 420/420 identical against the old binary.
- **C2-WEB** (10-17): codex-cli 0.162.0 ignores `--disable web_search_request`, so web search was available to every call, and the controller's tool count (exec only) could not see it.
  - **0 searches in about 15.8k recorded transcripts** (every dev run since 10-14, and every holdout run, counts only), so no result recorded before confirmation-2 is affected.
  - Fixed for proposers. Dev runs stop on any search, and confirmation-2 is audited. The provider itself is fixed after the confirmation-2 freeze.
  - **Confirmation-2 is the exception:** 2 searches by arm B in replicate 1, which voids its claim (above).
  - **Lesson:** a known open channel on a spend-once set must be closed and re-registered before the spend, even at the cost of a re-review. An audit run after the spend can only tell you the set is gone.
- **C2-MCP** (10-18): codex also loads a curated Cloudflare plugin (an MCP server) and local skills, despite `--ignore-user-config`.
  - The server needs OAuth and never connected, so the coder's rare `list_mcp_resources` calls returned nothing.
  - Counts: 7 in about 5.7k dev transcripts; 3 of 1,240 in look 1, all in X; 0 in confirmation-2's replicate 1.
  - Look 1's gate still passes with those 3 tuples imputed as fails (a one-bit check).
  - About 12k chars of codex-injected skill and multi-agent context precede every prompt, the same in every arm.
  - No recorded result is affected. The fix is an empty `CODEX_HOME`, a feature allowlist and structured-event detection.

- **Confirmation-3 instrument work** (10-18 and 10-19; plan §0.12 and §0.13):
  - **Authoring sandbox:** a copied `/usr/bin/open` escaped the path-based seatbelt profile, so Mach lookups became an allowlist and localhost was denied. Jobs are reaped by sandbox membership, not by process group.
  - **Failed calls had no transcript.** All 34 failed dev calls were timeouts, and under the new tuple-void rule that would have charged X. The provider now kills the process group and records the partial transcript.
  - **Codex `--json` prints no version banner,** so the binary is pinned by a fingerprint taken before and after each replicate.
  - **Injected context that no switch removes:** codex's 4 bundled skills and the multi-agent text, about 10k chars. It is the same in every arm and disclosed.
  - **Detector controls, live:** a tool switched back on is counted on both lanes, and the shipped provider shows 0 events.
  - **Concurrent calls could share a cwd** (found 10-20 by a counts-only look at the audit ledgers). The cwd was named by pid and clock, which has microsecond resolution on macOS. Two calls in the same microsecond therefore shared a cwd and CODEX_HOME, and both failed without a transcript. These were audit round 1's 4 voids. In the spend, such voids would have been imputed against SUPPORTED. Fixed before registration with a counter in the name and an exclusive create (§0.14), and the controls were re-run.
  - **Two mid-authoring pipeline fixes:** a command-scan false positive on prose `at` lines, and a retry isolation check that saw codex's own `skills/`. Both are disclosed.

## Claims and scope

- **[pending: confirmation-3's `decision.json` selects one of the blocks below]**
- **Pre-written 2026-10-22, before any confirmation-3 data exists.**
  - The class comes from `nano/confirm3.py decide()` and is copied, not interpreted.
  - Numbers are reported vs D and vs B, as lift with its 90% whole-family CI, on complete tuples and under the registered imputation. Calls and tokens by lane go beside them.
  - Registered power (`c3_power.py`): P(SUPPORTED) = 0.14 / 0.33 / 0.47 / 0.96 at a true +7.1 / +8.9 / +10 / +15 pp, and P(RULED OUT | 0) = 0.91. At E002's dev confirm estimate (+7.1 pp), INCONCLUSIVE or RULED OUT is the likely outcome.
  - **SUPPORTED:** the claim is the registered wording, verbatim (`registration.json` `claim_wording`), with its qualifiers kept in the same paragraph:
    - the equal invocation cap (5 calls per task), not equal cost;
    - X used about 4.6× D's tokens on dev;
    - Sol is a stronger model from the same vendor, not a different family;
    - the tasks were Sol-authored, with a Sol judge lane (author-judge alignment), so the claim covers this authoring distribution only.
    - The harness is frozen as the cycle's result.
  - **RULED OUT:** "On a blind 144-task set, frozen E002 does not deliver a +10 pp all-pass lift over Luna's plain repair loop at an equal invocation cap."
    - It rules out the bar, not an effect. The point estimate and CI are reported, and a positive lift below +10 pp stays possible: dev estimated +7.1 pp (SE 3.5) at the fresh confirm.
    - E002 stays the frozen best harness of cycle 2's *confirmed* line, with no claim.
  - **INCONCLUSIVE (any variant, including the worst-case and best-case imputation ones):** "No claim: confirmation-3 neither supports nor rules out a +10 pp lift."
    - The numbers are reported descriptively, with the registered power beside them, so that a low-power null is not read as evidence of no effect.
    - The set is spent.
  - **IntegrityReview or Invalid:** "No claim; the set is spent." The report gives only the triggering counts (the fixed vocabulary). The cause is examined only on copies, and only after the decision.
  - **In every case:**
    - **Self-improvement:** there is no claim that A²D's own proposer beats an uninformed one. Phase B was inconclusive by construction (0/4 vs 0/4 keeps; one task of headroom per round).
    - **E007** (+17.1 pp vs E002 on the dev confirm replicate) is a dev result only: its one dev-holdout look was invalid, and it was never confirmed.
    - **Confirmation-2's** sealed numbers are reported descriptively under their own heading, never as a claim.
- Wording rules already fixed:
  - "at an equal invocation cap", never "equal cost";
  - Sol is the same vendor as Luna, so not a cross-family claim;
  - measured on hidden-test all-pass for this suite, not general software reliability.

## Deviations and amendments (process diagnostics)

- **v2.2: primary metric fraction → all-pass,** after E002's screen (fraction −, all-pass +11.8 pp).
  - Disclosed. The goal names fraction. The affected experiments were re-registered (E002b, E003b, E004b), and the fresh confirmation replicate is the actual test.
- **v2.3:** the A/A from a keep's own replicates; its noise was calibrated on v0 (±30%).
- **Look 1 re-registered** after C2-ISO changed the controller's frozen paths, before any look data.
- **Confirmation-2 re-registered** (b8a274aa → 78727f29) after its own binding check failed on a path bug, before any call.
- **The C2-WEB amendment** was registered before any confirmation call.
- **Confirmation-3** replaces the spent confirmation-2 (plan addendum 10-18). Its §0 rules were fixed before replicate 2 and before any batch confirmation.
  - Look 2 (C2-LOOK2, the conditional dev-holdout look of a promoted batch keep) was registered on 10-19 because no queue line ran it. Without it, a promotion would have defaulted to E002.
- **No live Claude lane:** the Claude CLI is blocked inside the session sandbox, so Claude contributed offline via subagents (C2-E1's primary judge, C2-E2, C2-FA2, proposer round c1, reviews). Their tool use was not machine-audited; this is disclosed.
- **Phase B started before 3 confirmed keeps** (the budget trigger fired first). It started on 10-20 with 2 confirmed keeps (E002, then E007) and 2 provisional ones (E005, E010). This disclosed deviation tests the system as it stood then.
- **Phase B stopped at R = 4 instead of 6.** This was pre-committed on 10-21, before round 4 reported, because the dev gym is saturated (see Phase B).
- **results.tsv was missing phase B's uninformed rows; restored on 10-22.**
  - `eval.log_result` de-duplicates on (id, stage, decision) and ignores the proposer. Both arms logged `PC-<r>` / `round` / `screen-fail`, so every uninformed row was silently dropped.
  - `c2_phaseb_backfill.py` rebuilt the rows from the registered `scores.json`. As a check, it first reproduced the four logged informed rows field for field.
  - Neither the arms' inputs nor the endpoints were affected. The fix to `eval.py` waits for confirmation-3's freeze to lift (`todos/results-log-dedup-key.md`).
