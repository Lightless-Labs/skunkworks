# Plan: Confirmation-3 — a new blind confirmation set for cycle 2

**Created:** 2026-10-18
**Status:** Draft, for design review (Codex `gpt-6.1-sol` high, no tools, web disabled, everything inline, plus a fresh Claude subagent with copies only). Nothing is authored or registered before the review dispositions are recorded here.
**Parent:** `docs/plans/2026-10-15-cycle2-nanoharness.md`, addendum 2026-10-18.
**Why:** confirmation-2 ended in IntegrityReview (LOG, 10-18 03:59 UTC: 2 web searches in arm B, replicate 1). Confirmation sets are spent once, so the goal's done-path (a frozen harness SUPPORTED vs both D and B on its confirmation set) needs a new blind set in the same regime.

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
