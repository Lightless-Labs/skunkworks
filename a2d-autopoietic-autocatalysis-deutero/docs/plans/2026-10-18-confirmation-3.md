# Plan: Confirmation-3 — a new blind confirmation set for cycle 2

**Created:** 2026-10-18
**Status:** Active. **§0 is a pre-commitment, fixed 2026-10-18 before confirmation-2's replicate 2 (≥ 00:05 UTC 10-19) and before `confirm_best` (≥ 10-20).** §§1–5 are the reviewed draft, read with the dispositions in §7. Nothing is authored before the work items in §0.6 that come ahead of authoring are done.
**Reviewed:** 2026-10-18, draft design review by Codex `gpt-6.1-sol` high (no tools, web disabled, everything inline; 0 search events) and a fresh Claude subagent (copies only). Dispositions in §7.
**Parent:** `docs/plans/2026-10-15-cycle2-nanoharness.md`, addendum 2026-10-18.
**Addendum:** 2026-10-18 ~12:00 UTC — §0.12. Sol's second pipeline re-check found 5 startup blockers. A live probe then showed that a *copied* `open` launched an app outside the sandbox. All 5 are closed in code, and one gap in the frozen rules is filled: an authoring-exhausted family is replaced. Still before any selection call.
**Addendum:** 2026-10-19 ~07:00 UTC — §0.13: the scoring shape (imputation binds the point threshold, as in confirmation-2; a best-case guard on RULED OUT). A failed call has no transcript and so voids its tuple; this is fixed on the provider branch before registration. Written during selection, before any authored content.
**Addendum:** 2026-10-20 ~01:10 UTC — §0.14: concurrent provider calls could share a cwd and CODEX_HOME (the 4 failed calls behind audit round 1's 4 voids). Fixed on the provider branch (36bea1e1) with a regression test; controls re-run complete at that head (73bc99ee). Before registration; no rule changes.
**Addendum:** 2026-10-21 — §0.15: the registration review (Codex Sol high and a fresh Claude, copies only) answered Yes with 7 blockers between them; all are fixed before the registration commit, with dispositions below. Candidate E002 (look 2 invalid).
**Addendum:** 2026-10-21 — §0.16: `code_mode_host` is disabled (it carries codex's built-in MCP tools, which a Luna coder called in phase-B round 1); the detector exempts its one fixed pre-turn notice; the bound runner recounts every transcript and requires the notice. Reverses §0.13's "code_mode_host stays on". Before registration; tightens the instrument.
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

### 0.11 Rules frozen before step 4 (2026-10-18, after the pipeline code review by Codex Sol and a fresh Claude)
Implemented in `tools/holdout/` and committed before any selection call. The pipeline's commit is recorded at launch in `records/pipeline.json`.

- **Selection:**
  - Write-once. Exactly 40 opaque briefs come from one Sol job, and an independent Sol job (fresh workdir and `CODEX_HOME`) marks each OK or CONFLICT against the inventory of every prior family.
  - `FAMILIES.txt` takes the first 36 OK ids; the remaining OK ids are spares, in id order.
  - With fewer than 36 OK: one fresh re-selection (`--reset`). If still short, the set uses the OK families when there are ≥ 30, disclosed. Below 30, stop and report.
- **Authoring:**
  - At most **2 attempts per family**, each one Sol (high) job, reserved before launch.
  - A family is done only when verified from its files: exit 0, zero non-allowlisted `--json` events, 4 complete tasks with opaque ids and exactly `m1..m4`, and `REPORT.md`.
  - Any non-allowlisted event fails the attempt (fail closed).
- **Similarity** (word 5-gram shingles of `requirements`):
  - A task conflicts at Jaccard ≥ 0.15 or containment ≥ 0.40 against any prior task (the repo's `tasks/` plus everything in the holdout dir except confirmation-3, authoring history included) or against another confirmation-3 family.
  - A family's first conflict means a re-author, which uses its 2nd attempt. A conflict after that, or with no attempt left, means replacement by the next spare; with no spare left, the family is dropped (disclosed).
  - The similarity check re-runs after every assembly, including after the last repair round.
- **Validation and repair:**
  - At most **2 FIX rounds per family**, each bound to the validation run that produced its verdicts.
  - Tasks still rejected after round 2 are excluded, and their family is replaced by the next spare (fresh authoring with its own 2 attempts) or dropped.
  - The suite contains only families with all 4 tasks.
- **Independent-solve audit and adjudication** (step 5, on the re-frozen fixed provider):
  - Audit: Codex Sol (medium), arm D, k = 3, once per task, the same solver as for confirmation-2.
  - **Adjudication** runs in one sandboxed Sol job per family with failing tasks. It classifies each failing hidden test:
    - **DETERMINED:** the requirements text implies the expected behaviour. The task is **kept unchanged**; a solver failure on a well-specified test is legitimate difficulty.
    - **UNDERDETERMINED:** the requirements leave it open. The adjudicator repairs the requirements or the test, followed by one re-audit of that task. If it is still failing and still UNDERDETERMINED, the task is excluded and the family replaced or dropped.
  - So the audit never conditions the set on Sol solving it, only on the tasks being specified (Claude D8, Sol #19).
- **Disclosed limits** (not closed by the code):
  - Network stays open to jobs, because codex needs the API and per-process network rules are not available. Copying public tasks is caught by similarity.
  - The OAuth token in `auth.json` is readable by the job; it is never writable.
  - Global file-metadata reads are allowed.
  - The controller's API-surface check compiles model-written Rust without the oracle's sandbox. It compiles only, never runs, and the crates are frozen until `decision.json`.
  - No dedicated uid (no admin rights). Instead the profile denies writes by default, keeps signals and process info within the sandbox, denies broker binaries (`launchctl`, `open`, `osascript`, `ssh`, …) and blocks unix sockets in user dirs. Every route was probed live with real codex on 10-18 and denied. *(Corrected by §0.12: denying the binaries' paths did not hold against copies.)*
- **SPEC.md stays byte-identical** (sha256 prefix `447f84b907a6`) to the spec that authored the dev-holdout and the confirmation-2 extension. That keeps confirmation-3 on the same task distribution as the data the harnesses were searched on. Sol's #22, to remove its retrospective difficulty cues, is therefore not adopted, and the registration discloses it.

### 0.12 Addendum: startup blockers from Sol's second re-check (2026-10-18, before any selection call)
Sol re-checked the round-2 pipeline (d50bee58): "No — resolve these startup blockers first." What was done for each:
- **Broker confinement (was open).**
  - A live probe under the real profile showed that a copy of `/usr/bin/open` in the workdir **launched TextEdit outside the sandbox** through LaunchServices. Path denies do not stop a copy, so the escape was real; `open -a Terminal x.command` would have run code unsandboxed.
  - Now Mach lookups are an allowlist (`c3lib.MACH_ALLOW`):
    - logd, notification center, opendirectoryd (libinfo and membership), dirhelper, trustd (and its agent), configd, and SecurityServer, which codex needs to load TLS roots (bisected live: without it, codex's API connection fails while `curl` works).
    - The list was harvested with `(allow mach-lookup (with report))` from a sandboxed compile, link and test run and a live codex call.
  - TCP to `localhost` (v4 and v6) is denied.
  - Probed live and now repeated before every job:
    - copied `open` (by name and by path), `launchctl`, `pbpaste` and `mdfind` fail;
    - `curl` to a localhost listener fails, and the listener sees no connection.
  - Checked once: `osascript` to Finder hangs and never answers, and a weakened profile fails the probes (negative control).
  - **Disclosed:** a copied `security` can list the *system* keychain's item metadata, which is world-readable outside the sandbox anyway. The login keychain's file is unreadable.
- **Job quiescence (was open).**
  - `setsid` descendants survive `killpg`. Jobs are now identified by sandbox membership: each profile alone may read its own tag file, and the host checks this with `sandbox_check` on every pid.
  - After each job or probe, the driver stops every member to a fixpoint, kills them, and verifies that none remain before touching any artifact. It raises if any survive.
  - `DriverLock` first reaps every unretired tag (driver-death recovery), and `drained()` requires no live member.
  - Verified live: a setsid'd, double-forked, env-scrubbed, chdir'd daemon is found only under its own tag and reaped in 0.35 s.
  - Host reads are descriptor-relative (`O_NOFOLLOW` per component; `os.fwalk`).
- **Selection provenance:** `select.json` carries the generation; `--reset` retires it with the failed briefs and job records.
- **STOP enforcement:** one launch lock serialises `STOP` creation and every job `Popen`, select and namecheck included. A launch refused at the gate releases its reservation.
- **Inventory completeness:** traversal errors raise; excluded subtrees are pruned before descent.
- **Similarity state (was partly open):**
  - a re-authored family is decided only by a check of a suite that holds the re-authored version (`assembled.json`); a stale suite leaves it pending;
  - a failed re-author is terminal.
- **Sol re-check 3** (6ea429f6): FIXED for broker confinement, selection provenance, STOP, inventory and both PARTLY items. Quiescence was PARTLY: a launcher between `Popen` and sandbox entry is unsandboxed, so a recovery after driver death could miss it.
  - Fixed with a launch handshake. The launcher waits on a driver pipe and execs `sandbox-exec` only after its pid and start time are fsync'd; if the driver dies first, it gets EOF and exits.
  - Recovery kills every recorded live launcher group (start-time verified, so a reused id is never signalled) before reaping tagged members.
- **Sol re-check 4** (85bf32e1) found the same race in **probes**, which were released without registration. Fixed: probes now launch exactly like jobs (own session, registered before "go", under the launch lock).
  - Also, an authoring-shaped smoke job (a Sol job writing and testing a small Rust file through `check.sh` under the real profile) passed: exit 0, 0 non-allowlisted events, `test result: ok`.
  - The command scan flagged its `"$HOME/.rustup/…/rustc" --version`. That false positive would have spent an attempt, so the toolchain's `$HOME`/`~` spellings are now stripped like its absolute path (test added).
- **Sol re-check 5** (134393a2): probes FIXED, no remaining blocker. **Verdict: Yes.** Selection and authoring may start once replicate 2's audit is committed and the pipeline is committed clean.
- **Step-5 tooling** (`c3_audit.py`, `c3_adjudicate.py`, `authoring/ADJUDICATE.md`, frozen with the pipeline and reviewed with the registration) implements §0.11's audit and adjudication. It makes these choices, which the registration discloses:
  - a void audit tuple is no information. The task is re-audited once; a second void keeps it, disclosed;
  - a task whose first informative audit is the re-audit (void at round 1) is classified only, with no repair left;
  - a repair may change only `requirements` and/or `hidden_tests.rs`, and must then pass validation (the reference passes, the mutants fail) and similarity again;
  - a missing or malformed repair, or 2 failed adjudication jobs for a family and mode, excludes the task.
- **Gap filled in §0.11** (it named no outcome for an authoring-exhausted family): a family whose 2 authoring attempts are used up without verified output is **replaced by the next spare, or dropped** at assembly. This is the same rule as the other exhaustion paths.

### 0.13 Addendum: scoring shape and failed-call transcripts (2026-10-19 ~07:00 UTC, during selection, before any authored content)
- **Scoring shape** (`nano/confirm3.py`, bound by sha256 in the registration). It mirrors confirmation-2's reviewed `confirm-score` and needs no crate change.
  - The controller's pooled claim is computed on complete `(task, replicate)` tuples: whole-family bootstrap, ci_low > 0 and lift ≥ +10 pp vs both D and B, ≥ 10 families, discordance guard.
  - §0.4's imputation applies to the **point** threshold. **SUPPORTED** also needs the worst-case imputed all-pass lift ≥ +10 pp vs both D and B (a void X tuple counts as a fail, a void D or B tuple as a pass), over **every planned** `(task, replicate)`. A missing score counts as void.
  - **RULED OUT** (X vs D RuledOut on complete tuples) also needs the *best-case* imputed lift X − D < +10 pp (void X as a pass, void D as a fail). Otherwise it is INCONCLUSIVE.
  - Imputation lowers X, which makes a null easier to reach. This guard keeps the null claim as conservative as the positive one. It only tightens §0.4.
  - A tuple, per §0.10, is `(task, arm, replicate)`. It is void if:
    - its submission has a `void_reason`;
    - it has no score;
    - its `tool_calls` > 0;
    - any of its invocations lacks a transcript;
    - any transcript is not the pinned codex version.
  - Run-level rules, any of which dominates (IntegrityReview or Invalid; no claim):
    - any hidden-test name hit;
    - per replicate, Σ `tool_calls` / Σ invocations > 0.5%. This counts events rather than invocations with an event, so it can only overcount;
    - void tuples > 5% of the 2 × 3 × 144 pooled `(task, arm, replicate)` tuples;
    - the controller's own Invalid or IntegrityReview.
- **Gap found: failed calls have no transcript.** `a2d-lift`'s meter writes `<inv>.transcript.txt` only for a successful call. A provider error writes the prompt and an `ERROR:` response, and nothing else.
  - Confirmation-2's replicate 2 had 11 such calls (X 8, B 2, D 1) during the host stall. Under §0.4 each one voids its tuple, so transient provider errors would cost X through imputation and could cross the 5% gate. Such a call is still unaudited: a timed-out call may have run a tool before failing.
  - **Instrument fix (step 3, on the provider branch, merged ≥ 10-22 with `c2-web-provider`):** the Codex provider returns its captured stderr with the error, and the meter writes the transcript for failed calls too. The scoring counts them like any other transcript.
  - "Missing transcript" then means a recording failure. The §0.4 rule is unchanged, and the fix only adds audit coverage.
- **Two probes, 2026-10-19:**
  - **Version pin.** `codex exec --json` (0.162) prints no version banner, so with the provider branch no transcript can carry the version.
    - The pin is a **bracket** instead. The generation runner records the codex binary's realpath and sha256 before launch (in the attempt record) and again after `generation_complete`.
    - A mismatch voids every tuple of that replicate, which makes it Invalid under the 5% gate.
    - Legacy text transcripts keep the banner check.
  - **Failed calls are timeouts.** All 34 failed invocations in the dev ledgers (0.47% of 7,177) are provider timeouts. So the transcript fix has to cover the timeout path:
    - after the kill, a bounded join of the pipe readers collects the partial `--json` stream and stderr;
    - the meter writes it for the failed call and counts its tool events *without* the missing-`turn.completed` penalty. That penalty would otherwise turn every timeout into a tool event, because a killed call never completes its turn.
    - A timeout whose partial stream holds only allowed items (reasoning, agent messages) is a failed call. One holding any other item is a tool event, and its tuple is void.
- **Mid-authoring scan fix (2026-10-19 ~08:00 UTC; disclosed in the registration).** Two of the first 7 authoring attempts (c3-f01, c3-f06) were rejected by the command scan, each with 2 flagged commands.
  - A category-only diagnostic printed the matched pattern class, never command text. Every flag was the job scheduler `at` as the first word of a prose line inside a multi-line command or a heredoc.
  - Fix:
    - heredoc bodies fed to `cat`/`tee` are skipped as file content (bodies fed to a shell are still scanned);
    - `at`/`batch` leave the tool list, because `atrun` is not loaded on macOS.
  - Both records then scan 0. The rule's intent is unchanged: broker, network or outside-path commands still count, and the seatbelt profile and probes remain the boundary.
  - The two spent first attempts stay spent; both families keep their second.
  - Authoring was stopped by STOP, which lets in-flight jobs finish, and restarted on the fixed commit. Jobs before 08:00 ran under the earlier pipeline commit.
- **Retry isolation fix (2026-10-19 ~08:40 UTC).** After the restart, both retry families (c3-f01, c3-f06) were refused by the isolation check before any attempt was reserved, so nothing was spent.
  - Cause: codex 0.162 writes its bundled `skills/` into `CODEX_HOME` on every run. The check runs before `run_codex` recreates the job's home, so it saw the previous run's `skills/`. A fixed-vocabulary diagnostic confirmed it: only `skills` was present, and in every workdir that has run.
  - Every re-author and every `c3_fix` repair, which uses the same family workdir, would have been refused the same way.
  - Fix: `c3_author.prepare` and `c3_fix` reset the job's codex home before the isolation check. The fake codex now writes `skills/` like the real one, and the suite passes 21/21.
- **Mutants-directory check (2026-10-19 ~09:25 UTC).** Two first attempts (c3-f10, c3-f18) exited 0 with 0 events and 0 flagged commands, yet failed verification.
  - A fixed-vocabulary diagnostic (check names, counts and entry kinds only) showed that every level held m1–m4 plus one extra non-source file, such as an author's notes. The exact-listing check rejected that, although SPEC.md never says "nothing else" and assembly copies only m1–m4.
  - Fix (`c3lib.mutants_ok`, used by the verifier and by assembly): m1.rs–m4.rs must be plain files; any other `.rs` file, any directory or any symlink still fails; other files are ignored. The authoring instructions are unchanged.
  - Both outputs now verify. Their recorded first attempts stay failed, and their second attempts rerun.
- **Injected-context check (step 3, 2026-10-19, `codex debug prompt-input`, local render, no model call).**
  - With the provider's isolation (fresh `CODEX_HOME` holding only auth, empty cwd, features disabled, web search off), the injected developer context falls from 18.4k to 12.3k chars. The plugins and the user's skills are gone.
  - **Residue that cannot be disabled in 0.162** (5 variants tried: more features off, `skip_host_skill_discovery`, `multi_agent_v2`/`collaboration_modes` off, `skills.enabled=false`):
    - ~7.3k chars of `<skills_instructions>` listing codex's 4 bundled skills (imagegen, openai-docs, skill-creator, skill-installer), which codex writes into every fresh home;
    - ~2.8k chars of multi-agent role and mode text.
  - It is the same in every arm and carries no task information. With shell tools disabled, the model cannot open the skill files. **Disclosed in the registration.**
  - **Tightening:** 10 more default-on features are disabled on the provider branch (d7f04741):
    - sleep tool, worktrees, workspace dependencies, unified-exec tty, shell snapshot, realtime, MCP elicitation, write-stdin approval, local automation, daemon auto-start.
    - A live PONG with them all off emits only thread/turn events and one agent_message.
    - `code_mode_host` stays on, because disabling it emits an `error` item on every call, which the allowlist would count.
  - At merge, `c3lib.PROVIDER_COMMIT` becomes the branch head (f06dc9fb at 09:05 UTC 10-19).
- **Audit from the provider branch, before main's merge (2026-10-19 ~14:30 UTC; a step-order deviation, disclosed).** §0.6 step 5 says the audit runs "under the re-frozen commit".
  - Main cannot merge the provider before L37 (10-22) and look 2 or T (10-23 23:59), and look 2's controller is bound to `98672c36`. Waiting would push the audit, and so the registration and the spend, to about 10-26.
  - Instead, `main` (52f778) was merged into `c2-web-provider` (merge commit fd36c2b2, which differs from main only in the provider's `crates/` files). `a2d` was built in the branch worktree, and the controls (`controls.json`) and the audit run from there. The tooling is main's, the provider is the reviewed one, and the holdout and records dirs are shared.
  - Main later merges that same branch commit, so the audited instrument's trees equal the registered ones.
  - The generator binds the audit by provider ancestry (`PROVIDER_COMMIT` bc6922f7 in the audit head) and the codex fingerprint, not by main's HEAD.
  - The queue and look 2 keep main's binary, so they are unaffected.
- **Registration-step items from §7, status 2026-10-19:**
  - **Interleaving: checked, no fix needed.** The controller runs arms interleaved, in completion order under parallel jobs.
    - Confirmation-2's manifests (432 submissions each): every quarter holds 32–40 of each arm, and the longest same-arm run is 6–7.
    - Dev runs (E005): the first half holds exactly half of each arm.
  - **Latency go/no-go: defined in the runner** (`nano/confirm3.py`). Before each replicate, 3 PONG calls of the registered model run from empty dirs. Go if the median is ≤ 45 s, where a failed probe counts as infinite. Every probe is logged to `runs/c2-confirm3/latency.jsonl`. A no-go defers to the next 00:05 UTC and is not an attempt.
  - **Runner** (`nano/confirm3.py generate | manifest | score`):
    - spend-day gates; a drain check through the pipeline's own `--drained`; the codex fingerprint (sha256 of every file beside the resolved binary) checked against the registration and re-taken after generation;
    - an attempt record before the first call; one relaunch with the same seed;
    - scoring by §0.13.
  - Still open for the registration: new seeds, the exposure-exclusion policy, and branch-specific call caps from the frozen candidate.
- **Early reviews of the runner, 2026-10-19 (copies only, web disabled):**
  - Codex Sol (high, 0 searches): **No**, with 10 blockers.
  - A fresh Claude: **Yes** once 4 blockers are fixed.

  Dispositions:
  - **Registration binding (Claude B1).** Fixed. The only valid `--registered-at` is the single commit that added `registration.json`. Every attempt record must carry it, and the registered provider commit must be in its history.
  - **Controller verdicts dominate (Claude B2, Sol B3).** Fixed: the controller's Invalid and IntegrityReview, the claim and the comparison classes.
    - Its `complete` flag is **not** required: it is false on any void tuple, which would contradict §0.4's tuple voids up to 5%. Aborted generations are Invalid.
  - **CI under imputation (both, the open question).** Adopted, which tightens §0.4:
    - SUPPORTED also needs the worst-case imputed whole-family bootstrap ci_low > 0;
    - RULED OUT needs the best-case imputed ci_high < +10 pp.
    - The bootstrap is a stdlib port of the controller's and matched its lift and CI exactly on 4 dev comparisons. Decisions use unrounded values.
  - **Launch isolation (both).** Fixed for the parts that apply to a no-tool replicate (temp-root ancestry, auth-only home per call, native-binary fingerprint). The rest is disclosed.
  - **Spend time and deferrals (Sol B5).** Fixed. A replicate starts at or after 00:05 UTC. A cap or latency no-go is recorded and holds for the UTC day. Latency probes count toward the cap (Sol B9).
  - **Mutual exclusion (both).** Fixed: an exclusive runner lock, an atomic attempt reservation with the pid recorded, and a relaunch only after a recorded terminal failure.
  - **Generator discretion (Claude B4, Sol B7).** Fixed. The candidate is derived by §0.1: the look-2 snapshot iff a valid PASS completed by T, else E002 by sha. The generator also requires:
    - the provider merge (recorded);
    - certification on exactly this suite;
    - 4 tasks per family and ≥ 30 families;
    - non-empty lanes.
    It refuses with the list of what is missing.
  - **Detector strictness (Sol B1).** Fixed on the provider branch (88462f81). The marker selects the JSON parser, and every unparsable line counts, a killed call's cut-off line included. This supersedes the cut-off-line exception of the first fix (f765c67d; LOG 10-19): only the missing `turn.completed` is excused.
  - **Controls coverage (Sol B8).** Fixed. Live controls run on the registered lanes at their efforts (Luna low, Sol medium) for shell, web search and file change, each showing its own item type. MCP and image are covered by the allowlist only.
  - **Selection count (Sol B10).** A LOG error, now corrected: all 40 briefs were written and namecheck validity requires all 40 verdicts, giving 38 OK, 36 active and 2 spares. No rule changed.
  - **Non-blocking:**
    - transcripts must be `--json`, and tuples with invocation-count mismatches are void;
    - planned tuples come from the registration;
    - manifests are written only when both replicates are done;
    - no holdout paths appear in messages;
    - disclosures added, plus the exposure policy and branch caps as a required field;
    - the review packet now includes the pipeline's code;
    - `which codex` must resolve to the native binary; do not upgrade codex before the spend.
  - The full registration review re-runs on the written `registration.json`.
- **Sol re-check (round 2, 10:13–10:40 UTC): No, 9 blockers.** Every one is fixed (advisor-sequenced; one combined re-review follows):
  - **B1, candidate eligibility.** The generator checks the look-2 provenance: exactly one promoted keep; the look ran at its registration commit on that keep's evaluated snapshot; gate, manifest and snapshot are committed; the look completed by T. Without a result the candidate is unresolved before T.
  - **B2, unfinished subset.** The suite must equal `active.json`'s final active families, each verified. Recorded replacement and drop events are disclosed.
  - **B3, audit binding.** Each audit round records the content hash of the task it audited. Certification needs the final informative round's hash to equal the current task. The provider commit is bumped at merge.
  - **B4, relaunch overlap.** The generation child inherits the runner lock (flock per open-file description, verified live), so the lock proves no generation is alive.
  - **B5 and B6, isolation and probe accounting.** The preflight is a tiny `a2d lift run` of canary tasks through the real provider: 3 calls on the coder lane for latency, 1 on every registered lane. The secret must appear nowhere, with 0 tool events.
    - Every call's CODEX_HOME sits under the holdout dir (`A2D_CODEX_HOME_ROOT`, branch 6112c307).
    - The probe runs are ledgers under `holdout/runs/c2-*`, so the cap counts them. A probe group is recorded before launch, and one without a go holds the UTC day.
    - Live: one Luna probe took 15.9 s, with no leak, 0 tool events and no homes left behind.
  - **B7, controls.** Fixed (branch 70be19ac). The inventory of exec item types comes from the binary. Live positive controls run on every lane in `A2D_CONTROL_LANES` for shell, web search, file change, MCP (a stdio echo server), sub-agent and error, each showing its item type.
    - On 10-19, on Luna low and Sol medium: all present, plus the negative control.
    - `todo_list` could not be triggered in exec mode even with every feature on; the allowlist covers it.
    - `c3_controls.py` records the run (`runs/c2-confirm3/controls.json`). The generator requires that record, complete, for the coder lane and every registered lane, at the registered provider commit and codex fingerprint.
  - **B8, registration text:** states the imputed CI guards.
  - **B9, repairs.** A hidden-test repair may change or remove only the tests judged UNDERDETERMINED; the preamble and every other test stay byte-identical, and nothing is added. A whole-task (`*`) verdict allows only `task.json`. *(Superseded in round 3: repairs are requirements-only.)*
- **Round 3 (11:50–12:20 UTC).** Sol: **No**, 7 blockers. Fresh Claude: **Yes** once 5 blockers are fixed. All fixed (provider branch bc6922f7, now the pinned `PROVIDER_COMMIT`):
  - **Repairs are requirements-only** (Sol B1, Claude B1). The segment check could still drop or `#[ignore]` a DETERMINED test, so hidden tests are never edited. An UNDERDETERMINED test is repaired by stating its behaviour in the requirements. This tightens §0.12's step-5 choice before any adjudication ran.
  - **The audit is bound to the instrument** (Sol B2):
    - each run records its git head, binary sha and codex fingerprint with a bracket;
    - certification requires the provider commit in that head, the registered codex and a held bracket;
    - production refuses a binary override.
  - **Audit isolation and integrity** (Sol B3, Claude B2): a canary preflight on the solver lane, holdout-local homes and the temp-root ancestry check; tool events, missing transcripts and count mismatches void a tuple.
  - **Audit interruption** (Sol B4): each round is reserved durably before launch; an interrupted reservation becomes a void round, never a fresh call; the child inherits the driver lock.
  - **Canary preflight** (Sol B5, B6): every lane needs at least one successful, transcript-audited call, else a day-long no-go; probe children inherit the runner lock.
  - **Drain** (Sol B7): §0.10's positive check is added. Every family that used an attempt has session files in its relocated `CODEX_HOME` (real data: all present).
  - **Controller classes must agree** (Claude B3): SUPPORTED needs the controller's Supported for D and B; RULED OUT needs its RuledOut for D.
  - **Source build** (Claude B4): `a2d` is built with `cargo --locked` from the bound tree before any replicate call and before scoring.
  - **Missing bracket** (Claude B5): it voids its replicate.
  - **Claude's non-blocking points, all taken:**
    - the aborted-generation wording matches the runner;
    - the provider is pinned to the reviewed head, and the holdout dirs must agree;
    - the look-2 gate must be committed exactly once (look 1's ledger has the summary record the on-time check reads);
    - controls count the intended item in the event stream only;
    - skipped eligible days are disclosed;
    - a failed first attempt's tool events count.


- *Answered by the early reviews (2026-10-19): yes; imputed CI guards adopted, see the dispositions above.* Original question: should the CI also be computed under imputation? That would need a `pool --impute` flag in the controller, landing with the ≥ 10-22 merge. Until then the CI is on complete tuples and imputation binds the point threshold, as in confirmation-2.

### 0.14 Addendum: concurrent calls shared a cwd (2026-10-20 ~01:10 UTC, before registration)
- **Found by** a counts-only look at the step-5 audit ledgers (labels and booleans printed, no content). Audit round 1's 4 failed calls were 2 adjacent pairs. Each pair was the first call of two different tasks, failing in under a second with no transcript: one "failed to link codex auth.json", one "failed to start: No such file". These 4 calls are the 4 round-1 voids.
- **Cause.** `isolated_provider_cwd` named the call's cwd `a2d-provider-<cmd>-<pid>-<nanos>` and created it with `create_dir_all`. On macOS the clock has microsecond resolution, so two calls starting in the same microsecond shared the cwd and the CODEX_HOME derived from its name. The second call's auth link failed (EEXIST), its cleanup removed the shared directory, and the first call's spawn failed (ENOENT).
- **Why it matters here.** In a replicate, each such pair voids 2 tuples, and voids are imputed against SUPPORTED (§0.13). At the audit's rate (2 collisions in 148 calls at `--jobs 4`), a `--jobs 8` replicate of about 2,000 calls could void tens of tuples. That is a loss of lift, and possibly a crossing of the 5% Invalid gate, from an instrument fault.
- **Fix** (c2-web-provider 36bea1e1): a per-process sequence in the directory name, and `create_dir`, so a collision fails closed instead of sharing. The regression test (8 threads × 200 cwds, all distinct) failed before the fix and passes after. Provider and lift tests pass.
- **Controls** re-run at 36bea1e1 (record committed as 73bc99ee): complete on Luna low and Sol medium, negative control 0 events.
- **What does not change:**
  - `PROVIDER_COMMIT` stays bc6922f7, an ancestor of the fix.
  - The suite stays certified: `audit_instrument_ok` checks ancestry and the codex fingerprint, and the round-1 voids were re-audited and solved in round 2.
  - Main merges the branch head (≥ 73bc99ee) at the scheduled point, after L37 and after look 2 or T, so the dev batch and look 2 keep main's provider throughout.
  - The registration's binary sha and the review packet's provider diff carry the fix.
- **Dev runs on main** have the same naming without a per-call CODEX_HOME. There, a collision can only show up as a spawn failure (0 found in 9,035 dev responses) or as a cwd removed under a running call. This is paired noise that the A/A threshold already includes, and it is left unchanged mid-batch.

### 0.15 Addendum: registration review and dispositions (2026-10-21, before the registration commit)
- **Candidate:** E002 (`runs/c2-confirm2/frozen.py`, sha e772bd6e…). Look 2 (E007) was invalid because of one hidden-test name, coincidental and echoed from a Luna draft into a judge prompt. Under §0.1 the candidate is fixed at the look's result, so it does not wait for T.
- **Reviews** (copies-only packets, `c3_review.py`):
  - Codex Sol (high, web disabled, 0 searches): **Yes** with 4 blockers.
  - Fresh Claude: **Yes** with 3 blockers and 16 non-blocking points.
  - Both answered the open question the same way: imputed CIs on both claims, already adopted.
- **Sol blockers:**
  1. A failed first attempt's integrity evidence was dropped after a relaunch. Fixed: `failed_attempt_audit` counts every invocation record's tool events and counts an invocation without an auditable transcript as one event (fail closed). It also scans the attempt's prompts for hidden test names with a port of the controller's `test_names`. `merge_failed_attempt` feeds all of it into the decision.
  2. The preflight used the binary before the build. Fixed: the build comes first, every probe record carries the binary sha, and the launch refuses a binary that changed since then.
  3. Validator and audit provenance.
     - Validation: `c3_validate.py` now builds its own controller and records the head and binary sha; `--binary` is refused outside tests. Re-validation on 10-21 accepted 144/144 with that record (head caceb9b5, binary 848eb8fe…), and certification requires it.
     - Audit: re-running it would exceed the registered audit allowance, and the rebuilds at both audit heads are not byte-reproducible (59f990ee… and 5c4fd509… vs the recorded 790bcf91…). So the audit binary's provenance is its record plus functional evidence, and that is disclosed: `--json` transcripts with the provider's marker in every audited tuple, the codex bracket held, and the provider commit in each head.
  4. Compile-time file reads in the validator's unsandboxed API-surface compile. The counts-only audit found 0 `include_str!`/`include_bytes!`/`include!`/`env!`/`option_env!` and 0 sealed-path mentions in the 1,152 suite files and the 5,456 readable authoring files. No further authoring or validation runs for this set. Disclosed.
- **Claude blockers:**
  1. The runner had never run end to end on real data. Fixed:
     - a **dress rehearsal** (`c3_rehearsal.py`, 2 dev tasks, real binary and exact flags): generate ×2, manifest, score, pool and decide all ran, and runner voids 0 = controller `tuples_void` 0 in both replicates;
     - the submissions' claimed invocations include the judge-lane calls;
     - arms are compared as a set, and malformed records or summaries void or invalidate instead of crashing.
  2. Deterministic preconditions came after the attempt. Fixed: the suite hash (`a2d lift hash`) and the crates/Cargo binding (diff against the registration commit, nothing untracked) are checked after the build and before any probe or attempt. Scoring re-checks the suite hash.
  3. Invocation-level tool events were ignored. Fixed (a §0.4 tightening): any invocation record with `tool_calls > 0` voids its tuple, and the per-replicate rate uses the larger of the submission and invocation sums.
- **Claude non-blocking, taken:**
  - (2) each replicate's own summary feeds the hidden-name hits and the integrity flag;
  - (3) RULED OUT also needs the best-case point;
  - (4) build first;
  - (5) launch detached (the queue runner is detached; stated in the registration);
  - (6) other Luna work is held during generation windows;
  - (7) the registration texts were corrected: latency probes, the void-tuple text, invocations versus Luna;
  - (8) disclosures updated: requirements-only repairs, the step order, the mutants fix, §0.14, spent sets and looks;
  - (9) the provider head is named;
  - (10) calls and tokens by lane are in `decision.json`;
  - (12) the holdout dirs must agree;
  - (15) no codex upgrade during the windows;
  - (16) `registration.json` is committed together with these dispositions.
- **Non-blocking, answered:**
  - (1) Instrument errors count as tool events (fail closed, not loosened). On the merged provider, dev runs so far show 0 tool events (rehearsal 44 calls; phase-B round 1). Disclosed.
  - (11) `skipped_days` affects disclosure accuracy only; noted.
  - (13) No retry layer sits between `MeteredProvider` and `CliProvider`; a failed call is recorded and spent.
  - (14) An operator-killed run is a failure that permits the one relaunch; the reason will be logged, and the partial ledger stays unread.
- **Re-check round** (narrow, on the fixes):
  - The fresh Claude answered **Yes**: all 7 fixed as dispositioned, nothing loosens §0.4, no new blockers.
  - Sol answered **No**, with these open; all now fixed (675b77c9):
    - (a) Failed-attempt accounting was not conservative. Now: the larger of the submission and invocation sums plus one per unauditable call. Any unauditable call, or a failed attempt's own rate above 0.5%, blocks the claim. A dead run's cut-off ledger lines count as unauditable and do not crash scoring.
    - (b) `classify()` could crash on a malformed comparison. `well_formed()` now gates it, and a malformed comparison is Invalid.
    - (c) Audit provenance was not enforced. `c3_rescore_audit.py` re-scores every task's final audit-round submission with an oracle independent of `a2d`: python + rustc, compile and run under seatbelt profiles modelled on the controller's, with the oracle's compile environment. Result: **144/144 agree** (run 1: 139, run 2: 5), 0 compile failures, 0 timeouts, committed as `runs/c2-confirm3/audit-rescore.json`. Certification requires it.
    - (d) The compile channel was disclosed, not closed. `c3_channel_audit.py` with a committed result (`channel-audit.json`): 0 macros and 0 sealed-path mentions over the suite, the authoring tree and both audit runs. The validator refuses to run once a registration exists. The re-scorer's sandbox blocks a compile-time `include_str!` canary, which compiles without the sandbox. Certification requires the clean result.
  - The fresh Claude's non-blocking points, taken:
    - the tolerant dead-ledger parse;
    - each failed attempt gated on its own rate;
    - scoring also checks the crates binding;
    - real-data checks: the runner's hidden-name scan finds look 2's single hit; E007's 35-family comparison is well-formed; the registered suite hash equals a fresh one;
    - a second dress rehearsal pinned to the final runner's sha256;
    - text gaps closed (per-lane canary evidence; malformed or aborted ⇒ Invalid).
- **Sol's second re-check** (No, 4 items partly fixed), all fixed (19d6e1e6):
  - (1) interrupted attempts: ledgers are read tolerantly everywhere (`records_tolerant`). For a failed attempt, a missing ledger or a prompt file without its invocation record is unauditable, and that blocks a claim.
  - (2) malformed summaries: the container, the comparison list, finite numbers and the guard fields are validated, and anything malformed is Invalid; `well_formed` rejects NaN and infinities.
  - (3) audit provenance: the audit records must carry a binary sha. `c3_rescore_audit.py` also recounts every final-round audit transcript with the bound runner's own detector port: 144 transcripts, 0 missing, **0 events**. Certification requires it.
  - (4) the channel scan uses a string-aware Rust lexer that removes line and nested block comments, plus `#[path]` and `mod x;` loaders and a permissive raw-text pattern: 0 macros, 0 loaders, 0 sealed mentions.
- **Third re-check** (Sol No, fresh Claude No):
  - **Fresh Claude's blocker, fixed (fdb0f739):**
    - The recount, `host_off` and the ledger reader split on `\n` only, like Rust's `str::lines()`. Python's `splitlines()` also split on U+2028, U+2029 and U+0085, which serde_json leaves raw inside strings, so one benign model output could have voided tuples or made a replicate IntegrityReview.
    - JSON parsing is as strict as serde (no NaN or Infinity; deep nesting is unparsable).
    - It confirmed that §0.16 cannot be forged and does not bias toward SUPPORTED.
  - **Sol's items:**
    - (1) A call in flight when a run dies left no record. Fixed: the meter writes a durable `invocation_started` before each call, and an unmatched start in a failed attempt is unauditable and blocks the claim.
    - (2) Summaries were touched before validation. Fixed: `cmd_score` validates the pooled and replicate summaries before folding them, and `decide()` rejects boolean or non-positive `min_families` and shares outside [0, 1].
    - (3) The audit's historical launch isolation (its CODEX_HOME and environment) cannot be re-verified. **Dispositioned, not fixed:**
      - the audit decides which tasks were certified, not how the arms compare (every arm runs every certified task);
      - its verdicts and tool events are re-derived without its binary (re-score 144/144, recount 0 events);
      - re-running it would exceed the registered audit allowance;
      - disclosed.
    - (4) The scanner missed aliased imports and non-ASCII `mod` names. Fixed (aliases with a `::` path, bare macro names, any `mod x;`), and the artifact is still clean. The disclosure states that a lexical scan is not a proof (user-macro indirection is not detected) and that the validator refuses to run after registration.
- **Stopping rule for the review loop** (written 10-21, before the final re-check): after a fourth narrow re-check, any remaining finding is fixed only if it can bias the arm comparison toward SUPPORTED or break the bound runner on data it will actually see. Findings that fail closed (voids, Invalid, IntegrityReview), or that bear only on certification or historical artifacts, are dispositioned and disclosed instead. The plan asks for a review; it does not require unanimous assent to every hardening.
- **Fourth re-check** (under the stopping rule; Sol No, fresh Claude No): both found the same single in-scope blocker. `load_summary_obj` turned an unparsable or non-object replicate `summary.json` into a sentinel dict, and `fold_replicate_summary` let it pass as clean (fail-open; reproduced: truncated, `null` and `["h"]` gave SUPPORTED).
  - Fixed with their minimal fix (ae853669): the sentinel, a non-dict, or a summary without `integrity_ok` blocks the claim. A regression test runs the real path (load → fold → decide on `""`, `{` and a summary missing its flag).
  - The final runner re-scored rehearsal 4's data and reproduced its committed decision.
  - Non-blocking points taken: the Invalid wording (a missing replicate summary is IntegrityReview), and a relaunch disclosure (a first attempt that dies with calls in flight ends IntegrityReview).
  - Dispositioned:
    - the residual parser differences, which cannot loosen anything because the binary's count is maxed in;
    - same-key scope masking, which is not present (44/44 starts matched);
    - the descriptive `controller_last_change`.
- **Confirmation of the last fix** (Codex Sol, narrow, 0 searches): **VERDICT: Yes**. The blocker is fixed (`nano/confirm3.py:268-269` and the load → fold → decide regression), and the fix introduces no in-scope finding. With every in-scope blocker from both reviewers fixed, the registration is committed with these dispositions.
- **Not changed:** the hidden-name rule is not narrowed (§0.10). Its false-positive risk (about 10–15% for an IntegrityReview with no leak) is disclosed with the candidate's wording.

### 0.16 Addendum: `code_mode_host` off; the bound runner recounts every transcript (2026-10-21, before registration)
- **Evidence:** phase-B round 1, the first dev run at scale on the merged provider.
  - A Luna coder call (X:inf) called codex's **built-in** `list_mcp_resources` and `list_mcp_resource_templates` (server `codex`; both returned empty lists). That is 4 events (started and completed counted), a rate of 0.89%, so the run was integrity-invalid and used the round's one re-run. The re-run again had 2 such events (0.44%).
  - Earlier rates: 7 in about 5.7k dev calls (C2-MCP), 3 of 1,240 in look 1 (all X), 0 in confirmation-2's replicate 1.
  - At about 0.12–0.24% of calls × 2–4 events, a replicate of about 1.6k calls crosses §0.4's 0.5% line in roughly a third to half of replicates, and either replicate crossing makes the run IntegrityReview. Harmless empty listings would have decided the claim.
- **Probes** (live, Luna low, the provider's exact arguments, an isolated home):
  - with `code_mode_host` on, a call asked to list MCP resources does;
  - with it off, the tool does not exist (the model answers NO_TOOL). A configured MCP server is not callable either, so the host carries every MCP tool;
  - codex then emits one fixed `error` item, `CODEX_HOST_OFF_NOTICE`, after `thread.started` and before `turn.started`;
  - per-server overrides cannot remove the built-in tools (codex rejects them);
  - the injected instructions are unchanged (prompt-input differs only in ids and timestamps).
- **Change** (19d6e1e6), a tightening of the instrument: the tools are removed rather than tolerated.
  - The provider disables `code_mode_host`.
  - The detector exempts exactly one byte-identical notice that is an `item.completed` before `turn.started`. The same text inside the turn, any other error, or a second notice counts. Unit-tested.
  - The bound runner (`nano/confirm3.py`) ports the detector line for line (`py_tool_events`) and recounts every transcript, independently of the binary's recorded counts. `host_off()` requires the notice before every turn that started. Either check voids its tuple, and the rate uses the larger of the submission, invocation and recount sums. The preflight canary requires both.
  - Live controls (re-run at 19d6e1e6): shell, web, file and sub-agent are counted; MCP is counted with the host switched back on for that control only; the **notice** is present and counted as 0; the negative control shows the notice before the turn and 0 events.
- **What does not change:** §0.4's thresholds and void rules; `PROVIDER_COMMIT` (bc6922f7); certification (ancestry; the audit evidence is the re-score and recount, §0.15).
- **Disclosures:**
  - dev and the dev-holdout looks ran with these tools available and occasionally called; any such tuple was voided there too, so the dev estimates already exclude them;
  - phase B round 1 ran on the old provider and rounds 2–6 run on the new one. The precision metric pairs arms within a round, so this is a comparability note, not a bias.

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
