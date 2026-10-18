# Plan: Confirmation-3 — a new blind confirmation set for cycle 2

**Created:** 2026-10-18
**Status:** Active. **§0 is a pre-commitment, fixed 2026-10-18 before confirmation-2's replicate 2 (≥ 00:05 UTC 10-19) and before `confirm_best` (≥ 10-20).** §§1–5 are the reviewed draft, read with the dispositions in §7. Nothing is authored before the work items in §0.6 that come ahead of authoring are done.
**Reviewed:** 2026-10-18, draft design review by Codex `gpt-6.1-sol` high (no tools, web disabled, everything inline; 0 search events) and a fresh Claude subagent (copies only). Dispositions in §7.
**Parent:** `docs/plans/2026-10-15-cycle2-nanoharness.md`, addendum 2026-10-18.
**Why:** confirmation-2 ended in IntegrityReview (LOG, 10-18 03:59 UTC: 2 web searches in arm B, replicate 1). Confirmation sets are spent once, so the goal's done-path (a frozen harness SUPPORTED vs both D and B on its confirmation set) needs a new blind set in the same regime.

## 0. Pre-commitment (fixed 2026-10-18; binds the registration)
These choices are written before any data that could inform them. That means before confirmation-2's replicate 2, before any E006–E010 decision, before `confirm_best` and before any confirmation-3 content exists. The registration copies them; it does not reopen them.

### 0.1 Candidate rule (dated; no operator choice)
- **Candidate = the batch winner** iff both hold:
  - `nano/confirm_best.py` promotes a keep (its registered sequencing rule, 10-17);
  - that keep passes **exactly one** registered dev-holdout look under the existing gate (all-pass lift vs D ≥ +10 pp point estimate at r = 2, paired family-cluster CI low > 0), completed by **T = 2026-10-23 23:59 UTC**.
- **Otherwise the candidate is E002** (`runs/c2-confirm2/frozen.py`, which passed look 1). There is no second look and no other candidate.
- If a promotion happens and its look is not complete by T, for any reason, the candidate is E002.
- The candidate is frozen by file hash at T, or at the look's result if that comes first.
- **Its dev-holdout look doubles as the cycle's "final frozen-candidate look"** when the final frozen harness is that candidate. That look is not run twice.

### 0.2 Spend date
- **Replicate 1** runs on the first UTC day ≥ max(T + 1 day, certification day, freeze-lift day + 1). Certification is §0.6 step 5, and freeze-lift is when `decision.json` exists.
- **Replicate 2** runs on the next UTC day the daily cap allows. A cap or latency deferral is not an attempt.
- Neither preliminary results, nor phase-B progress, nor certification speed can move the candidate. They can only delay the spend.

### 0.3 Size and replicates
- **36 new families × 4 levels = 144 tasks, r = 2, k = 5, arms D, B and X:frozen,** in the same regime and with the same claim rule as confirmation-2: SUPPORTED iff the point estimate is ≥ +10 pp and CI95 low > 0 vs both D and B; RULED OUT iff X vs D is RuledOut.
- **48 families do not fit** the budget: they would leave phase B about 0.8k, below its R = 4 floor (§0.10). So they are not used.
- The power statement is computed blind (§0.6 step 1) and recorded in the registration. It does not change n.
- **Step 1 result, 2026-10-18** (`docs/experiments/analysis/c3_power.py 300 2`, dev summaries only):
  - Pooled dev discordance is 0.173 over 14 paired comparisons; confirmation-2 assumed 0.26.
  - The calibrated task-level deviation SE is 1.5 (confirmation-2 used 3.2).
  - At 36 × 4, r = 2, with B − D = −3.6 pp:

    | true X − D | P(SUPPORTED) | P(INCONCLUSIVE) | P(RULED OUT) |
    |---|---|---|---|
    | 0 (null) | 0.00 | 0.09 | **0.91** |
    | +7.1 pp | 0.14 | 0.67 | 0.19 |
    | +8.9 pp | 0.33 | 0.61 | 0.07 |
    | +10 pp | 0.47 | 0.49 | 0.04 |
    | +15 pp | **0.96** | 0.04 | 0.00 |

  - Both PROTOCOL targets are met: P(RULED OUT | 0) ≥ 0.8 (unlike confirmation-2), and P(SUPPORTED | +15 pp) ≥ 0.8.
  - The ex-ante statement (§0.7) stands: at E002's estimates INCONCLUSIVE is the most likely class.

### 0.4 Integrity policy (replaces run-level "any event → IntegrityReview")
- **Detection is an allowlist** (`codex exec --json` structured events): any item other than an agent message or reasoning is a tool event. That includes `error`, unknown types and parse failures.
  - Each detector gets a positive control per tool kind, on every lane, before registration.
  - The codex version is pinned to the probed binary, and a mismatch voids the tuple.
- **Tuple-level voids with worst-case imputation.**
  - A tuple with any tool event or missing transcript is void.
  - A void X tuple counts as a fail; a void D or B tuple counts as a pass. This can only lower X's lift.
- **Run-level IntegrityReview** (no claim) if any of these holds:
  - any hidden-test name hit in any prompt;
  - tool events in > 0.5% of the replicate's invocations;
  - void tuples above the registered void gate.
- **Disclosure:** this is weaker than confirmation-2's any-event amendment, which would have voided the claim on one stray event. It stays conservative for the claim through imputation. The registration review may tighten it, and no post-registration amendment is allowed unless a live probe backs it.

### 0.5 Budget priority (Luna)
- **The 10-17 projection:** about 11.1k left after the E005–E010 batch, with confirmation-2 already inside the spend. After at most 2.5k of batch confirmations, about 8.6k is left for phase B (6.6k) and the final look (1.3k). The slack was about 0.7k.
- **Confirmation-3 is ring-fenced first,** before phase B starts: about 3.0k for r = 2 (replicate 1 of confirmation-2 used 1,486), plus a 1.5k relaunch reserve.
- **The candidate's look** (1.3k, only if a keep is promoted) merges with the final look (§0.1).
- **Phase B then gets what remains.** Its registered fallback applies ("if the budget ends early, rounds already run are analysed as is; the minimum is R = 4"). Continuations (endpoint 1) run only if the budget remains after the confirmation-3 reserve.
  - This is a disclosed amendment to phase B's budget envelope, fixed before any phase-B data. Its precision endpoint (2) keeps R ≥ 4.
- **Confirmation-2's replicate 2 still runs** as registered (about 1.5k, already inside the 10-17 projection).
- An explicit ledger table is kept in the HANDOFF and updated daily.

### 0.6 Order of work (each step gates the next one marked ⇒)
1. **Blind power script:** it reads only dev and dev-holdout ledgers and prints the SE and P(SUPPORTED / INCONCLUSIVE / RULED OUT) at fixed hypothetical lifts (0, +7.1, +8.9, +10, +15). It never prints a lift estimate.
2. **Authoring pipeline in the repo** (driver, assembly, gateway, disjointness check), tested on visible fixtures and frozen by hash. Old scripts died in a scratchpad.
3. **Capability inventory and the `--json` allowlist detector:** designed now, built on a branch (crates stay frozen), and merged once **both** confirmation-2's `decision.json` exists **and** the batch confirmation step has ended. The second condition is the 10-17 review rule: the provider must not change between an experiment's stages. The merge comes with `c2-web-provider` (4b80fe67), before phase-B round 1. Then run the positive-control probes per tool per lane, and re-freeze at a new commit. Authoring (step 4) calls `codex exec` directly with its own flags, so it does not wait for this merge.
4. ⇒ **Authoring** starts only after confirmation-2's replicate 2 generation and web audit have finished (no overlap), with these settings:
   - Option B: Sol picks the families from a disjointness inventory;
   - opaque ids (`c3-fNN-lM`) from the first byte;
   - `CODEX_HOME` inside `$A2D_HOLDOUT_DIR`, and the prompt passed as a pointer file (never in argv);
   - a canary read-deny check before the first job.
5. ⇒ **Validation, the independent-solve audit and hashing,** all under the re-frozen commit:
   - the audit runs on the fixed provider;
   - DETERMINED failures stay as they are, and only UNDERDETERMINED tests are repaired;
   - a mechanical content-similarity check runs against every prior set and prints only the maximum and the conflict count.
6. ⇒ **The registration,** with its own Codex Sol and fresh-Claude review, then the spend per §0.2.

### 0.7 Ex-ante statement (for the owner)
- **E002's honest estimates are +7.1 pp** (the fresh dev confirm stage) and about +9 pp (dev). The claim needs a +10 pp point estimate.
- At those values, P(SUPPORTED) is roughly 0.2–0.4, and **INCONCLUSIVE is the most likely outcome at any feasible size**.
- More tasks only make a null result informative (RULED OUT power). They do not raise SUPPORTED near +10 pp.
- The only lever on SUPPORTED is the candidate's true effect, which is why §0.1 waits a few days for a possibly stronger batch winner under a fixed rule.
- **The +10 pp bar is the owner's and is not lowered.**

### 0.8 Confirmation-2's scored data stays sealed
- `confirm-manifest` and `confirm-score` are run by hand after replicate 2's audit.
  - Their stdout goes to `$A2D_HOLDOUT_DIR/runs/`.
  - `decision.json` is moved, unread, to `$A2D_HOLDOUT_DIR/runs/c2-confirm2-decision/`. The repo records only its sha256 and a "written" line.
- Nobody reads it until confirmation-3's `decision.json` exists. The C2-WEB rule has already fixed confirmation-2's outcome, so its contents cannot change it.

### 0.10 Closing the gaps found by the review of §0 (Codex Sol, 2026-10-18, before 00:05 UTC 10-19)
- **Amendments:**
  - Before the first confirmation-3 call, the registration review may only tighten §0.4: lower thresholds, add detectors, add isolation checks. Every other part of §0 is copied unchanged.
  - **After the first confirmation-3 call, nothing in the registration changes.** No amendment may depend on any confirmation-3 output.
- **Phase-B arithmetic (worst case):** 8.6k − 4.5k (confirmation-3 plus its reserve) − 1.3k (look) = **2.8k for phase B**.
  - Rounds cost about 0.43k each, so R = 6 (about 2.6k) fits without continuations. R = 4 (about 1.7k) is the floor.
  - With 48 families (+1.0k per replicate), phase B would fall to about 0.8k, below R = 4. That is why §0.3 uses 36.
  - **Shortage outcome:** if fewer than 4 rounds' worth of Luna remain when phase B starts, it runs the rounds that fit and reports them descriptively, with no precision-endpoint claim, as a disclosed shortfall.
  - The unused relaunch reserve returns to phase B after confirmation-3's decision.
- **Relaunch:** confirmation-2's registered failure policy, copied.
  - Each replicate is attempted once, and `attempt-r<n>.json` is written before any model call. A cap or latency deferral before launch is not an attempt.
  - A generation that fails before `generation_complete` may be relaunched **once** with the same seed in a new output dir, logged.
  - A second failure ends confirmation-3 as not run: no claim, and the set is spent.
  - Tuple-level failures are voids under §0.4.
- **Validity gate and tuples (fixed now):**
  - A tuple is one (task, arm, replicate), meaning all of that arm's invocations on that task in that replicate.
  - The void gate is confirmation-2's: excluded (void) tuples ≤ 5% of all tuples, over the pooled run (both replicates, all arms).
  - The tool-event rate of §0.4 (> 0.5%) is computed per replicate, over that replicate's invocations in all arms. Either replicate crossing it means IntegrityReview.
  - Controller guards are unchanged: ≥ 10 families, and ≥ 1 discordant pair per comparison.
- **Spend day:**
  - Replicate 1 starts at 00:05 UTC on the first UTC day that meets all of these:
    - it is ≥ 2026-10-24;
    - it is strictly after the UTC day on which the registration, with its review dispositions, was committed;
    - `decision.json` of confirmation-2 exists, and the re-frozen instrument is committed.
  - A cap or latency no-go moves the start to the next day's 00:05 UTC.
  - Replicate 2 follows the same rule from the day after replicate 1's generation.
- **Rules frozen before the step that uses them** (each committed in this plan or in the pipeline's own spec, before the step starts):
  - **before step 4 (authoring):**
    - per-family caps on authoring, repair and audit calls;
    - DETERMINED / UNDERDETERMINED classification, at most 2 repair rounds per task and one re-audit;
    - the exhausted-cap outcome: the task is excluded with disclosure before the hash, and the family is excluded if fewer than 4 tasks remain;
    - the similarity metric, corpus and threshold, with conflict handling (conflicting family re-authored once, else excluded before the hash);
  - **before step 5:** the accepted `--json` event schema, including which thread/turn bookkeeping items are benign. It comes from step 3's positive and negative controls.
- **Executable isolation and drain checks:**
  - **Launch isolation, scripted and run before every authoring, repair and audit job, and before each replicate.** Any failure means the job does not start.
    - No `.codex/` directory, `AGENTS.md` or skills directory exists in the cwd or any ancestor.
    - `CODEX_HOME` is under `$A2D_HOLDOUT_DIR`.
    - The canary read-deny check passes.
    - The codex version matches the pinned one.
  - **Process drain, before each replicate:**
    - The authoring driver has stopped launching.
    - Every child pid it recorded in its pid file has exited, checked with `kill -0` (never `pgrep -f`).
    - The relocated `CODEX_HOME` holds the authoring jobs' session files. This positive check shows the relocation took effect without reading `~/.codex`, which the A² session also writes to.

### 0.9 Owner request
- The A² session shares the account. Luna calls take 60–175 s today.
- **Please pause the A² session during confirmation generation windows** (about 2–3 h each, announced in the HANDOFF), and if possible during confirmation-2's replicate 2 (from 00:05 UTC 10-19).
- This is not a pause of this session. Each spend also checks latency first (a pre-registered go/no-go), and a deferral is not an attempt.

## 1. The set
- **Regime:** identical to confirmation-2: single-file Rust tasks from numbered specifications, with weak public tests and ≥ 20 hidden tests. Authoring spec: the `SPEC.md` used for the dev-holdout and the confirmation-2 extension (sha256 prefix `447f84b907a6`).
- **Size:** 36 families × 4 levels = 144 tasks, the same as confirmation-2, so its power analysis carries over.
- **Families:** new, and disjoint from every existing set and its near-siblings: train-all (35), c1-validation, confirmation-1 (37), dev-holdout (25), confirmation-2 (36), plus the excluded `spreadsheet-eval`.
- **Who picks the families (open question for the review):**
  - **Option A (precedent):** I write the one-paragraph `FAMILY.md` briefs, as in cycles 1 and 2. Family names are the only content I see.
  - **Option B (stricter):** a Codex Sol job picks the 36 names and writes the briefs from the disjointness list. I see only counts, task ids and hashes, and the names after the final hash.
  - I am also a harness proposer. The harness changes so far are generic (sampling, selection, review), but knowing the topics is a channel A does not close. **I lean to B.**

## 2. Authoring pipeline
The pipeline is the one in `docs/solutions/best-practices/blind-holdout-authoring-pipeline-2026-10-14.md`. The driver and assembly scripts lived in an old scratchpad and are rebuilt under `$A2D_HOLDOUT_DIR/authoring/confirmation-3/` (not the repo, not `/private/tmp`).
- **Authoring:** `codex exec`, `gpt-6.1-sol` high, one job per family, in `$A2D_HOLDOUT_DIR/authoring/confirmation-3/<family>/`.
  - Flags: `--sandbox workspace-write`, `-c approval_policy="never"`, `--ignore-user-config`, stdin closed, and **`-c web_search="disabled"`** (C2-WEB).
  - The driver records exit codes only. `codex.out`, `codex.err` and `REPORT.md` are never read.
- **Assembly:** contract files only (`task.json`, `public_tests.rs`, `hidden_tests.rs`, `reference.rs`, `mutants/*.rs`). The assembly prints task ids and counts.
- **Validation:** `a2d lift validate` prints verdict lines only. Rejections get a blind FIX pass (verdict lines plus a fix brief).
- **Independent-solve audit:**
  - `a2d lift run --arms D --model codex/gpt-6.1-sol --k 3 --unregistered`, run on the fixed provider (§3).
  - Failures go to blind adjudication, which repairs tasks and never drops them.
  - Exit criterion: all tasks pass the audit, or are repaired and re-audited.
- **Hash:** `a2d lift hash` after the last repair, logged. Only the hash, counts and (option-dependent) family names enter the repo.
- **Account contention:** authoring jobs slowed Luna calls about 3× before (E05). Today Luna calls already take 60–175 s with the A² session sharing the account.
  - Authoring runs at parallelism ≤ 3, preferably while the queue waits on a date or the daily cap.
  - The driver is paused (`kill -STOP`) whenever a registered or confirmation run is in flight.
- **Never print process command lines while authoring or holdout jobs run** (PROTOCOL; the 10-15 `spreadsheet-eval` exposure).

## 3. Instrument requirements before any confirmation-3 call
- The confirmation-2 freeze is lifted only by `decision.json` (replicate 2 runs at or after 00:05 UTC 10-19). Then:
  1. Merge `c2-web-provider` (4b80fe67): `-c web_search="disabled"`, and `codex_tool_calls` counts `web search:` lines, so `integrity_ok` fails on any search. Tests must pass: providers 20/20, a2d-lift 66/66.
  2. **Live probe:** a Luna call with the provider's exact flags is asked to search. Expected: no search event, no deprecation notice. A probe counts as a pass only on that evidence.
  3. Enumerate the other default-on Codex 0.162 tools (fetch, MCP, image, and so on) and probe each. Every channel found is closed and counted the same way.
- The count-only web audit (`confirm-web-audit`) stays as a second check, generalised to every tool kind. The rule is unchanged: any event in any arm → IntegrityReview.
- The instrument is re-frozen at a new commit, and the registration binds it.

## 4. What it tests (registered later, after this review; open questions)
- **Candidate:**
  - **E002**, the current incumbent. It is frozen as `runs/c2-confirm2/frozen.py`, and look 1 PASSED the gate (dev-holdout all-pass lift ≥ +10 pp, CI low > 0).
  - **Or the batch winner,** if `confirm_best` (10-20 to 10-22) promotes a keep. That keep must pass its own dev-holdout look (≈ 1.3k Luna) before it can be the candidate.
- **Proposed selection rule** (fixed before any confirmation-3 data): the candidate is the incumbent at the moment confirmation-3 is certified, provided it has passed a dev-holdout look under the gate. If a promoted incumbent fails its look, the candidate is E002.
  - Phase B's frozen incumbent is the same harness, and phase B runs on dev only, so it does not touch confirmation-3.
- **Arms:** D, B and X:frozen, k = 5, as in confirmation-2.
- **Replicates:** r = 2 on separate UTC days. The power exception is recomputed from discordance on the dev and dev-holdout runs, and **not** from the confirmation-2 scored data: that data stays sealed until the cycle report, and it is descriptive.
- **Claim rule:** identical to confirmation-2's, since it was already reviewed. SUPPORTED iff both comparisons (X vs D and X vs B) are Supported: lift ≥ +10 pp, CI95 low > 0, whole-family bootstrap. RULED OUT iff X vs D is RuledOut. Invalid and IntegrityReview dominate.
- **Timing:**
  - Authoring starts after this review. Expect about 1–2 days of wall time at low parallelism, plus validation, the audit and the instrument fix.
  - Spend happens after the batch confirmation step (≥ 10-22) and before phase B ends, so phase-B budget can't crowd it out. **Alternative for the review:** spend on E002 as soon as the set is certified, without waiting for the batch.

## 5. Budget
- **Luna:** about 3.0k for r = 2 (replicate 1 of confirmation-2 used 1,486). This takes confirmation-2's reserved slot, and the phase-B trigger at 11k left still covers confirmation-3, the final look and phase B.
- **Sol:** about 36 authoring jobs, repairs, and the audit (about 450 Sol calls at k = 3). These count against Sol, not the Luna cap.
- **Deadline:** 2026-11-30. This plan finishes well before it if authoring starts by 10-20.

## 6. Questions for the reviewers
1. Option A or B for family selection? Is B's blinding worth losing my disjointness check (the Sol job checks it instead)?
2. Is the candidate selection rule (§4) free of forking paths? Should it be fixed to E002 now instead?
3. Wait for the batch (≥ 10-22), or spend on E002 as soon as the set is certified?
4. Are 144 tasks and r = 2 still right, given that confirmation-2's power at a true +10 pp was about 0.47?
5. Is the §3 channel enumeration sufficient? What else in codex-cli 0.162 is on by default?
6. Anything about running the authoring jobs while the A² session shares the account?

## 7. Review dispositions (2026-10-18)
Reviewers: Codex `gpt-6.1-sol` high (no tools, web disabled, everything inline; 0 search events) and a fresh Claude subagent (copies only). Both recommended revising before any authoring or registration.

- **Accepted, now in §0:**
  - Option B (both reviewers), with opaque ids everywhere and names revealed only after confirmation-3's decision.
  - A dated candidate rule fixed now, with no timing discretion. Wait for the batch under a fixed cutoff T.
  - A blind power script that prints SE and power only, never a lift estimate.
  - Allowlist detection on structured events, positive controls per tool and lane, a pinned codex version, and an isolated environment (`CODEX_HOME`, no project `.codex`, no `AGENTS.md`).
  - Tuple-level voids with worst-case imputation, plus a rate threshold. Confirmation-3 cannot again be spent by one stray event.
  - `CODEX_HOME` inside the holdout dir for authoring, repair and audit jobs, a canary read-deny check, and a content-similarity disjointness check.
  - DETERMINED audit failures stay as they are, so the audit does not condition the set on Sol solving it.
  - Validation, audit and hashing re-run under the re-frozen commit.
  - The pipeline committed to the repo and tested on visible fixtures.
  - Prompts passed as pointer files (argv is visible in `ps`).
  - Drain the authoring driver before any registered window (`kill -STOP` does not stop children), and no overlap with confirmation-2's replicate 2.
  - The budget ring-fence and ledger.
  - A registration review step, and no post-registration amendments without a live probe.
  - Confirmation-2's data sealed in the holdout dir until confirmation-3's decision (resolves Claude D3 and Sol's release boundary).
  - An ex-ante statement of the likely outcome.
- **Accepted, for the registration step:**
  - Branch-specific call caps read from the frozen candidate. Sol noted that a no-green Luna repair makes the branch 4 Luna + 1 Luna, so claim wording follows the implementation.
  - Per-family Sol caps and a predefined failure outcome for authoring, repair and audit.
  - Balanced interleaving of arms and task order. Check whether the controller interleaves; if it runs arms in blocks, fix that before registration.
  - New seeds and an exposure-exclusion policy.
  - Disclosure of possible model drift, the author–judge alignment (Sol authors, audits and judges), and every spent set. The claim stays narrow, to this authoring distribution.
  - A latency go/no-go before each spend.
- **Partly accepted:**
  - **"Exclusive evaluation windows across both sessions" (Sol).** This session cannot control the A² session, so it becomes an owner request (§0.9) plus a latency go/no-go.
  - **"Add families rather than replicates" (Claude).** Right in principle, but 48 families do not fit the budget (§0.5). The size stays 36 × 4, and the power statement records the RULED OUT power honestly.
  - **"Cancel confirmation-2's replicate 2 to fund confirmation-3" (Claude D7, offered as an option).** Not taken: it is registered and in the projection, and cancelling a registered step the first time the budget looks tight is the decision shape this cycle's lesson warns against.
- **Rejected:** none.
- **Noted:**
  - Option A would let the brief author, who knows E002's mechanism, favour weak-public-test families, which biases the set toward X (Claude). That is why B is chosen.
  - SUPPORTED near +10 pp is decided by the point threshold, not by n (Claude, with Sol's point 4 concurring). See §0.7.
