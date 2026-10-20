# A²D Handoff Document

**Last updated:** 2026-10-20 16:20 UTC (session 145, cont.: **E007 PROMOTED**. Its confirmation, launched early at the owner's request, gave +17.1 pp all-pass (SE 4.0), 137/140 vs E002's 113/140. A/A rebuilt from E007's replicates. **Look 2 is armed for 00:05 UTC 10-21** (detached, pid 13208); if it passes, E007 is the confirmation-3 candidate. Confirmation-3's provider fix and pre-check are done (plan §0.14).)
**Update this document:** before context compaction, at session end, or when significant state changes. Keep it short: current truth only. History lives in git, `docs/experiments/LOG.md` and `docs/handoff-archive/`.

## Headline (read this first)

- **Owner's framing:** A²D is *autoresearch on agentic coding harnesses*, and you have the reins. Never pause or ask for direction. A failed variant is a data point, not a verdict.
- **Active goal: cycle 2,** the session `/goal` of 2026-10-15. Working plan: `docs/plans/2026-10-15-cycle2-nanoharness.md` (its addendum holds the design-review dispositions and definitions).
  - Done when a frozen harness is SUPPORTED vs both D and B on confirmation-2, or when the budget (≤ 3k Luna/day, ≤ 25k total) or the deadline (2026-11-30) is reached.
- **Cycle 1 is closed** (falsified roadmap). Report: `docs/reports/2026-10-15-lift-first-outcome.md`.
- **Incumbent = E002** ("Luna generation plus Sol selection", `nano/harness.py`; v0 = D is at `nano/history/before-E002b.py`).
  - 4 Luna samples, then one Sol judge call that picks among the distinct public-green files from the spec alone.
  - Exploratory dev-set result vs v0 (= D): screen +11.8, full +10.7 (SE 2.9), confirm +7.1 (SE 3.5).
  - **Not cost-matched:** about 4.0 Luna + 0.9 Sol calls and 6.4M tokens per 140 tasks, vs about 1.3 Luna and 1.4M for D.
  - Sol is from the same vendor as Luna, so this is not a cross-family claim.
- **Keep rule (eval v2.3, `nano/eval.py`):** all-pass is primary and fraction is a guard.
  - Screen (half): Δ ≥ thr/2.
  - Full: Δ > max(thr, 1.645·SE), with Δfrac not significantly negative.
  - Fresh confirmation replicate: Δ > thr/2, then promote.
  - `nano/aa.json` is E002's A/A, taken from its own two full replicates: thr_allpass 0.0357, thr_frac 0.0174.
- **Results log:** `nano/results.tsv` (every experiment and stage, including discards). Narrative and pre-registrations: `docs/experiments/LOG.md`, Cycle 2.
- **Report draft:** `docs/reports/cycle2-nanoharness-report.md`. The settled sections are written; fill in the **[pending]** ones at close.

## In flight / next
0. **State, 10-20 16:20 UTC (read first; supersedes the 07:10 block below):**
   - **Incumbent = E007** (Sol-directed repair). Promoted 15:56 UTC on its confirmation: +17.1 pp all-pass (SE 4.0) vs E002, discordant 24 vs 0. `nano/harness.py` = `runs/c2-E007/candidate.py`; `nano/aa.json` was rebuilt from E007's replicates. The batch's confirmation step is done; E005 and E010 stay unconfirmed provisional keeps.
   - **Look 2:** a detached job (pid 13208, `~/.a2d-queue/look2-early.out`) runs the L38 line (`look_if_promoted.py`) at ≥ 00:05 UTC 10-21, taking about 3 h. Only `runs/c2-look2/gate.json` reaches the repo. PASS ⇒ the confirmation-3 candidate is E007, with the E007 entry of `c3_wording.json`; FAIL ⇒ E002.
   - **After look 2:**
     1. Merge `c2-web-provider` (≥ ecbb9611) into main, build, run the tests, log it.
     2. Phase B from E007. Pools are done (`runs/2026-10-20-c2-e007-pool/`, screen + rest + confirm). Then stats with `--runs` screen=S, rest=R, confirm=`tasks/train-all`, then dossiers, `propose` and `round` (steps below).
   - **Queue (pid 7116):** L23–L29 at 00:05 are no-ops after the promotion (E-resumes fail on the new aa.json by design; `confirm_best` says already promoted). L30 holds until 10-22; L38 and L40 will print "already attempted".
   - **Luna:** about 2,705 today (10-20); about 11.2k left of 25k.
0. **State, 10-20 07:10 UTC:**
   - **Batch E005–E010 vs E002 is complete to full.** Provisional keeps: E007 +7.1 (SE 3.2), E010 +5.0 (SE 2.6), E005 +5.0 (SE 2.8). Discards: E006 (full), E008 and E009 (screen).
   - **Queue (runner pid 7116, alive):** waits at L22 until 00:05 UTC 10-21.
     - L23–L27 are no-op resumes. L28 `confirm_best` runs E007's confirmation (worst case 1,400 Luna; deferred on 10-20 at 1,673 + 1,400 > 3,000).
     - If E007 is discarded, L29 confirms E010 (tie with E005 broken by the smaller SE). At most 2 confirmations.
     - L36 and L37 (10-22) repeat as needed. L38 and L40: `look_if_promoted` runs look 2 by T = 10-23 23:59 UTC if a keep was promoted.
   - **After a promotion:** `python3 -I nano/eval.py aa-from-replicates --id <id>`, then phase B from that incumbent (`c2_phaseb.py`, by hand). The 3-keep trigger has fired; phase B still waits for the confirmation step.
   - **Phase-B kickoff, by hand.** Order: the batch confirmation step, then look 2, then the provider merge and rebuild, then round 1.
     1. Pools: `python3 -I docs/experiments/analysis/candidate_pool.py <suite> <out.json> <run>` for each of the incumbent's screen, rest and confirm runs.
        - E007's screen and rest pools are done (`runs/2026-10-20-c2-e007-pool/`); E002's are in `runs/2026-10-16-c2-e002-pool/`.
        - If E010 is the incumbent, score with `candidate_pool.py --all-blocks` (both files of each reply). E010's screen and rest pools are done (`runs/2026-10-20-c2-e010-pool/`).
     2. `c2_phaseb.py stats --runs <screen>=<S> <rest>=<R> <confirm>=tasks/train-all --pool … --arm X:cand --out runs/c2-pc/round1/stats.json`.
     3. `dossiers … --round 1 --max 8`.
     4. `propose` for both arms.
     5. `round --round 1`.
     - The driver pattern is in the smoke (LOG 10-17). Stats now name the incumbent's own branches (ee9453ff).
   - **Confirmation-3:**
     - The provider branch head is ≥ 73bc99ee. It holds the cwd fix (36bea1e1) and controls re-run complete at that head, and main was merged into it at ecbb9611.
     - On the merged tree all registration predicates are true (LOG 10-20 01:15).
     - What remains: the merge into main (after L37, and after look 2 or T), the candidate at T, `c3_registration.py`, the review, the commit, then the spend ≥ 10-24 on the day after registration.
   - **Luna:** 11,206 ledgered plus about 40 for controls at 10-20 01:20; 1,673 today at 07:00.
1. **Confirmation-2 (C2-CONFIRM2):** registered at `78727f29e53743dc5d989c62e1de23ab1a81e6c6`. The spec is `runs/c2-confirm2/registration.json`; the LOG entry holds the review dispositions and the power exception.
   - **Outcome fixed: IntegrityReview, no claim** (LOG 10-18 03:59 UTC). The replicate-1 web audit (`runs/c2-confirm2/web-audit-r1.json`) shows B 2 / 720, D 0 / 185 and X 0 / 713.
     - Don't narrow the rule, and don't look at what was searched: no transcripts, task ids or file order.
     - Replicate 2, its audit, `confirm-manifest` and `confirm-score` still run as registered. Report the `decision.json` numbers as **descriptive**.
     - Keep the freeze until `decision.json`. After it: merge `c2-web-provider` (4b80fe67), smoke it live (0 searches), then log it.
     - **Next goal path: confirmation-3.** Its pre-commitment is fixed in `docs/plans/2026-10-18-confirmation-3.md` §0 (fb8b55c9, 1958ba0a; LOG 10-18 ~05:30). The registration copies it.
       - Candidate: the batch winner iff it is promoted and passes one look by 10-23 23:59 UTC; otherwise E002.
       - The spend is at or after 10-24.
       - The order of work is in §0.6: blind power script → pipeline in the repo, tested on visible fixtures → capability inventory and allowlist detector → authoring after replicate 2's audit → validation and audit → registration and its review.
       - **State (10-19 13:20 UTC):**
         - Authoring is done (36/36), assembly is done (144 tasks), similarity is clean, and **validation passed 144/144** (after one repair, c3-f13). The suite is built.
         - Set STOP (`c3_author.py --stop`) only after adjudication; the drain check before each replicate needs it.
         - **Step 5 waits for the provider merge.** The merge comes after L37 (the batch's confirmation step, 10-22) **and** after look 2 is done or T has passed.
         - At the merge:
           - merge the `c2-web-provider` head (≥ 73bc99ee: the provider with the cwd-collision fix 36bea1e1, main's tooling and the committed `controls.json` re-run at the fix) and build. Do not re-pin `PROVIDER_COMMIT` (bc6922f7).
           - Re-run `c3_controls.py` only if the candidate's lanes differ from Luna low and Sol medium.
         - **Step 5 is done (10-19 15:54): the suite is certified.** Audit round 1 solved 139/144 (1 failure, 4 voids). The failure was adjudicated UNDERDETERMINED and repaired, requirements only. Round 2 solved all 5. `certified()` is True, and the pipeline is stopped and drained.
         - Only the merge, the candidate (at T), the registration and its review, and the spend remain.
         - **Registration:** after T, `c3_registration.py` (dry-run first; it lists any unmet prerequisite), then a full review (Sol plus a fresh Claude) on the written `registration.json`, then commit.
           - The candidate's `--frozen-from`, `--claim-wording` and `--branch-caps` are drafted in `docs/experiments/analysis/c3_wording.json` (E002, E007, E005).
           - Spend ≥ 10-24, on the day after the registration's day, via `nano/confirm3.py generate` (queue lines).
         - **Early reviews:** 3 rounds; every blocker fixed (plan §0.13). Round 3: the fresh Claude said Yes (with fixes), Sol said No (7, all fixed). The next review is the full one on the registration.
       - **State (10-18 14:40 UTC):**
         - Step 1 is done: `c3_power.py` gives null RULED OUT 0.91 and SUPPORTED 0.47 at +10 pp.
         - Step 2 is done: the pipeline is in `tools/holdout/` (read its README first). Sol's 5th re-check (134393a2) says **Yes**. Plan §0.12 and the LOG (10-18) cover 4 rounds of fixes:
           - a copied `open` escaped the profile, so Mach lookups are now an allowlist and localhost is denied;
           - quiescence is by sandbox membership, plus the launch handshake;
           - the STOP gate, selection generation, raising inventory traversal and terminal states are in.
         - Step-5 tooling is built and tested: `c3_audit.py`, `c3_adjudicate.py` and `authoring/ADJUDICATE.md` (737e826c).
         - **Next (≥ 10-19, after L09 and L10 finish and `web-audit-r2.json` is committed):**
           1. confirm `fseventsd` is healthy, then `tools/holdout/detached.sh c3_select.py`, following `~/.a2d-c3-drivers/*.out`;
           2. then `detached.sh c3_author.py --parallel 3`, in idle queue windows, never overlapping a confirmation generation;
           3. then assemble → similarity → validate/fix.
         - Never run the long drivers as Claude Code background shells.
         - **Host note:** the owner's `~/bin/agents-disk-maintenance` (`cargo-sweep --recursive ~/Projects`) floods `fseventsd` while it runs. File-creating processes then stall (an unsandboxed hello-world compile took 3.5 min). Don't read a timing-bound test failure as a regression without checking `top` for a stuck `fseventsd` first.
         - Step 3: provider and controller hardening on branch `c2-web-provider` (be96d21b, 27757c6b). Merge it after `decision.json` **and** the batch confirmation step.
         - **Next:**
           - after replicate 2's audit is committed (≥ 10-19): `python3 -I tools/holdout/c3_select.py`, then `c3_author.py --parallel 3` in idle queue windows;
           - before step 5: adjudication tooling, positive-control probes per tool per lane, and an `eval.py` CONFIRM3 scoring section with worst-case void imputation.
     - **Seal confirmation-2's scores (§0.8):**
       - Run `confirm-manifest` and `confirm-score` with stdout redirected to `~/.a2d-holdout/runs/`.
       - Move `decision.json` and `pooled-summary.json`, **unread**, to `~/.a2d-holdout/runs/c2-confirm2-decision/`. Commit only their sha256.
       - Read them only after confirmation-3's decision.
   - **Owner request (pending):** please pause the A² session during confirmation generation windows: replicate 2 at ≥ 00:05 UTC 10-19, and confirmation-3's replicates at ≥ 10-24. Luna calls took 60–175 s on 10-18.
   - **Luna ledger (25k total):**

     | | Luna |
     |---|---|
     | Spent, from all cycle-2 ledgers on 10-20 01:20 UTC | 11,206 (dev 7,166; holdout 4,040 = look 1 + confirmation-2 r1 and r2), plus about 40 off-ledger in the live detector controls (4 runs) |
     | Left | ≈ 13.75k (10-19: about 2.56k of the 3k cap; 10-20 so far: 171) |
     | Projected left after the batch and its confirmations (10-17 projection) | ≈ 8.6k |
     | Ring-fenced: confirmation-3 plus relaunch reserve | 4.5k |
     | Candidate look (only if a keep is promoted; it doubles as the final look) | 1.3k |
     | Phase B (rounds only; R = 6 ≈ 2.6k, floor R = 4) | ≈ 2.8k worst case |

     Update the ledger daily: `python3 -I nano/eval.py budget` gives today's spend; the total comes from the ledger count script in the 10-18 session.
   - **The queue runner is RUNNING** (pid 7116, started 01:35:43 UTC 10-18 on the owner's request; check with `kill -0 $(cat ~/.a2d-queue/runner.pid)`). Replicate 1 started at that time. Before that it had been stopped by Claude Code's memory reaper (15:23 UTC 10-17, idle before line 5, nothing half-run); see the LOG entry of that time.
     - **Restart (if it dies):** `zsh ~/.a2d-queue/start.sh` (detached, `nohup caffeinate -i`, refuses a second copy). Run it from the owner's terminal, or by Claude only when the owner asks; **never as a Claude Code background shell,** whose reaping mid-replicate would spend the single relaunch.
     - Output: `~/.a2d-queue/runner.out`; per-line logs: `~/.a2d-queue/qlogs/`; pid: `~/.a2d-queue/runner.pid` (check with `kill -0`).
   - **Queue** (`~/.a2d-queue/queue-2026-10-18.txt`, re-read line by line; its line n is the old line n+4):
     - at ≥ 00:05 UTC 10-18, `L05-confirm-r1` (`python3 -I nano/eval.py confirm --rep 1 --registered-at 78727f29… --jobs 8`), then the web audit, E007 resume, E006;
     - at ≥ 00:05 UTC 10-19, `L09-confirm-r2` the same way, then its audit, E006 resume, E008, E009 and E010. Both confirm lines retry any "deferred" at the next 00:05 UTC (≤ 5 days), so a late start still gives two distinct UTC days;
     - at ≥ 00:05 UTC 10-20, 10-21 and 10-22: resumes for E006–E010, then `eval "$(python3 -I nano/confirm_best.py E005 … E010)"` (once on 10-20, twice on the later days). `confirm_best` enforces the pre-registered maximum of 2 confirmations. The date waits are lower bounds; a slip only shifts them.
     - **Every line is `~/.a2d-queue/qrun.sh <tag> ~/.a2d-queue/qcmds/L<nn>-<tag>.zsh`.** The full output goes to `qlogs/<UTC>-<tag>.log`, and the runner output shows one `"decision": "qrun <tag> rc=<n> … last=<line>"` per line. Any `rc≠0` is a failure to investigate.
     - The runner itself (`queue-runner.sh`) must not be edited while it runs, and lines ≤ the current one must never change. Two copies-only reviews (Sol and a fresh Claude, 10-17) checked this automation; their dispositions are in the LOG.
     - **Expected noise after a promotion:** once `confirm_best` promotes, every later E-resume line ends `rc=1 last=nano/aa.json does not match…`. That is by design: nothing runs against the new incumbent until `aa-from-replicates`.
     - **Safe runner restart** (if the runner dies again):
       1. Find the last `=== … [N] …` marker in `runner.out`, and check whether `[N] done` was printed.
       2. Create a new queue file from line N+1 on, or from line N if it never finished, copying the `qrun.sh …` lines; point `start.sh` at it.
       3. A confirm line that died mid-run is **not** replayed: follow the registered relaunch policy below. Look 1, E005 and the E007 screen are done and never replayed.
     - **Watchers:** optional. Only the runner's survival matters; `caffeinate` now wraps the runner itself. Milestone checks use `kill -0 $(cat ~/.a2d-queue/runner.pid)` and `grep "[N] done"`, never `pgrep -f`.
     - A cap-deferred stage prints "deferred". Re-running a command is free and safe: completed stages are cached, and `log_result` logs each (id, stage, decision) once. So over-provision resume lines rather than re-queue by hand. Append lines to the end only.
   - **End of the batch (pre-registered in the LOG, 10-17):** `confirm_best` mechanically confirms the largest full-stage provisional keep, at most 2 confirmations. This happens even if the 11k phase-B trigger has fired.
     - If one is promoted: `python3 -I nano/eval.py aa-from-replicates --id <id>`, then phase B starts from that incumbent. Otherwise phase B starts from E002.
     - Phase B is started by hand (`docs/experiments/analysis/c2_phaseb.py`).
   - **After both replicates have generated:**
     1. `python3 -I nano/eval.py confirm-manifest`;
     2. commit and push `runs/c2-confirm2/manifest-r*.json`;
     3. `python3 -I nano/eval.py confirm-score` (jobs 6). It writes `pooled-summary.json` and `decision.json`. Only the pooled claim decides, **subject to C2-WEB:**
     4. `runs/c2-confirm2/web-audit-r{1,2}.json` (queued after each confirm line; counts only) must show 0 web-search events. Any search means the outcome is IntegrityReview. This amendment was registered in the LOG before any call. Commit both audit files with the manifests.
   - **Freeze window, through `decision.json`:** do not touch `crates/`, `Cargo.toml`, `Cargo.lock` or the `CONFIRM2…main()` section of `nano/eval.py`. The controller and the runner both refuse to run if they change.
     - Rebuilding the binary from the same source is fine: the binary sha is logged, not enforced, because debug builds are not byte-reproducible.
     - The harness-supplied-tests op idea waits until this window closes. So does the **provider web-search fix (C2-WEB):** add `-c web_search="disabled"` and count `web search:` lines in `codex_tool_calls`, with a test.
     - codex-cli 0.162 ignores `--disable web_search_request`, so web search is ON in every `a2d lift` call until then. There were 0 uses in about 15k recorded transcripts. `eval.py` stops a dev experiment on any search, and phase B invalidates the round.
     - **The fix is ready** on branch `c2-web-provider` (4b80fe67, pushed): provider tests 20/20, a2d-lift 66/66, and a live 1-task smoke with no deprecation notice and a pass.
     - **Merge it after the batch's confirmation step and before phase B round 1,** not right after `decision.json`. Merging mid-batch would change the provider between an experiment's stages (Claude review). Then rebuild and log it.
   - **If a replicate's generation fails or aborts:** the runner refuses to rerun it by design. The registered policy allows one relaunch with the same seed:
     1. move `~/.a2d-holdout/runs/c2-confirm2-r<n>` (and its `.log`) to `…-failed1`;
     2. move `runs/c2-confirm2/attempt-r<n>.json` to `attempt-r<n>-failed1.json`;
     3. log it in the LOG, then re-run the queue line.
     - A second failure means confirmation-2 is "not run": no claim, and the set is spent.
   - **While holdout or confirmation jobs run:** never print process command lines (`pgrep -f` pids only).
2. **Search continues on dev** from incumbent E002 (thr_allpass 0.0357). Look 1's numbers stay in `~/.a2d-holdout/runs/c2-look1/report.json` until the final report; proposers learn only PASS.
   - For each new run, read its first prompts and DIAG lines, then log it in the LOG and `results.tsv`.
   - **E005 (diverse angles): provisional keep at full, +5.0 pp (SE 2.8), 123 vs 116/140.** Its confirmation waits for the sequencing rule: once E006–E010 have reached full, the largest Δ is confirmed first.
     - Per-angle: no per-sample cost, small decorrelation, +5 tasks from generation.
     - The in-run E002 arm drew low (116 vs 123 and 117 on 10-16), so expect a close confirmation. See the LOG.
   - **E007 (Sol-directed repair): screen passed,** 65/68 vs 62/68 (+4.4 pp). Its rest stage was cap-deferred and resumes on 10-18. Its first prompts were read and are as designed.
   - **Phase-B tooling:** live-smoked end to end (`runs/c2-pc-smoke2`, after the C2-WEB fix): valid proposals, the round runs and scores. The driver is `$SCRATCH/phaseb_smoke.py <root> <family> <dir>`. The real `runs/c2-pc` is not created yet: the first `propose` freezes the incumbent.
3. **Proposer comparison (phase B)** triggers at ≥ 3 keeps or 11k Luna left. The protocol is frozen (plan addendum 2026-10-16) and the tooling is ready (`docs/experiments/analysis/c2_phaseb.py`).
- **C2-ISO2 (merged 32e72a53):** the oracle's compile and test run now execute under sandbox-exec.
  - Both steps: no network and no file access under `$HOME` or the temp roots.
  - Run step: no process spawning, Mach lookups or Apple Events.
  - The environment is cleared, and the harness sandbox gets the same broker closure.
  - Parity: 420/420 identical against the old binary. References: 140/140, 100/100 and 144/144.
- **Claude-quota work** (the quota resets Sunday 12:00): C2-E2 (Claude judge 66/68 vs Sol 68/68), C2-FA2 failure taxonomy, proposer round c1 (E009, E010), and the reviews for C2-ISO2 and C2-CONFIRM2. There is no live Claude lane: the CLI is blocked inside the session sandbox.
- **Budget:**
  - Cycle-2 Luna: 275 (10-15), about 2.9k (10-16), and 1,060 (10-17 so far) plus about 22 dry-run calls outside `runs/`.
  - About 20.7k Luna left of 25k. Confirmation-2 costs about 1.6k Luna and 125 Sol per replicate.

## The instrument (`a2d lift`, `crates/a2d-lift/`)

```bash
cargo build -p a2d
./target/debug/a2d lift validate <suite> --jobs 4
./target/debug/a2d lift run --experiment <id> [--unregistered | --registered-at <sha> --expect-suite-hash <sha>] \
  --suite <dir> --model codex/gpt-6-luna --effort low --arms D,C:a,C:b --harness a=<json> --harness b=<json> \
  [--d-note n=<file> (arm D:n)] [--claim "C:a>C:b,C:a>D"] --k 5 --primary D --seed <n> --jobs 4 \
  --max-invocations <n> --max-wall-clock-secs <s> --out <dir> [--defer-scoring]
./target/debug/a2d lift manifest|score|pool|compare|verify-ledger ...
./target/debug/a2d distill --from <run>=<suite> --harness <in> --out <out> --log <prov> --name <n> \
  [--arm C:x] [--families <json>] [--mode informed|uninformed] [--max-dossiers 40]
```

- **Arms:** A (one-shot), B (best-of-k), D (repair loop), C / `C:<name>` (A²D metabolism with a harness), `D:<name>` (D with a note).
- **Oracle:** the candidate is the crate root, and the trusted tests are an earlier-declared `#[path]` module. Sources are deleted before the run, and a clean exit is required.
- **Classifier:** a whole-family bootstrap with a boundary tolerance of 1e-9. `--claim` = all listed comparisons Supported; IntegrityReview and Invalid dominate.
- **Arm-C harness keys** (`crates/a2d-cli/src/lift_arm_c.rs`):
  - `drop_enzymes`, `drop_coder_catalysts`;
  - `coder_prompt` (must keep `main()`), `coder_note`, `lessons`;
  - `route_previous_code`, `stop_at_public_green`, `dedupe_input`;
  - `prompt_format` (json, plain or repair), `escalation`.
- **Harnesses:** `harnesses/h0-v6.json` (parity base), `s4b-lessons.json` (researcher lessons), `p4-{i1,i2,u1,u2}.json` (search outputs).
- **Analysis scripts** (`docs/experiments/analysis/`): `p4_search.py` (the ratchet orchestrator, resumable), `c2_power.py`, `c2_joint_power.py`, `t0_stage.py`, `p4_0_probe.py`.

## Data
- Visible: `tasks/calibration-v2` (24), `tasks/dev-v2` (48), `tasks/c1-train` (68), and the combined `tasks/train-all` (140) and `tasks/train-clean` (72), both made of symlinks.
- Metric-only: `~/.a2d-holdout/c1-validation` (80). 2 of 4 looks used (S4, E12). Read only `summary.json` aggregates.
- Dev-holdout: `~/.a2d-holdout/dev-holdout`, 100 tasks in 25 new families, hash `9114bae3…`. Validated and audited 100/100.
  - Metric-only (aggregates only).
  - Looks: after each confirmed keep (at most 4) plus a final one. Gate to confirmation: incumbent vs D all-pass ≥ +10 pp, with CI low > 0.
- Claim set: `~/.a2d-holdout/confirmation-2` (144). Never opened; unspent. Valid only in the current regime.

## Provider state
- Working lanes: Codex (`gpt-6-luna` coder, `gpt-6.1-sol` reviewer and author) and Claude subagents. The other subscriptions were stopped on purpose. The A² session shares the Codex account.
- Codex gotchas:
  - run reviewers and authors in copies-only temp dirs;
  - close stdin;
  - give the sandbox the toolchain's `rustc` path directly;
  - git commits from inside Codex's sandbox fail (index.lock), so commit outside it;
  - **never `ps -o command` while holdout jobs run** (argv carries prompts).

## Tests

`cargo test` is all green (24 result lines). Provenance tests in `a2d-cli` fail when there are **untracked files under `crates/`**, so `git add` first. The `a2d-lift` tests take about 30–90 s.
