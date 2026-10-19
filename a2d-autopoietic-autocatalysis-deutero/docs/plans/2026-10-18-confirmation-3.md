# Plan: Confirmation-3 — a new blind confirmation set for cycle 2

**Created:** 2026-10-18
**Status:** Active. **§0 is a pre-commitment, fixed 2026-10-18 before confirmation-2's replicate 2 (≥ 00:05 UTC 10-19) and before `confirm_best` (≥ 10-20).** §§1–5 are the reviewed draft, read with the dispositions in §7. Nothing is authored before the work items in §0.6 that come ahead of authoring are done.
**Reviewed:** 2026-10-18, draft design review by Codex `gpt-6.1-sol` high (no tools, web disabled, everything inline; 0 search events) and a fresh Claude subagent (copies only). Dispositions in §7.
**Parent:** `docs/plans/2026-10-15-cycle2-nanoharness.md`, addendum 2026-10-18.
**Addendum:** 2026-10-18 ~12:00 UTC — §0.12. Sol's second pipeline re-check found 5 startup blockers. A live probe then showed that a *copied* `open` launched an app outside the sandbox. All 5 are closed in code, and one gap in the frozen rules is filled: an authoring-exhausted family is replaced. Still before any selection call.
**Addendum:** 2026-10-19 ~07:00 UTC — §0.13: the scoring shape (imputation binds the point threshold, as in confirmation-2; a best-case guard on RULED OUT). A failed call has no transcript and so voids its tuple; this is fixed on the provider branch before registration. Written during selection, before any authored content.
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
  - At merge, `c3lib.PROVIDER_COMMIT` becomes d7f04741.
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
- **Question for the registration review:** should the CI also be computed under imputation? That would need a `pool --impute` flag in the controller, landing with the ≥ 10-22 merge. Until then the CI is on complete tuples and imputation binds the point threshold, as in confirmation-2.

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
