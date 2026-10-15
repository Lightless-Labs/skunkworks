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

### Holdouts — Confirmation-2  [phase P1]
- 2026-10-14. **Confirmation-2: 48 tasks, 12 families × 4 levels, suite hash `25435ab324375a0f79f0b3fdd8f07b6f94341f92d3bca23642d4b68b02957ad5`.** Authored blind by Codex `gpt-6.1-sol` (high) into `$A2D_HOLDOUT_DIR/confirmation-2`.
- Families: calc-parser, date-arith, elevator-sim, huffman-codec, interval-tree, inventory-alloc, log-store, markdown-inline, polynomial, regex-lite, trie-autocomplete, undo-editor.
- Validation: 48/48 accepted on the first pass.
- **Independent-solve audit:** Codex `gpt-6.1-sol` (medium), arm D, k = 3. **48/48 pass every hidden test** (49 invocations; one task needed a repair attempt). 0 public-green but hidden-fail; integrity OK.
- Reserved for the final P4 claim. Power planning for P4 happens at the P3 → P4 gate; any extension families are authored blind before any P4 result.
- **Deviation (2026-10-15): accidental exposure of one extension family's requirements.**
  - While stopping E08, I listed processes with full command lines. A Codex `exec` call of the in-flight AUDIT-C2EXT run carries its task prompt in argv, so about 2 KB of one extension task's requirements (family `spreadsheet-eval`, clauses R1–R4) were printed into my context.
  - Hidden tests were not exposed. The saved full output was deleted unread.
  - **Response, fixed before any Confirmation-2 scoring and before any P4 harness is written: family `spreadsheet-eval` is excluded from Confirmation-2.** Its tasks are removed from the set before the final hash.
  - Rule added to `docs/PROTOCOL.md`: never print process command lines while a holdout or confirmation job runs.
- **Extension and final set (2026-10-15).** 25 families were authored blind by Codex `gpt-6.1-sol`, using the same pipeline as Confirmation-1.
  - Independent-solve audit (`AUDIT-C2EXT`: Sol, arm D, k = 3, the 100 extension tasks): **97/100**, integrity OK, 0 void.
  - The 3 failures (`calendar-recurrence` l1–l3) went to blind adjudication: `tests_determined=0 tests_underdetermined=3 tasks_changed=3`. The adjudicator repairs tasks and never drops them. Re-audit (`AUDIT-C2ADJ`): 3/3.
  - `spreadsheet-eval` was excluded (exposure deviation above).
  - **Final Confirmation-2: 36 families × 4 = 144 tasks. `a2d lift validate`: 144/144 accepted. Suite hash `cb7cb49655654169505811a5503b165a3d80c308fcbc2362e2108729c3d907a4`.**
  - Families: base58check, bipartite-match, calc-parser, calendar-recurrence, circuit-breaker, config-merge, consistent-hash, date-arith, edit-distance-align, elevator-sim, fen-parser, flood-fill-regions, huffman-codec, interval-tree, inventory-alloc, ipv4-cidr, ledger-reconcile, log-store, lz77-codec, markdown-inline, nonogram-check, order-book, parking-lot, polynomial, regex-lite, rle-bitmap, seat-booking, skip-list-map, tax-brackets, time-zone-offset, traffic-light-fsm, trie-autocomplete, undo-editor, unit-convert, version-vector, word-freq.
  - **Power** (`docs/experiments/analysis/c2_power.py`, which simulates the implemented rule with discordance calibrated to 0.26):
    - P(SUPPORTED | true +15 pp) = 0.86–0.90 at r = 1;
    - P(RULED OUT | true 0) = 0.62 at r = 1 and 0.75 at r = 2, below the 0.8 target.
    - Arm-specific variance sits at task level and the set is near the authoring cap. The shortfall is recorded, not hidden.

### E07 — P2 seed lift confirmation on Confirmation-1  [phase P2]
- Registered: 2026-10-15 (frozen paths at commit e1915c89). Since E06 (eeaa7e7) only controller and validator files have changed: `crates/a2d-lift/src/{lib,tests,validate}.rs`. The arm-C treatment is the one piloted in E06.
- Hypothesis: the seed A²D metabolism (arm C) passes hidden tests more often than its own coder used as a plain repair loop (D) **and** than best-of-k (B), at an **equal invocation cap** (k = 5).
- Claim rule (roadmap Addenda of 2026-10-14):
  - Primary comparison C vs D; secondary C vs B.
  - Overall **SUPPORTED** needs both to be SUPPORTED: 95% paired cluster-bootstrap CI lower bound > 0 **and** point lift ≥ 10 pp, with at least 10 families and at least one discordant pair.
  - **RULED OUT** (CI upper < 10 pp) is decided by C vs D alone.
  - Otherwise **INCONCLUSIVE**. The single allowed extension is a second replicate pass, analysed pooled at the family level.
- Arms / configuration: A, B, D, C; k = 5; replicates r = 1 (E06 rule); `--primary D`; seed 7; jobs 4; tuples in seeded random order; arms in a seeded per-tuple order.
- Task set: **Confirmation-1**, `$A2D_HOLDOUT_DIR/confirmation-1` (suite hash 7bd1b669051c93c7aa6ad0cd9eb316384682a332aa3d981ba205464932f1b9dd), n = 148, families = 37
- Model(s): `codex/gpt-6-luna`, reasoning effort low, no-tools lane, provider timeout 300 s     k = 5     replicates = 1
- Budget cap: 2,400 invocations (planned max 148 × 16 = 2,368), 21,600 s wall-clock (about 2× expected). No other Codex jobs run on the account during generation.
- Infrastructure exceptions (predefined):
  - Provider errors and timeouts count as arm failures.
  - Voided or incomplete tuples are excluded. If more than 5% are excluded, the run is Invalid and is rerun as a new registration.
  - Hidden-name hits in prompts or any coder tool call → IntegrityReview before any class is reported.
- Protocol:
  1. Run `--defer-scoring --out $A2D_HOLDOUT_DIR/runs/E07`.
  2. `a2d lift manifest` → `runs/2026-10-15-e07-confirmation-1/manifest.json`; commit and push.
  3. `a2d lift score --expect-head <manifest ledger_head>`.
  4. Copy `summary.json` and the content-free ledger into the repo.
  5. Scored once.
- Reported beside the class:
  - per-arm invocations, tokens and provider time;
  - hidden-pass fraction (secondary);
  - public-green but hidden-fail counts;
  - family sign-flip p;
  - descriptive McNemar.
- Result: **INCONCLUSIVE** (scored once on 2026-10-15 against the committed anchor a235a0b; ledger chain verified, final head 5e0f5151…).
  - Run: complete, 148/148 tuples, 0 void, 0 provider errors, 0 coder tool calls, 0 hidden-name hits, 0 suspicious submissions.
  - Hidden pass rates:
    - A 86/148 (58.1%)
    - B 96 (64.9%)
    - D 98 (66.2%)
    - C 102 (68.9%)
  - **Primary, C vs D:** lift +2.7 pp, 95% CI [−4.7, +11.5], 37 families, discordant 20/16, family sign-flip p = 0.65, McNemar (descriptive) p = 0.62. Not SUPPORTED (the CI includes 0) and not RULED OUT (the upper bound exceeds 10 pp).
  - **Secondary, C vs B:** +4.1 pp, CI [−4.7, +12.8], discordant 23/17. Inconclusive.
  - Exploratory comparisons:
    - C vs A +10.8 pp, CI [+2.7, +18.9] (meets the SUPPORTED rule, but exploratory);
    - D vs A +8.1 pp, CI [+1.4, +15.5];
    - B vs A +6.8 pp, CI [−1.4, +14.2];
    - D vs B +1.4 pp.
  - Hidden-pass fraction (secondary): A 0.78, B 0.93, D 0.93, C 0.90. C vs D −3.0 pp: C's failures fail more hidden tests than D's.
  - Public-green but hidden-fail: A 33, B 49, D 47, C 38.
  - Spend at the equal invocation cap (k = 5):
    | Arm | Invocations | Tokens | Provider time |
    |---|---|---|---|
    | A | 148 | 1.04M | 3,804 s |
    | B | 740 | 5.03M | 19,478 s |
    | D | 197 | 1.60M | 5,440 s |
    | C | 740 (296 coder) | 5.97M | 15,058 s |
    C used 3.8× D's invocations and 3.7× its tokens for +2.7 pp.
- Artifacts: runs/2026-10-15-e07-confirmation-1/ (manifest.json committed pre-score, ledger.jsonl, summary.json); full run in `$A2D_HOLDOUT_DIR/runs/E07` (never opened)
- Deviations: 2026-10-15, at launch: the sibling A² session was running `gpt-6-luna` jobs on the same Codex account (about 1 process, its E1 run). That violates the pre-registered "no other Codex jobs" condition, but this session cannot control it and A²'s schedule is open-ended. Generation proceeded. Tuple and arm orders are randomised, so the contention is noise, not bias. Call durations and provider errors are monitored and reported with the result. Recorded before any submission was scored or inspected.
- Decision: INCONCLUSIVE, so the pre-registered response applies: run the single allowed extension, E07b (a second replicate pass over Confirmation-1, pooled at the family level). **P2's exit criterion (a result class is recorded) is met.** P3 code waits until E07b is scored, because the treatment must stay byte-identical for pooling. Finding regardless of class: C buys +2.7 pp over D with 3.8× the invocations and 3.7× the tokens.

### E07b — P2 sample extension: second replicate pass on Confirmation-1  [phase P2]
- Registered: 2026-10-15 (frozen paths at commit c4ec034a). Since E07's e1915c89, only `crates/a2d-lift` has changed: the `pool` command and TestScore deserialisation. The arm-C treatment, controller run path, providers and metabolism are byte-identical to E07's.
- Purpose: the roadmap's single allowed extension after E07's INCONCLUSIVE.
- Design: identical to E07 (A, B, D, C; k = 5; `--primary D`; `codex/gpt-6-luna`, effort low, no-tools lane; Confirmation-1, suite hash 7bd1b669051c93c7aa6ad0cd9eb316384682a332aa3d981ba205464932f1b9dd; min-families 10; max excluded share 5%). Only the seed differs: **seed 8**.
- Budget cap: 2,400 invocations, 21,600 s wall-clock.
- Protocol: as E07 (deferred scoring, manifest committed before `score --expect-head`).
- **Analysis, pre-registered:** `a2d lift pool --out <pooled summary> <E07 ledger> <E07b ledger>`. E07 becomes replicate 0 and E07b replicate 1, giving 296 tuples in 37 families. The registered claim rules apply to the pooled tuples: SUPPORTED needs the primary C-vs-D and secondary C-vs-B comparisons SUPPORTED; RULED OUT is decided by C vs D alone; the excluded-share rule applies to all 296.
- **The pooled class is final.** E07 alone is not re-classified, there is no third look, and an INCONCLUSIVE pooled result means NO-GO for adding complexity (roadmap § Metric).
- Infrastructure exceptions: as E07.
- Deviation expected at launch: the sibling A² session may share the Codex account, as in E07. This will be recorded if observed.
- Result, **FINAL (pooled E07 + E07b, as pre-registered): RULED OUT.** Each run was scored once against its committed anchor. The E07b ledger is verified (head ac83faa7…). Both runs are complete: 0 void, 0 errors, 0 coder tool calls, 0 hidden-name hits.
  - Pooled over 296 tuples in 37 families, hidden pass rates:
    - A 173/296 (58.4%)
    - B 194 (65.5%)
    - D 200 (67.6%)
    - C 204 (68.9%)
  - **Primary, C vs D:** lift **+1.4 pp, 95% CI [−5.4, +8.8]**, discordant 40/36, family sign-flip p = 0.79. The upper bound is below 10 pp, so **RULED OUT**.
  - **Secondary, C vs B:** +3.4 pp, CI [−2.7, +9.8], discordant 44/34. RULED OUT.
  - Exploratory comparisons:
    - C vs A +10.5 pp, CI [+4.1, +17.2];
    - D vs A +9.1 pp, CI [+3.7, +15.2];
    - B vs A +7.1 pp, CI [+2.0, +12.2];
    - D vs B +2.0 pp, CI [−3.7, +8.1].
  - Hidden-pass fraction: A 0.80, B 0.92, D 0.93, C 0.88. When C fails, it fails more hidden tests than D does.
  - Public-green but hidden-fail: A 71, B 95, D 88, C 71.
  - Pooled spend at the equal invocation cap:
    | Arm | Invocations | Tokens |
    |---|---|---|
    | A | 296 | 2.09M |
    | B | 1,480 | 10.17M |
    | D | 400 | 3.28M |
    | C | 1,480 | 12.10M |
    C used **3.7× D's invocations and tokens**.
  - E07b alone: A 87, B 98, D 102, C 102 of 148. C vs D 0.0 pp, CI [−8.1, +8.8].
- Sensitivity (descriptive, requested by the P2 gate review; the registered result stays primary; script `docs/experiments/analysis/p2_sensitivity.py`, with the grouping written before computing):
  - An independent Python re-implementation reproduces the registered interval exactly: +1.4 pp, [−5.4, +8.8].
  - **5 broad domain clusters** (codecs and parsers, text and format, data structures, graphs and algorithms, state/simulation/finance): CI [−5.8, +6.9]. Still RULED OUT.
  - **Leave-one-family-out:** the lift ranges from −0.7 pp (without iso-duration) to +2.4 pp (without life-rules). No single family drives the result.
  - **Two looks (Claude reviewer).** The result is pooled after an INCONCLUSIVE first look. The two-sided 97.5% interval (α/2 per look) is [−6.4, **+9.8**], so it is still RULED OUT, but narrowly. E07b alone, a fresh replicate, is also RULED OUT (upper bound +8.8).
  - **Self-test interference (Claude reviewer).**
    - C's prompt demands at least 3 self-tests, and the oracle compiles the candidate's own `#[cfg(test)]` code. A blind diagnostic compiled every hidden compile-failure without the hidden tests (`docs/experiments/analysis/selftest_diag.py`; no hidden tests used, counts only).
    - Pooled: of C's 7 compile failures, **5 are solutions that do not compile at all and 2 (E07b) are own self-tests breaking the test build**. D's single failure is an API mismatch with the hidden tests.
    - Worst case, both C artifacts pass: C 206 vs D 200, +2.0 pp. The 95% bound stays below 10. **Only if both adjustments are stacked** (two-look bound plus the worst-case artifact correction) does the bound reach about 10.5 pp. That is the edge of what the claim supports, and it is recorded rather than re-classified.
  - **Wording of the claim (adopted from the review):** under the registered family-bootstrap analysis, the frozen seed A²D harness did not deliver a 10 pp improvement over the public-test-stopping repair loop on these fresh Rust task families, using Luna-low, no tools, and an equal cap of five provider invocations per task. This does not establish zero benefit, equivalence, equal-cost performance, or the failure of an optimised harness or of autoresearch.
- Artifacts: runs/2026-10-15-e07b-confirmation-1-r2/ (manifest, ledger, summary, pooled summary)
- Deviations: 2026-10-15, at launch: the sibling A² session had 2–3 Codex processes on the account (its E1 workcells and one other job). As in E07, this cannot be controlled from this session. Tuple and arm orders are randomised. Recorded before any scoring.
- Decision: **P2 is complete. A lift of 10 pp or more for the seed A²D metabolism over its own coder in a plain repair loop, at an equal invocation cap, is ruled out on fresh families.** By the roadmap, this is the falsification half of "Completed (falsified)". The plan completes as falsified only if no P4 hypothesis is SUPPORTED within T7, so P3 and P4 proceed. They form the owner's autoresearch loop: outer-loop harness iteration on development data, then A²D's own inner loop, scored on Confirmation-2. The seed harness spends 3.7× the tokens for about +1 pp, so P3 starts by removing spend that cannot affect the submission.

### S0 — P3 Step 0: treatment audit and selection headroom (offline)  [phase P3]
- 2026-10-15. Offline, no live runs. Inputs: E06 (calibration, visible) and E07/E07b candidates on `tasks/c1-train` (development data since the P2-gate Addendum).
- **Treatment audit (E06, all 24 arm-C tuples):** the seed coder enzyme never receives its previous code. `code` is not a coder reactant or catalyst.
  - Its second call had **no new feedback at all in 15/24 tasks**: the first attempt was public-green, so `failure_report` stayed empty. That call was an independent resample under evolver-reworded instructions; its prompt differed from the first by 8 lines of the coder template.
  - In 9/24 it received a compiler or test diagnostic about code it could not see.
  - D always sees its previous file plus the public output.
  - This explains E05's coin-flip second attempts (fixed 5, broke 3) and C ≈ B-like behaviour. **The first fix in any A²D harness is to route the previous attempt to the coder.**
- **Selection headroom** (E07/E07b candidates on `tasks/c1-train`: 1,263 candidates, 68 tasks × 2 runs = 136 tuples; `docs/experiments/analysis/candidate_pool.py`):
  - The public-rule pick passes hidden in: A 63.2%, B 67.6%, **D 74.3%**, C 69.9%.
  - Any of the ~19 candidates passes hidden in **92.6%** of tuples.
  - **Within B's 5 candidates** (the realistic pool at k = 5), any-pass is **86.0%**. In 32.4% of tuples B's public-green candidates are mixed (some pass hidden, some fail).
  - P(hidden pass | public-green candidate) = 0.73.
  - **Conclusion:** a selector with a signal beyond public tests has up to about 12 pp of headroom over D within 5 candidates. Selection alone cannot reach +15 pp; generation must improve too.
- **Inertness** of the architect and tester in lift mode is certified by a unit test: their outputs never reach a coder prompt (`lift_arm_c::tests::architect_and_tester_outputs_never_reach_a_coder_prompt`).

### S1 — blind spec tests, offline falsification  [phase P3, development lane]
- Registered: 2026-10-15, before any call.
- Mechanism: a test-writer (`codex/gpt-6-luna`, effort low, no-tools lane, the harness's own model) sees only the numbered requirements and the public tests (for the API shape), never any implementation. It writes `#[test]` functions per clause. One call per `tasks/c1-train` task: **68 calls, cap 80**.
- Measurements, on the frozen S0 candidate pool:
  1. **Validity:** the share of generated tests that pass against `reference.rs`. Tests that do not compile against the reference are dropped and counted.
  2. **AUC:** the spec-test pass fraction as a predictor of hidden pass, among public-green candidates.
  3. **Recall:** the share of public-green but hidden-fail candidates that fail at least one reference-compiling spec test.
  4. End metric (offline): the hidden pass rate when re-selecting among B's 5 candidates by (public passed, spec passed, latest), against B's public pick (67.6%) and D (74.3%).
- **Kill rule:** validity < 0.85 **or** AUC < 0.65 **or** recall < 0.45 kills mechanism 1. In that case clause review becomes the primary mechanism.
- Data: development only. Outputs go to `runs/2026-10-15-s1-spec-tests/`.
- Result: **KILLED by the pre-registered rule** (validity and AUC both below threshold).
  - Generation: 68/68 calls, 2,120 tests, 721k tokens, 0 tool calls. 95% of tests compile against the reference (they are API-correct).
  - **Validity 0.831** (< 0.85): about 17% of Luna-written spec tests assert the wrong behaviour.
  - **AUC 0.630** (< 0.65), among 1,039 public-green candidates.
  - Recall 0.910 on public-green but hidden-fail candidates, but the false-alarm rate on hidden-pass candidates is **0.872**: one wrong test fails almost every candidate.
  - Not harness-realistic, for insight only: valid-tests-only AUC 0.758.
  - Offline end metric, re-selecting among B's 5 candidates by (public, spec, latest): **0.743**, against B's public pick 0.676 and D 0.743.
  - Artifacts: `runs/2026-10-15-s1-spec-tests/` (generation, tests, evaluation.json).
- Decision: mechanism 1 as specified is killed. The signal exists but is swamped by wrong tests. The one allowed train-only revision is S1b (consensus filtering). If S1b also dies, clause review becomes primary.

### S1b — consensus-filtered spec tests (the one train-only revision of mechanism 1)  [phase P3, development lane]
- Registered: 2026-10-15, before computing. Zero new invocations: it reuses S1's tests and the S0 pool.
- Mechanism: harness-realistic, no reference needed. Per tuple, among the candidates the harness would hold (B's 5), **drop every spec test that fails on more than half of the public-green candidates** (treated as likely wrong expectations). Select by (public passed, retained-spec pass fraction, compiled, latest).
- Measurements on the same pool:
  1. validity of the *retained* tests against the reference;
  2. AUC of the retained-spec pass fraction among public-green candidates;
  3. recall and false-alarm rate;
  4. end: the selection pick rate against B's public pick and D.
- **Kill rule** (same thresholds): retained validity < 0.85 **or** AUC < 0.65 kills mechanism 1 for P3. Separate promotion bar for the end metric: the offline pick must reach ≥ D + 5 pp (≥ 0.793) before any live harness is built around it.
- Result: **KILLED** (AUC 0.637 < 0.65; zero new invocations).
  - Consensus filtering keeps only valid tests (retained validity 1.000) with a false-alarm rate of 0.002, but recall falls to 0.275.
  - Offline end metric, the consensus-spec pick over B's 5: 0.728, below D (0.743) and below the 0.793 promotion bar.
- Decision: **mechanism 1 (blind spec tests) is dead for P3.** One reusable fact: consensus-filtered spec tests are a high-precision, low-recall detector of public-green failures (27.5% caught, 0.2% false alarm). Clause review becomes the primary mechanism (S2).



### S2 — clause-by-clause review-and-fix after green, offline  [phase P3, development lane]
- Registered: 2026-10-15, before any call.
- Mechanism: after D reaches public-green, one review call. `codex/gpt-6-luna`, effort low, no-tools lane. The reviewer sees the numbered requirements, the public tests and the current file. For each clause it marks OK or GAP, citing the implementing code, then emits the complete corrected file. **If the corrected file breaks a public test, the original is kept.**
- Sample: every public-green D pick on `tasks/c1-train` from the S0 pool, **133 candidates** (32 hidden-fail, 101 hidden-pass). One call each, **cap 150**.
- Measurements:
  - conv: the share of hidden-fail picks that pass hidden after review;
  - regr: the share of hidden-pass picks that fail after review;
  - end: D + review pick rate against D (0.743 on these 136 tuples).
  - Break-even for +10 pp: conv × 32 − regr × 101 ≥ 13.6, i.e. conv ≥ 0.43 at regr = 0.
- **Kill rule:** conv < 0.25 **or** regr > 0.05 (more than 5 of 101 broken). **Promotion bar** (before building a live harness around it): end ≥ D + 5 pp (≥ 0.793).
- Result: **KILLED** (conv 0.188 < 0.25).
  - 133 calls, 1.38M tokens, 0 tool calls. Code changed in 82 of 133 reviews; 10 fixes were reverted because they broke public tests or returned no code.
  - **conv 6/32 = 0.188, regr 3/101 = 0.030.**
  - End: D + review 104/136 = 0.765 against D 0.743 (+2.2 pp), below the 0.793 bar.
  - Artifacts: `runs/2026-10-15-s2-clause-review/`.
- Decision: clause self-review by the low-effort coder model finds too few real gaps. Mechanism 2 is dead for P3 unless a train-only revision changes the signal. The next mechanism is selection by agreement (S3), which uses the 86%-any-pass headroom within 5 candidates.


### S3 — dual-agreement selection (CodeT-style) at the k = 5 budget, offline  [phase P3, development lane]
- Registered: 2026-10-15, before computing. Zero new invocations: it reuses S1's Luna spec tests (API-correct subset, unfiltered) and S0's B candidates.
- Mechanism, budget-faithful: 1 test-writer call plus **4 coder samples** (B's first four attempts per tuple). Group candidates with code by the exact set of spec tests they pass. Group score = (candidates in the group) × (spec tests the group passes). Restrict to public-green candidates when any exist. Pick from the top-scoring group, breaking ties on most public passes, then compiled, then latest.
- Measurement: end pick rate (hidden pass) over the 136 train tuples, against D (0.743) and B's public pick.
- **Promotion bar = kill rule:** end ≥ D + 5 pp (≥ 0.793), otherwise mechanism 3 is dead for P3. Also reported: the same rule over all 5 of B's candidates (not budget-faithful; for insight).
- Result: **KILLED** (end 0.728 < 0.793).
  - Budget-faithful (B's first 4 candidates): CodeT pick 0.728; the public pick on the same 4 is 0.699.
  - Insight (all 5 of B): CodeT pick 0.721; public pick 0.676.
  - D = 0.743.
- Decision: agreement on the coder's own spec tests does not recover the any-pass headroom. Mechanisms 1–3 are dead offline (about 200 invocations in total).
  - Pattern: every signal the low-effort coder generates about its own code is too noisy to beat a plain repair loop. Spec tests are 83% valid; self-review converts 19% of failures; agreement selection reaches 73%.
  - Diversity across samples is real (86% any-pass in 5), but this model cannot tell which sample is right.


### E08 — H0 live check: D vs C(H0) on c1-train  [phase P3, development lane]
- Registered: 2026-10-15, one-row development pre-registration.
- H0 = `harnesses/h0.json`: the seed minus the architect and tester, **previous-code routing** (the S0 fix), and a cleaned coder prompt (no self-test demand, no GitHub boilerplate, a clause check before coding). At k = 5 the expected schedule is coder → evolver → coder → evolver → coder.
- Prompt hygiene is folded into H0 as a floor change, not as a P3 mechanism: a prompt gain would lift every arm.
- Arms D and C(H0); k = 5; `tasks/c1-train` (68 tasks); `--unregistered`; seed 9; jobs 4.
- Expectation (**sanity, not a claim**): C(H0) within ±5 pp of D. If C(H0) is much lower, the routing did not take and P4 has no baseline.
- Budget: ≤ 450 invocations, 7,200 s.
- P3 validation looks used so far: 0.
- Result: **ABORTED at 23 of 68 C tuples** (159 invocations; no hidden scores computed). Artefacts: `runs/2026-10-15-e08-h0-check-aborted/`.
  - **H0 defect.** The cleaned coder prompt dropped the seed's "contain a main() function" clause. The metabolism's internal sandbox compiles the file as a *program* before it runs tests.
  - So the first attempt of almost every task failed with a spurious `E0601: main function not found` (24 coder prompts), the second call was spent adding `fn main`, and the public-test output of the first attempt never reached the coder.
  - Public-green so far: C 15/19, D 19/20. In E05/E06 the seed coder included `main` in 48 of 48 submissions, so P2's C was not affected.
  - **Fix:** H0 v2 adds "Include an empty `fn main() {}`"; `validate_harness` now rejects a `coder_prompt` without `main()`, so a P4 proposer cannot reintroduce the defect. Tests: `coder_prompt_must_keep_the_sandbox_main_contract`, `metabolism_benchmark_requires_a_main_function`.
  - Lesson: a harness slot is coupled to implicit contracts of the machinery behind it, so check a new harness's first feedback prompts before trusting a run (see E08r).

### E08r — H0 v2 live check: D vs C(H0 v2) on c1-train  [phase P3, development lane]
- Registered: 2026-10-15, one-row development pre-registration. This re-runs E08 with the defect fixed; everything else is identical.
- H0 v2 = `harnesses/h0.json` (sha256 `867a616b01efc5472488678dc85d62b099d0bf4134ce3d8a09c1acf640cecfa6`).
- Arms D and C(H0 v2); k = 5; `tasks/c1-train` (68 tasks); `--unregistered`; seed 9; jobs 4. Out: `runs/2026-10-15-e08r-h0-check`.
- **Early check (before the run completes):** the first five C tasks' second coder prompts must carry public-test output or an empty failure report, with no `E0601`. Otherwise abort.
- Expectation (**sanity, not a claim**): C(H0 v2) within ±5 pp of D. If C is much lower, H0 is not a usable P4 baseline.
- Budget: ≤ 450 invocations, 7,200 s. P3 spend so far, including E08: about 360 invocations.
- Result (`runs/2026-10-15-e08r-h0-check/summary.json`; 0 void, integrity OK):
  - The early check passed: 58 later C coder prompts, with no `E0601`.
  - **D 51/68 = 75.0%; C(H0 v2) 53/68 = 77.9%.** Lift +2.9 pp, 95% CI [−11.8, +19.1], discordant pairs 10 vs 8. **Sanity met (within ±5 pp).**
  - Spend: C 340 invocations / 3.38M tokens vs D 80 / 0.60M (4.25× invocations, 5.7× tokens).
  - Public-green but hidden-fail: C 12, D 17. In 53 of 58 later prompts the C coder rewrites an already-green file, which is S2's post-green self-review in vivo (S2 offline: +2.2 pp).
  - **H0 for P4 = H0 v2 + `stop_at_public_green`** (H0 v3), so that it is structurally D plus routing. The post-green rewrite becomes an explicit P4 lever, not a baseline cost. H0 v3 itself is not live-checked; S4 runs it with lessons added.

### T0 — failure taxonomy of D's public-green, hidden-fail picks on c1-train  [P3 → P4 groundwork, development lane, descriptive]
- Registered: 2026-10-15, one-row development pre-registration.
- Cases: D's pick per tuple in E07 and E07b on the 17 `c1-train` families: **32 public-green, hidden-fail picks** (11 families). Train is visible development data, so hidden tests are shown.
- Method: Codex `gpt-6.1-sol` (medium), one call per case, run in an isolated staged dir containing the spec, the candidate, the hidden tests and the failing-test output.
  - Output per case: violated clauses, a category from a fixed list, whether the spec was clear, whether a careful clause-by-clause check would have caught it, and a **task-agnostic lesson** (≤ 25 words) or null.
- Decision use (fixed now): if ≥ 50% of cases share ≤ 3 categories **and** ≥ 50% have a task-agnostic lesson, P4's first lever is a cross-task `lessons` slot fed from train failures. Otherwise lessons are deprioritised, and P4 targets selection and schedule.
- Budget: ≤ 40 Sol calls.
- Result (32 Sol calls; `runs/2026-10-15-t0-taxonomy/t0-results.json`):
  - Categories: boundary/edge case 9, unit or offset counting 6, error precedence 5, other 4, numeric overflow 3, state/sequencing 2, and 1 each for format, timeout and missing requirement. **The top 3 cover 20/32 = 0.62.**
  - **Task-agnostic lesson for 32/32.** The rule is met, so P4's first lever is a cross-task `lessons` slot.
  - The lessons recur across families:
    1. validate in the specified order, splitting structure before validating fields (cron, url);
    2. compute error offsets in the original input's coordinates (cron, email, fs, text);
    3. never allocate or loop from an unchecked input count or dimension, and short-circuit empty work first (varint, rational-matrix);
    4. detect overflow while accumulating, not after (rational-matrix, scc);
    5. bound worst-case search when proving no result exists (cron).
  - **Caveats.**
    - Sol rated every spec clear and every failure catchable by a careful clause check (32/32). That is hindsight from seeing the failing test: S2 measured that the coder's own clause review converts 0.19.
    - The lesson criterion was weak (any lesson counts).
    - The lessons partly encode the authoring style of the hidden tests (extreme counts, precedence, absolute offsets). Confirmation-2 shares the author (Sol), so any transfer holds within one author's task distribution, and is reported as such.
  - So lessons must earn promotion **empirically, on families they were not distilled from**.

### S4 — oracle-lessons ceiling on c1-validation  [phase P3, development lane, validation look 1 of 4]
- Registered: 2026-10-15, one-row development pre-registration. **This is a ceiling experiment, not a promotable mechanism.**
  - The lessons were distilled by the researcher from T0, and Sol wrote T0 while looking at train hidden tests.
  - They measure what a cross-task `lessons` slot could achieve at best. P4 asks whether A²D can write such lessons itself from train labels alone.
  - Whether a researcher-lessons configuration may enter the Confirmation-2 run as a separate, disclosed arm is a question for the P3 → P4 gate reviewers, not for this entry.
- Harness: `harnesses/s4-oracle-lessons.json` (sha256 `c26774f0c7c3699c285601cd1649d0e4ee3bea8c799a10f7db7861116d56c9e3`). It is H0 v2 plus `stop_at_public_green` plus a `coder_note` of 8 task-agnostic lessons (no family names) condensed from T0.
- Data: `$A2D_HOLDOUT_DIR/c1-validation` (80 tasks, 20 families; none is a family T0 drew from). Metric only: `summary.json` aggregates are read; per-task and per-family results are not.
- Arms D and C(S4); k = 5; `--primary D`; seed 11; jobs 4. Out: `$A2D_HOLDOUT_DIR/runs/S4`; the summary is copied to `runs/2026-10-15-s4-oracle-lessons/`.
- **Decision rule (fixed now):**
  - C(S4) − D ≥ +5 pp on validation: the lessons lever has room, and P4 builds the self-written `lessons` slot first.
  - Below +5 pp: the ceiling is too low, the lessons slot is deprioritised in P4, and the post-green and selection levers are not revisited (S1–S3 killed them).
  - Promotion (≥ +12 pp with a paired lower bound > 0) does not apply to a ceiling arm.
- Budget: ≤ 500 invocations, 7,200 s. Validation looks used before this one: 0.
- Result (`runs/2026-10-15-s4-oracle-lessons/summary.json`; aggregates only; 0 void, integrity OK):
  - **D 52/80 = 65.0%; C(S4) 54/80 = 67.5%.** Lift +2.5 pp, 95% CI [−8.8, +13.8], discordant pairs 10 vs 8, 20 families.
  - **Below +5 pp, so by the rule the lessons lever is deprioritised.** Validation looks used: 1 of 4.
  - Spend: C 138 invocations / 1.88M tokens vs D 107 / 0.90M. Stop-at-green is verified: 1.73 invocations and 1.36 coder calls per task (corrected 2026-10-15 at the P3 gate review; earlier text said 1.38 invocations).
  - Arm-level breakdown (an observation, not a reinterpretation of the rule):
    - public-green but hidden-fail: C 18 vs D 26;
    - never public-green: C 8 vs D 2;
    - hidden pass among green submissions: C 0.75 vs D 0.67.
  - Diagnosis, from two checks on aggregate counts:
    1. **The budget explanation is refuted.** D's first public-green attempt on train (136 tuples) is 1: 111, 2: 18, 3: 3, 4: 1 and never: 3. So H0's evolver taking 2 of 5 calls cannot explain the gap.
    2. **A mechanical rendering defect.** The seed copies the task input into `requirements`, `design` and `plan`. Every C coder prompt therefore carries the spec, public tests and lessons note **three times**: the note appeared 3× in 109/109 prompts, and the mean prompt was 43K characters vs D's 15K. E08r (H0 v2, no lessons) already showed C non-green 3 vs D 0.
  - Fix: harness key `dedupe_input` (unit test: the spec appears once per coder prompt). **H0 v4 = H0 v3 + `dedupe_input`** (`harnesses/h0-v4.json`, sha256 `d7e4b4fa…`).

### S4b — lessons rendered once, on clean train (the one train-only revision of S4)  [phase P3, development lane]
- Registered: 2026-10-15, one-row development pre-registration. This is P3's permitted train-only revision of S4 (mechanism 5 of 5). Its content follows the S4 diagnosis: the task is rendered once, and there are fewer lessons.
- Harnesses:
  - H0 v4 (`harnesses/h0-v4.json`, sha256 `d7e4b4fa60bab0313a950551b6ce1bd5fdee0d383ce859855a60b5899dc672f6`);
  - H0 v4 + **4 lessons** through the `lessons` key (`harnesses/s4b-lessons.json`, sha256 `424a1d4a87a853a9a0a445e31ce4987c4782f00b3e281f79471b47bcd13bf827`), covering precedence, coordinates, unchecked sizes and checked arithmetic.
- Data: `tasks/train-clean` = dev-v2 + calibration-v2 (72 tasks, 18 families; suite hash `5cc699991cd99c026348233bb9b4c641e73e79985a1aac5d841df906465fe7d7`). T0 drew only from c1-train, so these families are clean for the lessons. There is no validation look.
- Runs, launched together, seed 17, k = 5, jobs 4, `--unregistered`:
  - **S4b-A:** arms D and C(H0 v4), out `runs/2026-10-15-s4b-a`;
  - **S4b-B:** arm C(H0 v4 + lessons), out `runs/2026-10-15-s4b-b`.
  - Comparisons: `a2d lift compare` across runs, paired by task.
- **Primary: C(lessons) vs C(H0 v4).** **Kill if the lift is below +5 pp.**
  - conv and regr are reported against the resampling null: two D runs on c1-train (E07 vs E07b) give null conv 0.294, null regr 0.118 and flip rate 0.162.
- **Secondary (H0 v4 sanity):** C(H0 v4) vs D within ±5 pp. If C(H0 v4) is below D − 5 pp, H0 still has a defect.
- Use (fixed now):
  - **If S4b survives**, P4 proceeds with the self-written lessons slot (`a2d distill`) from H0 v4, subject to the P3 → P4 gate.
  - **If S4b is killed**, no P3 or P4 lever has evidence behind it. P3 exits "not promoted", and the gate considers the roadmap's falsified path, with P4-0 (label-only diagnosis works) recorded as the open finding.
- Budget: ≤ 450 invocations (A) + ≤ 300 (B), 7,200 s each. P3 spend before this: about 1,100.
- **Clarification (11:02 UTC, 2026-10-15, while S4b was generating, before any S4b hidden score existed or was read; from the P3 gate review by Codex Sol). This clarifies the rule and does not relax it. The original rule stands beside it.**
  - On 72 complete tasks, +5 pp means **at least 4 net additional passes**.
  - Survival authorises **P4 development only**. P3's promotion bar to Confirmation-2 stays unmet.
  - C(H0 v4) below D − 5 pp is a **performance warning**. It invalidates or repeats nothing unless a defect is independently demonstrated.
  - The paired family interval, discordance, conv and regr are reported descriptively. None is an alternative way to pass.
  - Under an independent symmetric null with flip rate 0.162, a net gain of ≥ 4 on 72 occurs about 15% of the time. This is a screening rule, not evidence of improvement.
- **Amendment (11:14 UTC, 2026-10-15; from the fresh Claude gate reviewer). It is strictly stricter, and the original kill threshold is unchanged.**
  - Timing: S4b-B's hidden scores existed on disk, unread. S4b-A was still generating, so the primary comparison did not yet exist. No S4b result had been seen.
  - Net gain of C(lessons) over C(H0 v4) on 72 tasks:
    - **≤ 3:** kill, as originally registered;
    - **≥ 8:** survive;
    - **4–7:** one pooled replicate of both C arms with a new seed (about 330 invocations), surviving only if the pooled lift is ≥ +5 pp on 144 tuples.
  - **Binding: C(lessons) − D ≥ 0** (it was descriptive).
  - **If C(H0 v4) < D − 5 pp,** check the S4b-A ledger and prompts for E0601, hidden compile failures, and evolver rewrites of the coder template mid-task. S4b is void and rerun once only if a defect is demonstrated. Otherwise the result stands.
  - Survival authorises P4 spend only and supports no claim.
  - The evolver's call share is reported beside every C result.
- Result: pending


### P4-0 — label-only bug diagnosis probe (Luna-low)  [P4 groundwork, development lane, descriptive]
- Registered: 2026-10-15, one-row development pre-registration.
- Question: can A²D's own architect (Luna, low effort) diagnose a known bug from **train labels alone**? This is the information a P4 lessons distiller may use.
- Inputs: the spec, the public-green candidate, and the label "fails at least one hidden test". No test content and no hidden output.
- Cases: the same 32 T0 cases.
- Output: up to 3 suspect clauses, the bug in one sentence, and one task-agnostic lesson. One `codex exec` call per case (`gpt-6-luna`, low effort), with the prompt inline and an empty working directory.
- Metric: **clause hit rate**, meaning at least one suspect is among T0's violated clauses (Sol, with hidden tests). It is compared with the chance rate of 3 random clauses.
- Use (fixed now):
  - hit rate ≥ 0.5 and ≥ chance + 0.2: single-shot distillation from labels is viable;
  - otherwise the P4 distiller also gets **contrast pairs** (a hidden-passing and a hidden-failing candidate of the same task, from train labels) wherever they exist.
- Budget: 32 Luna calls.
- Result (`runs/2026-10-15-p4-0-probe/probe.json`; 32 Luna calls, 0 errors):
  - **Clause hit rate 0.562 (18/32) vs chance 0.252.** The rule is met: single-shot distillation from labels is viable.
  - Luna's label-only lessons cover the same themes as T0's hindsight lessons:
    - validation order and precedence;
    - overflow detected during accumulation;
    - no allocation from untrusted sizes;
    - offsets in original-input coordinates;
    - finishing ready work before advancing time.
  - Most misses still yield a sensible general lesson (typically validation precedence).
  - Contrast pairs remain an optional upgrade, not a requirement.

### E09 — H0 v3 live check on all train (also P4's G0)  [phase P3 → P4, development lane]
- Registered: 2026-10-15, one-row development pre-registration.
- H0 v3 = `harnesses/h0-v3.json` (sha256 `d0f530ef005b08c510718d6070a53a1cdaec161b17924b3957654623f925c2fd`), which is H0 v2 + `stop_at_public_green`. This is the P4 baseline, and it has not been run live before.
- Suite: `tasks/train-all` (relative symlinks to calibration-v2, dev-v2 and c1-train; 140 tasks, 35 families; suite hash `bf75b130cf7d924101fbc49da1cfc6467e259578aed0716f5061c5aabeb68196`).
- Arms D and C(H0 v3); k = 5; `--primary D`; seed 13; jobs 4; `--unregistered`. Out: `runs/2026-10-15-e09-h0v3-train`.
- Expectation (**sanity, not a claim**): C(H0 v3) within ±5 pp of D, and C's invocations per task within 1.5× of D's. C's public-green-but-hidden-fail submissions are the G0 failure dossiers for P4.
- Budget: ≤ 700 invocations, 10,800 s. Launched after S4 completes.
- Result: pending
