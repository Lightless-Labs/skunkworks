# A²D Experiments Log

Pre-registration and results for `docs/plans/2026-10-14-lift-first-roadmap.md`. An entry is written **before** a live run; its Result is filled in after. Never edit a pre-registration after the run starts — add a dated Deviation line instead. Keep entries short; artifacts carry the detail.

## Entry template

```
### E<NN> — <short name>  [phase P<n>]
- Registered: YYYY-MM-DD (commit <sha>; controller/oracle paths frozen at this sha)
- Hypothesis:
- Arms / configuration:
- Task set: <calibration | development | confirmation-1 | confirmation-2>, suite id + hash, n =, families =
- Model(s) (pinned version/settings):     k =      replicates =
- Budget cap: <invocations>, <tokens>, <wall-clock>
- Infrastructure exceptions (predefined):
- Result-class rule: SUPPORTED / RULED OUT / INCONCLUSIVE thresholds (default: roadmap § Metric)
- Result: <pending | class, lift with paired cluster-bootstrap 95% CI, exact McNemar p>
- Artifacts: runs/<date>-<id>/ (ledger: <path>)
- Deviations: <dated lines, never edits above>
- Decision:
```

## Entries

### E01 — tiny real arm-A run  [phase P0]
- Registered: 2026-10-14 (commit fd3a5b7; controller paths frozen at this sha)
- Hypothesis: none about lift. P0 exit check: the controller drives a live Codex lane end to end (invocation, code extraction, timeout path, hidden scoring, ledger). Secondary observation: Luna one-shot hidden pass on the legacy tasks, as a saturation hint for P1.
- Arms / configuration: A (one-shot), k = 1
- Task set: development, `tasks/dev` (suite hash 7782021a79ff08ace611cfa755d96302fc2dcc6366c27b1d368ff827074a924f), n = 3, families = 3 (legacy chess, rubiks, sudoku; hidden tests only)
- Model(s): `codex/gpt-6-luna`, reasoning effort low, provider timeout 300 s (default)     k = 1     replicates = 1
- Budget cap: 3 invocations, 1800 s wall-clock
- Infrastructure exceptions (predefined): none. A provider error or timeout counts as a task failure.
- Result-class rule: not applicable (single arm, no comparison). Report hidden pass rate per task.
- Result: **2/3 hidden pass** (chess 8/8, sudoku 6/6; rubiks 4/6, failing both solver tests). 3 invocations, 224 s wall-clock, 34–47 s per invocation. No extraction, provider or timeout failures. Token usage was not captured: the Codex CLI provider reports zeros (the count is only on Codex's stderr).
- Artifacts: runs/2026-10-14-e01-tiny-arm-a/ (ledger: runs/2026-10-14-e01-tiny-arm-a/ledger.jsonl)
- Deviations: none.
- Decision: P0 exit item 5 met. Luna-low passes chess and sudoku at pass@1, so the legacy tasks are near saturation and serve as plumbing only; P1 needs a harder suite. Capture Codex token counts before P1 so budget reporting is complete.

### E02 — first live four-arm run  [phase P1, development]
- Registered: 2026-10-14 (frozen paths at commit 2eefa0d; `crates/`, `Cargo.toml`, `Cargo.lock`)
- Hypothesis: none about lift. This is development data: 3 families is below the classification floor, so the class is Inconclusive by construction. Purpose:
  1. all four arms run live end to end;
  2. read arm C's realised schedule at k = 5 from the ledger (does C reach a second coder call?);
  3. zero coder tool calls and zero hidden-name hits.
- Arms / configuration: A, B, D, C; k = 5 (the stated seed schedule needs 5 invocations for a second coder call); primary comparator B; seed 1; jobs 3
- Task set: development, `tasks/dev` (suite hash 6604cf43eb94bfce33a20754636614d8b69dc9b8f887f3fb936eb455d4501128), n = 3, families = 3
- Model(s): `codex/gpt-6-luna`, reasoning effort low, provider timeout 300 s (default), no-tools lane     k = 5     replicates = 1
- Budget cap: 60 invocations (planned max 48), 3600 s wall-clock
- Infrastructure exceptions (predefined): tuples voided by the controller (wall-clock or audit failure) are excluded. Provider errors and timeouts count as failures.
- Result-class rule: descriptive only (development set).
- Result: **descriptive, development.**
  - Every arm passed 2/3 tasks: chess and sudoku pass, rubiks fails. Every comparison has lift 0, CI [0, 0] and is Inconclusive (3 families, below the floor of 10).
  - Rubiks partial hidden passes: C 4/6, A 1/6, B 0/6, D 0/6.
  - Public-green but hidden-fail submissions: B 1, D 1, C 1 (all on rubiks). Its public tests never exercise `solve`, so D stopped after one attempt.
  - Run integrity: complete; 0 void tuples; 0 coder tool calls; 0 hidden-name hits; 0 suspicious constructs; ledger chain verified (66 records, head e61204d9…).
  - Cost: 36 invocations, about 312k reported tokens. Per arm:
    - A: 3 calls, 34k tokens;
    - B: 15 calls, 129k tokens;
    - D: 3 calls, 39k tokens;
    - C: 15 calls (6 coder), 109k tokens.
  - Provider time 983 s in total.
- **Arm C's realised schedule at k = 5:** on all three tasks it ran coder → evolver → architect → tester → coder, so 2 code attempts; the 6th call (evolver) was refused for budget.
- Artifacts: runs/2026-10-14-e02-four-arm-dev/ (ledger: runs/2026-10-14-e02-four-arm-dev/ledger.jsonl)
- Deviations: none.
- Decision:
  - The seed schedule is one feedback cycle per 4 invocations after the first coder call. P2 must pre-register k ≡ 1 (mod 4): k = 5 gives C 2 code attempts, k = 9 gives 3. k = 4 is ruled out because it gives C a single attempt.
  - The P1 suite's public tests must exercise every major API item. The rubiks gap shows that weak public tests make B, C and D submit public-green failures and stop D early. This is now a rule in the P1 plan.

### E03 — calibration headroom pilot (A, B)  [phase P1]
- Registered: 2026-10-14 (frozen paths at commit 2eefa0d; suite committed in the following commit)
- Hypothesis: the calibration suite has headroom for the lift test at the P2 budget. Gate, pooled over all tasks: A (one-shot) passes 20–70% and B (best-of-5) passes at most 85%. Per family the same rates are reported; a family where A passes all 4 levels is flagged as saturated, and one where B passes none as floor.
- Arms / configuration: A, B; k = 5 (the P2 budget, k ≡ 1 mod 4); seed 3; jobs 4. **C and D are deliberately not run**, so the headroom and redesign decisions stay blind to C.
- Task set: calibration, `tasks/calibration` (suite hash ecd129fa0ca10aca51bc4bd2eaf95e91156005ba882abed0daba110205f57405), n = 24, families = 6
- Model(s): `codex/gpt-6-luna`, reasoning effort low, no-tools lane, provider timeout 300 s     k = 5     replicates = 1
- Budget cap: 170 invocations (planned 144), 5400 s wall-clock
- Infrastructure exceptions (predefined): a tuple with any non-model provider error in either arm (timeout, rate limit, non-zero exit) is excluded from the headroom computation and counted. If more than 20% of tuples are excluded, those tuples are rerun as a new experiment; that is not a redesign. Controller-voided tuples are excluded.
- Result-class rule: the headroom gate above (pass/fail). No lift classification.
- Redesign rule (decided before seeing results): if the pooled gate fails, redesign the family *templates* (harder levels if saturated, easier if at the floor) and re-author every family. That counts as T7 redesign 1 of 2. Never weaken public tests to pull B down, and never drop or select individual tasks.
- If the gate passes: E04 runs all four arms on this suite (A and B again as a second replicate) to fix the primary comparator and estimate discordance for power planning.
- Result: **gate FAILED (saturated).**
  - Pooled: A 19/24 (79%, outside 20–70%); B 21/24 (87.5%, above 85%). B vs A lift 0.083, CI [0, 0.25], discordant 2/0.
  - Per family:
    - saturated (A 4/4, B 4/4): dep-resolve, lru-repair, rate-sched, txn-kv;
    - semver: A 3/4, B 3/4 (l4 failed);
    - stream-b64: A 0/4, B 2/4 (byte-exact error offsets and canonical-encoding rules).
  - Public-green but hidden-fail: A 3, B 3.
  - Run integrity: complete; 0 provider errors; 0 tool calls; 0 hidden-name hits; ledger verified (243 records, head 5e1265bf…).
  - Cost: 144 invocations, 728k reported tokens, 2568 s total provider time.
- Artifacts: runs/2026-10-14-e03-calibration-headroom/ (ledger: runs/2026-10-14-e03-calibration-headroom/ledger.jsonl)
- Deviations: none. No tuples were excluded.
- Decision: per the pre-registered rule, this is **T7 suite redesign 1 of 2**, a template redesign.
  - Re-author all six families with harder level ladders: v1's l4 or l3 becomes v2's l1, and each level adds interacting clauses (error precedence, byte or format exactness, cross-feature state).
  - The new suite goes in `tasks/calibration-v2`; the v1 suite is burnt.
  - Model and effort are unchanged.

### E04 — calibration-v2 headroom pilot (A, B)  [phase P1]
- Registered: 2026-10-14 (frozen paths at commit 93dc025)
- Hypothesis: the v2 templates give headroom. Same gate as E03: pooled A in 20–70% and B ≤ 85%; per-family rates reported, with saturation and floor flags.
- Arms / configuration: A, B; k = 5; seed 4; jobs 4. C and D are not run (blind to C).
- Task set: calibration, `tasks/calibration-v2` (suite hash 067b4a6250521d173ceb3d9a42e191ff43878774f7d2c50bed0023e3749a3458), n = 24, families = 6
- Model(s): `codex/gpt-6-luna`, reasoning effort low, no-tools lane, provider timeout 300 s     k = 5     replicates = 1
- Budget cap: 170 invocations (planned 144), 7200 s wall-clock
- Infrastructure exceptions (predefined): same as E03. Tuples with any non-model provider error are excluded and counted; above 20% excluded, they are rerun.
- Result-class rule: the headroom gate (pass/fail).
- Redesign rule (decided before seeing results). If the gate fails, this is **T7 redesign 2 of 2, the last**, and it changes the model instead of the suite: keep `tasks/calibration-v2`.
  - If saturated (A > 70% or B > 85%), use coder `codex/gpt-5.6-luna`, effort low.
  - If at the floor (A < 20%), use `codex/gpt-6-luna`, effort medium.
  - The redesigned configuration gets one more headroom pilot. If that also fails, P1 proceeds with the closest configuration and the deviation is recorded; there is no third redesign.
- If the gate passes: E05 runs all four arms on this suite (A and B as a second replicate) to fix the primary comparator and estimate discordance for power planning.
- Result: **gate PASSED.**
  - Pooled: A 15/24 (62.5%), B 14/24 (58.3%). B vs A lift −0.042, CI [−0.21, 0.21], discordant 2/3.
  - Per family (A / B):
    - dep-resolve 1/4, 3/4
    - lru-repair 4/4, 4/4 (**saturated**)
    - rate-sched 2/4, 1/4
    - semver 2/4, 1/4
    - stream-b64 2/4, 2/4
    - txn-kv 4/4, 3/4 (A saturated)
  - Public-green but hidden-fail: A 6, B 9. The public tests discriminate weakly, so best-of-5 selection does no better than one-shot.
  - Run integrity: complete; 0 provider errors; 0 tool calls; 0 hidden-name hits; ledger verified (243 records, head 06e69bd2…).
  - Cost: 144 invocations, 1.33M reported tokens, 3696 s total provider time (about 23 s per call).
- Artifacts: runs/2026-10-14-e04-calibration-v2-headroom/ (ledger: runs/2026-10-14-e04-calibration-v2-headroom/ledger.jsonl)
- Deviations: none.
- Decision:
  - The v2 template is accepted for the confirmation sets (Codex is authoring them blind into `$A2D_HOLDOUT_DIR`). No further redesign; T7 stands at 1 of 2.
  - Proceed to E05, all four arms. Saturated families stay in, because the gate is pooled and tasks are never selected individually.
  - Implication for P2: weak public tests cap what B, D and C can gain from public signals. That is the regime the lift test measures, and it is reported, not corrected.

### E05 — four-arm pilot on calibration-v2  [phase P1 → P2 pilot]
- Registered: 2026-10-14 (frozen paths at commit 93dc025)
- Purpose:
  1. Fix the **primary comparator** for P2: whichever of B and D has the higher pooled hidden pass rate in E05, with ties going to D, the feedback baseline closest to A²D. The `--primary B` flag in this run is nominal.
  2. Estimate C-vs-comparator discordance for power planning, in families.
  3. Exercise C live on non-saturated tasks.
- Arms / configuration: A, B, D, C; k = 5 (C reaches 2 coder calls); seed 5; jobs 4. A and B are a second replicate of E04's configuration.
- Task set: calibration, `tasks/calibration-v2` (suite hash 067b4a6250521d173ceb3d9a42e191ff43878774f7d2c50bed0023e3749a3458), n = 24, families = 6
- Model(s): `codex/gpt-6-luna`, reasoning effort low, no-tools lane, provider timeout 300 s     k = 5     replicates = 1
- Budget cap: 420 invocations (planned ≤ 384), 7200 s wall-clock
- Infrastructure exceptions (predefined): same as E03/E04.
- Result-class rule: descriptive (pilot). 6 families is below the classification floor.
- Result: **descriptive pilot.**
  - Hidden pass rates: A 14/24 (58.3%), B 12/24 (50.0%), D 15/24 (62.5%), C 16/24 (66.7%).
  - C vs D: +4.2 pp, CI [−12.5, +25], discordant 3/2, family sign-flip p = 1.0.
  - C vs B: +16.7 pp, CI [+4.2, +33.3], discordant 4/0, p = 0.25.
  - D vs B: +12.5 pp, CI [+4.2, +20.8].
  - All classes are Inconclusive: 6 families is below the floor.
  - Public-green but hidden-fail: A 2, B 12, D 9, C 5.
  - Arm C's realised schedule was coder → evolver → architect → tester → coder on all 24 tasks, so 2 code attempts each.
  - Cost:
    - invocations: A 24, B 120, D 36 (stops at public green), C 120;
    - tokens: A 196k, B 960k, D 405k, C 1.18M;
    - provider time: A 785 s, B 3839 s, D 1506 s, C 3201 s.
  - Run integrity: complete; 0 provider errors; 0 tool calls; 0 hidden-name hits; ledger verified (519 records, head 165242e8…).
  - Replicate agreement with E04 (same configuration, same tasks): A 19/24, B 22/24. Outcomes are mostly determined by the task.
- Artifacts: runs/2026-10-14-e05-four-arm-pilot/ (ledger: runs/2026-10-14-e05-four-arm-pilot/ledger.jsonl)
- Deviations: 2026-10-14 — for about the first 45 minutes, E05 shared the Codex account with six holdout-authoring jobs. Mean Luna call time was about 65 s (23 s in E04), but there were no provider errors or timeouts, so nothing was excluded. The authoring driver was paused for the rest of the run.
- Decision:
  - **Primary comparator for P2: D.** By the pre-registered rule, D's 62.5% beats B's 50%.
  - **Power plan, C vs D.** Pilot discordance pd = 0.21 (p10 0.125, p01 0.083). 80% power at 10 pp needs about 162 independent pairs, or 210–307 with a family design effect (ρ 0.1–0.3, 4 tasks per family). Replicates add little (79–92% agreement), so power must come from tasks.
  - **Confirmation-1 is set at the cap: 37 families × 4 = 148 tasks, 1 replicate.** That is about 75% power with no clustering, and less with it. The shortfall is recorded, not hidden; the allowed single sample extension remains available if the result is INCONCLUSIVE.
  - 25 more Codex-authored families are needed (12 exist). P2 stays at k = 5, the configuration reviewed at the P1 gate.

### E06 — C and D re-pilot with the latest-tie submission rule  [phase P1 → P2 pilot]
- Registered: 2026-10-14 (frozen paths at commit eeaa7e7, which includes the tie-break change 007c7c7)
- Purpose:
  1. Pilot the changed treatment (latest-tie submission rule).
  2. Measure C and D **replicate agreement** against E05 on the same 24 tasks, to set the Confirmation-1 replicate count.
  For C, E05 is compared under the new rule, recomputed offline (descriptive, done before this run): 17/24.
- Arms / configuration: C, D; k = 5; primary comparator D (C vs D only); seed 6; jobs 4.
- Task set: calibration, `tasks/calibration-v2` (suite hash 067b4a6250521d173ceb3d9a42e191ff43878774f7d2c50bed0023e3749a3458), n = 24, families = 6
- Model(s): `codex/gpt-6-luna`, reasoning effort low, no-tools lane, provider timeout 300 s     k = 5     replicates = 1
- Budget cap: 260 invocations (planned ≤ 240), 5400 s wall-clock. Holdout authoring is paused for the whole run (no concurrent Codex jobs).
- Infrastructure exceptions (predefined): as E03–E05; tuple exclusions are capped at 5%.
- **Decision rule, pre-registered.** Agreement is the per-arm share of tasks with the same hidden outcome as E05 (C: against E05 re-selected under the new rule; D: against E05 directly).
  - If the mean of C's and D's agreement is **≤ 0.85**, Confirmation-1 runs **r = 2** replicates.
  - Otherwise it runs r = 1, and the one allowed extension (a second replicate pass) stays in reserve.
- Result: **descriptive pilot.**
  - D 16/24 (66.7%), C 14/24 (58.3%). C vs D −8.3 pp, CI [−16.7, 0], discordant 0/2. Six families, so no class.
  - Hidden-pass fraction: D 0.96, C 0.87. Hidden compile failures: C 2, D 0.
  - Cost: D 31 invocations and 317k tokens; C 120 invocations (48 coder) and 1.29M tokens.
  - Public-green but hidden-fail: D 8, C 7.
  - **Replicate agreement:** C 19/24 = 0.79 (against E05 re-selected under the new rule), D 23/24 = 0.96, mean **0.875**.
  - Combined with E05, under the latest-tie rule C passed 17 and 14 against D's 15 and 16: no sign of lift.
  - Run integrity:
    - complete; 0 provider errors; 0 coder tool calls; ledger verified (274 records, head 2e779d61…);
    - **the integrity gate fired.** One hidden test name appeared in C's own response for lru-repair-l4. The coder named its unit test `clean_replacement_flushes_old_value_before_storing`, which contains the hidden name `clean_replacement_flushes_old_value`. The name appears in no prompt and not in the requirements, so this is a false positive from substring matching on responses.
    - The scan now checks prompts only and matches whole identifiers (commit after 63af96f). Review outcome: no leak.
- Artifacts: runs/2026-10-14-e06-cd-repilot/ (ledger: runs/2026-10-14-e06-cd-repilot/ledger.jsonl)
- Deviations: none. Authoring was paused for the whole run, which averaged 18.8 s per call.
- Decision: mean agreement 0.875 > 0.85, so by the pre-registered rule **Confirmation-1 runs r = 1**. The single allowed extension, a second replicate pass, stays in reserve. The submission tie-break is now piloted. Next: resume holdout authoring, then the independent-solve audit of Confirmation-1, then E07.

E05 descriptive diagnostic, computed after the tie-break commit 007c7c7:
- Per-attempt hidden passes: A 14; B attempts 10, 13, 12, 10, 14; C first attempt 13, second attempt 15.
- C pairs (first, second): both pass 10; first only 3; second only 5; neither 6.
- B's attempt 0 is within the pre-stated ±5-task noise band of A, so nothing is investigated.

### Holdouts — Confirmation-1 final  [phase P1]
- 2026-10-14. **Confirmation-1: 148 tasks, 37 families × 4 levels, suite hash `7bd1b669051c93c7aa6ad0cd9eb316384682a332aa3d981ba205464932f1b9dd`.** Authored by Codex `gpt-6.1-sol` (high) into `$A2D_HOLDOUT_DIR/confirmation-1`. The developing agent saw only task ids, validate verdicts, counts and hashes.
- Family names, the only content seen: csv-quote, glob-match, varint-codec, cron-next, text-wrap, graph-paths, json-pointer, bank-ledger, ini-config, rational-matrix, url-parse, line-diff, json-lexer, toml-subset, bitset-ops, indexed-heap, scc-graph, union-groups, rope-text, base32-codec, money-format, numeral-convert, sparse-matrix, life-rules, bowling-score, vending-fsm, http-headers, email-address, row-filter, template-engine, fs-sim, checksums, iso-duration, pretty-printer, bank-queue-sim, knapsack-dp, key-maze.
- Validation (`a2d lift validate`, final rules) accepts 148/148. Earlier rejections, all repaired blind:
  - rope-text ×4: a validator parsing bug, since fixed;
  - fs-sim-l3: a public-coverage gap, repaired by Codex;
  - template-engine ×3: tests used private reference items, repaired by Codex.
  One family (key-maze) was re-authored after an OpenAI capacity error.
- **Independent-solve audit (roadmap § P1 Oracles).** Codex `gpt-6.1-sol` (medium) solved each task blind from the requirements and public tests only, as arm D with k = 3 (`$A2D_HOLDOUT_DIR/audit/c1-run1`, `c1-km`; never experiment data).
  - **148/148 pass every hidden test on the first attempt** (148 invocations, 1.66M tokens).
  - 0 public-green but hidden-fail; 0 tool calls; 0 hidden-name hits; ledgers verified.
  - No adjudication was needed: every hidden test is consistent with an independently written solution.
  - Limitations: auditor and coder share a vendor (OpenAI). A strong model at medium effort solves every task, so these tasks test precise spec compliance by a weaker or low-effort coder; they do not test what is beyond reach for current frontier models.
