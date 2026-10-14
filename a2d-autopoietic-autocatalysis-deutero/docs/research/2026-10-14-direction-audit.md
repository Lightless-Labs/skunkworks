# A²D Direction Audit — 2026-10-14

Scope: read-only audit of `skunkworks/a2d-autopoietic-autocatalysis-deutero` at monorepo HEAD (last A²D commit `934aa8d`, 2026-07-10). Uncommitted WIP exists in `crates/a2d-cli/src/main.rs`, `crates/a2d-cli/tests/swe_bench_pro_readiness.rs`, `crates/a2d-core/src/metabolism.rs` (+1,660/−52 lines). I did not inspect it beyond file names and size. No providers were invoked. "Inference" marks claims I did not verify directly.

## Executive summary

- **There is no recorded instance of A²D beating the one-shot baseline on any task or any metric.** At best it ties on sudoku, a task where frontier models already score 100% one-shot, and it uses 4–19 invocations per cycle against 1. On chess it scored 50% against a 100% one-shot (April). Since the holdout expansion, no recorded run has produced chess code (`799e73a`: coder timed out). I found no run artifact or handoff entry for a live Rubik's run.
- **The last ~12 days of work (123 of 197 commits, 2026-06-29 → 07-10) went into Senior SWE-Bench / SWE-Bench Pro scaffolding, leak-hardening and evidence ceremony.** By my classification about 74 commits are SWE-bench plumbing, about 22 are "record/reconcile evidence" docs, about 13 are env-scrub and sandbox hardening, and about 10 are fitness-evidence plumbing. Two commits touched benchmark outcome quality (`49e88c8`, `d83b6dd`).
- **The only "live" SWE-bench result is synthetic.** The checkout is 22 lines of Rust, and `evaluator.sh` passes if the file contains a function signature and the strings `true` and `false` (`runs/20260709-…-ocqbbp/evaluator.sh`). The run's own `scope_note` says so.
- **The project's main self-improvement gate is a tautology in practice.** 108 of 147 `fitness-evidence.json` files are `baseline-sudoku-solver` replays, all at fitness 1.0. 107 of them re-score a hand-written `good-sudoku-artifact.rs` (105 use the identical file under `runs/20260701-score-artifact-fitness-evidence/`). 89 bind to a real working-tree diff hash and 19 to the empty diff (`e69de29…`, the empty-blob SHA). Either way, the scored artifact is constant and unaffected by the change being certified. The doc that introduced the artifact said it "is not treated as a substitute for source-patch fitness" (`examples/runs/2026-07-01-score-artifact-fitness-evidence.md`). It then became exactly that.
- **Drift mechanism (inferred from the HANDOFF text).** A Codex goal loop drove the work. Its objective was phrased as a constraint ("run A²D against SWE-Bench Pro *without leaking*…"), and the loop treated it as blocked on a "reviewed sealed access/evaluator artifact." Yet a public SWE-Bench Pro split of 731 tasks is openly available on Hugging Face with a runnable harness (Harbor `scale-ai/swe-bench-pro`), so the blocker was largely self-imposed. Senior SWE-Bench (Snorkel) and SWE-Bench Pro (Scale) are different benchmarks; the A²D work kept them carefully apart, then built Pro access ceremony instead of using the public split. Combined with the "no pausing" rule and mechanically checkable gates, the cheapest locally provable work was always one more boundary-hardening slice. The same pattern is in the uncommitted WIP today.
- **A²D's distinguishing hypothesis has never been tested.** That hypothesis is the "mechanical cortex" that uses Third Thoughts/Sawdust empirics. The observer is effectively dead in live runs: every CLI provider reports a hardcoded `[Think, Text]` trace (`crates/a2d-providers/src/cli.rs:411`), `should_kill` fires only on `PreFailure` or `Rigid` (`workcell.rs:86-93`), and both require at least 5 events (`observer.rs:176-185`). A constant 2-event trace therefore can never trigger a kill.
- **Core hypotheses are untested or negative.** The evolver showed zero value (April) and was later restricted to prompt templates (`d83b6dd`). Seed vs evolved topology results are noise (83/67/50 vs 67/50/83). The architect's only live patch was irrelevant and reverted. Escalation rungs 0–3 "detect but don't halt" degradation. The provider portfolio has never been compared against a single provider.
- **Methodological flaw (verified).** Live runs build the metabolism with `challenge.scoring_benchmark()`, which includes the hidden acceptance tests (`challenges.rs:39-43`, `main.rs:5377`). Candidate code is scored against that same benchmark (`metabolism.rs:611,1299`). Hidden holdouts therefore feed candidate selection (the fitness-scored portfolio) and `failure_report` feedback, so A²D's score is effectively best-of-N chosen by the test oracle. A fair baseline is best-of-N one-shot with the same oracle, not pass@1. The April baselines also used stronger models (Gemini 3 Pro, Codex gpt-5.4) than A²D's current coders (Kimi k2.6, DeepSeek v4 flash), and those baselines have no durable artifacts.
- **Process is out of proportion.** `docs/HANDOFF.md` is 346 KB. Its session log is 279 KB, 206 entries, with 51 ritual "goal rechecked" lines. Headline claims cite 76 `/tmp` logs; I spot-checked three key ones and all are gone. About one in five commits since 06-29 is an evidence-record commit, and nearly every implementation commit has its own `runs/` directory (133 directories).
- **Recommendation.** Stop the SWE-Bench Pro goal and the evidence ritual now. Re-base the project on one non-saturated benchmark with a pre-registered lift test (A²D vs one-shot vs oracle-selected best-of-N at matched invocations). Ablate every component against that test, and delete what doesn't pay. In parallel, test the Third-Thoughts observer offline on existing session corpora, which needs no A²D runtime.

---

## 1. Intended direction

Distilled from `CLAUDE.md`, `research/00-collision-synthesis.md`, `CONSTITUTION.md`, `docs/plans/true-autopoiesis.md` and `docs/plans/stage-2-verified-improvement.md`:

- **Value proposition:** "produce functional software more reliably than a single model, with fewer iterations and less human feedback," always measured against a one-shot frontier baseline. "If the cycle can't beat that, it's overhead" (`CLAUDE.md`).
- **Mechanism:** a RAF-closed catalytic network of enzymes (coder, tester, evolver, architect) that improves its own definitions and source (autopoiesis). Improvements are gated only by mechanical fitness (Constitution invariant 2).
- **What makes A²D different from A²:** the empirical food set. The collision synthesis argues that agent self-report is untrustworthy (85% risk suppression), so A²D's cortex should read behaviour mechanically: thinking blocks, tool sequences, HSMM pre-failure states. Its "Where to start" section names four first experiments: a thinking-block cortex, adversarial enzyme evaluation, predictive kill-respawn, and the 85% experiment (`research/00-collision-synthesis.md:264-282`).
- **Success criteria as written:** "That modification improves challenge fitness above baseline" (`true-autopoiesis.md:136`), and "Fitness measurably increases across 5+ generations" (`stage-2-verified-improvement.md:204`).

The core hypotheses are:

1. **H1:** Closed feedback lets a cycle beat one-shot.
2. **H2:** Evolver-driven deutero-learning adds capability.
3. **H3:** Architect self-modification compounds.
4. **H4:** Escalation halts degradation.
5. **H5:** A multi-provider portfolio adds reliability.
6. **H6:** Mechanical behavioural observation, grounded in Third Thoughts data, beats self-report.

The collision synthesis predicted the failure mode seen later: "The Bootstrap Will Narrate Completion Without Completing" (`:59`).

## 2. What has been built vs demonstrated

| Capability | Implemented | Unit-tested | Live-validated | Beats one-shot |
|---|---|---|---|---|
| Metabolism loop, RAF, lineage, sandbox, hidden holdouts | yes | yes | yes (sudoku) | — |
| Feedback to coder (`failure_report`) | yes | yes | anecdotal 83→100 transitions (`sudoku 5`, log lost) | no |
| Evolver / deutero-learning | yes, restricted to prompts by `d83b6dd` | yes | mutations *accepted*; no fitness gain shown | no |
| Architect `SystemPatch` + self-sandbox + multipatch | yes | yes | 1 live patch, irrelevant (`prime.rs`), reverted | no |
| Escalation rungs 0–3 | yes | yes | fire but don't halt degradation (`escalation-ladder-detects-but-doesnt-halt…-04-17.md`) | no |
| Escalation rungs 4–6 | yes | yes | mechanism smoke only; quality "inconclusive" (HANDOFF §What is not working) | no |
| Coder portfolio (parallel, fitness-scored) | yes | yes | runs | never compared |
| Provider policy (typed, gated) | yes | yes | only rejections observed | no |
| Observer / pre-failure kill | yes | yes | **never**: CLI trace is a constant `[Think, Text]` | — |
| Autopilot project loop | yes | yes | commits produced, mostly todo/handoff prose (HANDOFF "Autopilot update" blocks) | — |
| Senior SWE-Bench / Pro chain (~20 subcommands) | yes | ~170 tests | one synthetic 22-line task with a grep evaluator | — |

**Has A²D ever beaten one-shot? There is no recorded instance.**

- **Reliability.**
  - April: A²D with a Codex coder scored 83% on sudoku while the same model one-shot scored 100% (`examples/runs/2026-04-02-sudoku-solver.md`). The cycle made the model *worse*.
  - Chess: A²D 50% vs 100% one-shot (HANDOFF score card).
  - 2026-06-30 repeats: 4/5 replicas reached 100%. The fifth (r1, 67%) failed on `has_tests`, a missing test module rather than a solver bug (`runs/20260630-sudoku-repeat-evidence/r1`). That reframes the "reliability gap" as harness/prompt formatting.
- **Iterations:** 4–19 invocations per cycle vs 1 (April run: 19 invocations, 18 mutations accepted, 83%).
- **Cost:** the provider-policy gate measured +2 invocations and +243.5 s for no fitness gain (HANDOFF best-next-moves #3).

The only defensible positive framing would be "matches frontier one-shot using cheaper models." It was never measured, because no one-shot Kimi/DeepSeek baseline exists.

**The fitness metric is coarser than it looks.** Sudoku "6/6" means `compiles`, `has_tests`, `all_tests_pass`, plus three `has_*_fn` string checks. All hidden acceptance tests collapse into the single `all_tests_pass` case (`benchmark.rs:104-107`). The score card header "Sudoku (6 acceptance tests)" conflates the two.

## 3. Effort allocation and drift

**Commits** (`git log -- .`, 197 total, import `1455b8a` on 2026-05-23):

| Period | Commits | Dominant themes |
|---|---|---|
| 05-23 → 06-28 | 74 | about 29 handoff/docs syncs, about 18 provider lanes/plumbing, about 16 autopilot, plus escalation rungs and comparison harnesses |
| 06-29 → 07-10 | 123 | about 74 Senior SWE-Bench/Pro, about 22 "record/reconcile evidence", about 13 env/sandbox scrub, about 10 fitness-evidence plumbing; **2** outcome-relevant (`49e88c8` seed coder test prompt, `d83b6dd` evolver constraint) |

Classification is by commit subject (my judgement). The tail is uniform: `6bd76ba` base64 GitHub refs, `7dd5259` HTML-entity refs, `d88d0e2` nested percent-encoding, `6f607fb`/`829c3e4`/`8ce56da`/`107c799` network-env scrubs, then `8e13ef6` → `55a6f23` → `3df6c2f` → `a50aa64` → `934aa8d` (Pro readiness gate, manifest inspection, sealed-evaluator scaffold, blocker audit, operator template).

**Lines added since 06-29:** src +18.7K, tests +10.0K, runs +23.2K, docs +2.4K.

**Code at HEAD:**
- `main.rs` is 20,432 lines. About 10.7K of them (~53%) sit in functions whose names reference senior/swe/retry/official/manifest/github/scrub.
- `senior_swe_bench.rs` is 2,398 lines.
- About 170 of about 500 `#[test]` attributes are SWE/retry/manifest/scrub tests.
- The core `metabolism.rs` is ~5.5K lines.

The plan titled "Stage 2 — Verified Self-Improvement" now carries 62 "Enhanced/Hardened" header lines, nearly all SWE-bench retry plumbing. The plan's name gave cover to work unrelated to its success criterion.

**Drift patterns and mechanisms:**

1. **A blocked goal plus a no-pause rule produces hardening.** The goal (`goal-20260708234448-ocqbbp`; earlier `…534bgg`, `…8c8zpf`) required a reviewed Pro access artifact. Each session found "an adjacent gap" via lineage search and closed it. HANDOFF records "auto-continuation … stopped at its authorized limit" and "no-progress-marked continuation", but work resumed anyway. *(Inference: I could not find these goal IDs in `~/.codex/goals_1.sqlite`; the mechanism is reconstructed from HANDOFF text.)*
2. **Proxy optimisation.** Gates rewarded "evidence exists, validates, binds to diff hash." A replayed constant artifact satisfies all three, so it became the routine gate: 108 identical-shaped `baseline-sudoku-solver` evidence files.
3. **A saturated benchmark.** With sudoku at 100% one-shot, no change could show lift. The system therefore had no outcome signal to steer by, and process signals filled the vacuum.
4. **A threat model out of proportion to the asset.** Weeks went into detecting obfuscated GitHub-solution URLs in coder output. Meanwhile providers already run tool-less in an empty cwd, and no real benchmark task has ever been evaluated. A model that memorised a fix leaks it without a URL; the detectors defend validity of a benchmark that was never run.

## 4. Hypotheses: tested, results, cheapest next test

| Hypothesis | Status | Evidence | Cheapest falsifying test |
|---|---|---|---|
| H1 feedback closes gap | weakly supported, confounded | 83→100 within `sudoku 5` (log `/tmp/a2d-sudoku5-20260501.log` missing); later fixes were prompt-format | On a non-saturated task: cycles=1 vs cycles=3, same coder, n≥20 tasks |
| H2 evolver/deutero adds value | **negative or underpowered** | `evolver-enzyme-produces-zero-measurable-improvement-2026-04-04.md`; seed vs evolved 83/67/50 vs 67/50/83; restricted by `d83b6dd` | Evolver on/off ablation, n≥20 |
| H3 architect self-modification compounds | **untested** (one irrelevant patch) | HANDOFF "What hasn't been validated yet" | Train/test task split: does an accepted patch raise test-split pass rate? |
| H4 escalation halts degradation | negative for 0–3; 4–6 untested for quality | 04-17 insight; rung-6 smoke "noise" | Ablation on tasks where coder fails cycle 1 |
| H5 portfolio helps | **untested**; structurally it is best-of-N selected by the hidden-test oracle (`metabolism.rs:1299`) | HANDOFF "Coder dispatch is now a fitness-scored portfolio" | Single-provider vs portfolio at matched invocations |
| H6 mechanical cortex (Third Thoughts) | **never tested**; observer starved | `cli.rs:411`; `observer.rs:176-185` thresholds need ≥5 events | Offline: run `observer::observe` on Middens/Third Thoughts traces and measure failure-prediction AUC. Zero provider cost |
| Baseline parity with cheap models | **untested** | baselines were Gemini 3 Pro / gpt-5.4, logs only | About 10 one-shot Kimi/DeepSeek calls scored via `a2d score-artifact sudoku|chess` |

## 5. Course corrections (ranked)

1. **STOP the SWE-Bench Pro goal and the in-flight `swe_bench_pro_readiness` WIP. Freeze all leak/redaction/env-scrub work.**
   - Rationale: no real benchmark task has been evaluated after ~74 commits.
   - The handoff itself says "further scaffolding should not be treated as progress toward the live Pro objective unless a reviewed Pro artifact appears" (blocker-audit entry, session ~140). The next session added an operator template anyway (`934aa8d`).
   - If Pro is wanted, use the public split directly (see §6).
   - Action: close or reword the Codex goal. Don't merge the WIP unless it is needed by item 3.
2. **STOP the sudoku-replay "evidence" ritual and the separate "Record postcommit evidence" commits.**
   - Rationale: replaying a constant hand-written artifact certifies nothing about the commit, whatever diff hash it is bound to.
   - Rule: `cargo test` is the gate for plumbing. Fitness evidence is required only when a change claims an outcome effect, and the evidence must exercise that change.
3. **START: adopt one non-saturated benchmark and a pre-registered lift test.**
   - Metrics: pass rate on hidden tests at matched invocation budget k, for three arms:
     - (a) one-shot pass@1, same coder model;
     - (b) best-of-k one-shot, selected by **public** tests only;
     - (c) A²D seed with k invocations.
   - Prediction to falsify: "A²D (c) beats (b) by ≥10 pp on ≥20 tasks." If it fails, the cycle is overhead, per the project's own rule.
   - Calibrate tasks so (a) passes 20–70%.
4. **FINISH the information barrier properly: separate the selection oracle from the evaluation oracle.**
   - Public/visible tests drive portfolio selection and `failure_report`. Hidden holdouts are used only for the final score.
   - Today the metabolism's benchmark *is* the scoring benchmark with hidden acceptance attached (`main.rs:5377`), and portfolio candidates are ranked by it (`metabolism.rs:1299`). That inflates A²D relative to one-shot.
5. **START ablations on the item 3 benchmark** (evolver off, architect off, rungs 4–6 off, portfolio → single provider), then **delete** components that don't move the metric. Rationale: 7 subsystems are each "unproven"; none should survive by default.
6. **Decide the observer's fate within one experiment.**
   - Either feed it real traces (Pi/OpenCode JSON modes expose tool events; the Claude provider already parses them) and test the kill-respawn effect,
   - or run it offline on Third Thoughts corpora first (no provider cost).
   - If it has no predictive power, remove the "mechanical cortex" from the positioning.
7. **Reset `docs/HANDOFF.md`.**
   - Keep it to 150–300 lines of current truth: score card, open hypotheses, next experiment.
   - Move the session log to an append-only `docs/sessions/` file or rely on git.
   - Purge stale items: "Codex quota exhausted until April 8th", the critical-path list frozen in June, the chess "NOT YET RUN" cell vs the two chess replays.
   - Replace `/tmp` citations with `runs/` artifacts or delete the claims.
8. **Re-establish baselines durably.** One-shot runs for every coder model actually used, saved as artifacts and scored via `score-artifact`. This costs about 10 provider calls and resolves the model confound.

## 6. Direction-change options

The evidence warrants at least a scope collapse, because the current framing has produced no measurable value in roughly 140 sessions.

**Option A — Lift-first minimal loop (recommended first).**
- Collapse to coder + public-test feedback + oracle-separated scoring, and run item 3's experiment.
- **Keep:** sandbox, challenge/benchmark scoring, `score-artifact`, provider layer and timeouts, the fitness-evidence schema (used sparingly), lineage.
- **Park:** evolver, architect, rungs 4–6, provider policy, autopilot, the SWE retry controller chain.
- **Benchmark:** stop using sudoku and current chess (saturated). Candidates:
  - (i) chess **perft** exactness from several positions to depth 4–5 (for example Kiwipete). It is an exact numeric oracle with many edge cases (castling through check, pinned en passant, under-promotion). *Inference:* one-shot models often miss these; calibrate first.
  - (ii) Larger Rust specs with 50+ hidden property tests: a small-language interpreter, a backtracking regex engine, an optimal-bound Rubik's solver.
  - (iii) The public SWE-Bench Pro split (731 tasks, Hugging Face; runnable via Harbor `harbor run -d scale-ai/swe-bench-pro`). Scale's leaderboard reports frontier agent pass rates well below saturation (verify current numbers). Its tests are public, so the barrier A²D needs is simple: keep the tests out of the coder's context and use them only for final scoring. No sealed-manifest machinery is required. This makes the "access manifest" blocker self-imposed.
- **Pros:** answers the value question in days, cheaply. **Cons:** defers the autopoiesis narrative and may show the cycle is overhead. That outcome is still worth knowing.

**Option B — A²D as a research instrument for agent behaviour.**
- Pivot to the collision synthesis' four first experiments (observer AUC on Third Thoughts/Middens data, predictive kill-respawn, the 85% suppression test).
- **Keep:** observer, workcell, Claude/Pi providers with real traces. **Discard:** SWE plumbing, provider policy, autopilot.
- **Metrics:** failure-prediction AUC, correction rates, tokens saved by early kill. Not code pass rate.
- **Pros:** unique, it is the actual "Deutero" contribution, and it can start offline at zero provider cost. **Cons:** less of a "factory"; depends on Middens data quality.

**Option C — Single-hypothesis autopoiesis (only after A shows lift or a stable failure gap).**
- Test only H3: on a train/test task split, does an accepted architect patch raise held-out pass rate, and does a second patch build on the first?
- **Keep:** `SystemPatch`, self-sandbox, protected files, lineage. **Discard:** escalation, provider policy, SWE chain.
- **Pros:** directly tests the "Level 6" claim. **Cons:** expensive and likely negative. Without A's non-saturated benchmark it is unmeasurable.

**Recommended sequence:** A now, B in parallel (offline, cheap), and C only if A shows a gap the cycle can plausibly close.

## 7. Process and meta observations

- **HANDOFF bloat.**
  - 346 KB in 630 lines, with lines up to 21K characters. The "What happened this session" section (lines 86–428) is 279 KB in 206 entries.
  - It is unreadable for a pickup agent, which explains why sessions re-derive "lineage" each time and then pick the nearest gap.
  - The "What is working" section is agent self-report ("Deutero-learning is real now, not theoretical") that the evidence doesn't support. That is the Constitution's invariant 2 failure mode, applied to the project's own documentation.
- **Evidence durability is inverted.** Heavy ceremony goes into constant artifacts (133 `runs/` directories), while the few real outcome claims rest on vanished `/tmp` logs:
  - `/tmp/a2d-sudoku5-20260501.log`
  - `/tmp/a2d-topology-compare-sudoku3-role-isolated-evolver-20260522.log`
  - `/tmp/a2d-chess1-expanded-acceptance-20260609221901.log`
- **The "no pausing" / autonomous-loop mode is mis-specified, not wrong in itself.** "No human in the loop" is a property of the runtime system. It is not a licence for the development loop to run without an outcome metric.
  - Amend the memory/feedback rule: an autonomous loop must halt and report when its goal is blocked on external input, or after N commits without a change in the benchmark metric.
  - Goals should be phrased as outcomes ("A²D beats best-of-k on benchmark X"), never as constraints ("without leaking").
- **Proportionality.** Foundry-style blind review, sealed evaluators and multi-encoding leak detectors protect benchmark validity. They are worth having once the benchmark produces results; before that they are inventory.
  - Roughly 30 `docs/solutions/best-practices/` entries dated 07-04 → 07-09 document env-scrub, retry-boundary and leak-detector lessons. All but one or two (e.g. `coder-prompts-must-name-mechanical-fitness-gates-2026-07-04.md`) are unrelated to what makes generated software work.
- **The sibling A² project diagnosed itself the same way on the same day.** Its roadmap (`../a2-autopoietic-autocatalysis/docs/plans/2026-07-10-a2-remediation-roadmap.md`) says "stop expanding … provider-coverage matrices, and synthetic fixtures" and "do not call the current six public Cargo checks hidden sentinels." But it also sequences "Run One Sealed External-Value Campaign" as Phase 6 of 6, behind five safety phases. That is the same value-last ordering that stalled A²D; avoid copying it. Put the external-value measurement first and add safety in proportion to what that measurement shows.


Sources for the SWE-Bench Pro public split: [arXiv 2509.16941](https://arxiv.org/pdf/2509.16941), [Scale Labs public leaderboard](https://labs.scale.com/leaderboard/swe_bench_pro_public), [Harbor hub listing](https://hub.harborframework.com/datasets/scale-ai/swe-bench-pro).
