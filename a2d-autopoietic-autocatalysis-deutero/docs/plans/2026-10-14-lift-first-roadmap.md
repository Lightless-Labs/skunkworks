# Plan: Lift-First Roadmap

**Created:** 2026-10-14
**Status:** Completed (falsified) — 2026-10-15. The seed was RULED OUT on Confirmation-1 (LOG E07 + E07b). Self-modification was not supported at the development screen and is unconfirmed. Report: `docs/reports/2026-10-15-lift-first-outcome.md`.
**Completed:** 2026-10-15 (falsified)
**Reviewed:** 2026-10-14 (independent review, Codex gpt-6.1-sol xhigh). The other non-Claude lanes were unavailable; see Blockers. Changes adopted: repair-loop comparator, three-way result classification, pilot/development/confirmation split, frozen controller, B-headroom calibration, harness-first P4, process metrics as diagnostics only.
**Addendum:** 2026-10-14 — coder lane: the owner stopped the Kimi, Z.ai, MiniMax and Gemini subscriptions on purpose and suggested Luna via Codex or Pi. `gpt-6-luna` works through Codex with A²D's provider flags. `gpt-6.1-luna` is not offered on this ChatGPT account (Codex model list fetched 2026-10-14 has 6.1 only as Sol). Pi's `openai-codex` provider is not authenticated. P0/P1 updated accordingly.
**Addendum:** 2026-10-14 — P0 gate. Exit criteria met (isolation tests, four arms end to end on mocks, E01). Reviewed by Codex `gpt-6.1-sol` (high) and a fresh-context Claude subagent, each in an empty temp dir with copied inputs. Diversity reduced to one non-Claude family (see Blockers). Adopted (commits `e8e7fa3`, `2eefa0d`):
- no-tools Codex lane (live-verified);
- an oracle that compiles trusted tests apart from candidate code and requires a clean exit;
- two-phase run (generate everything, then score once);
- seeded per-tuple arm order;
- tuple voiding;
- `--primary` comparator;
- a classification floor of `--min-families` (default 10) and at least one discordant pair;
- family sign-flip permutation p reported beside the cluster bootstrap;
- hash-chained ledger;
- a registration freeze over all of `crates/`, `Cargo.toml` and `Cargo.lock`, so the treatment is frozen, not only the controller.

Two findings are deferred:
- **To P2:** at k = 4 the seed schedule gives arm C one coder call (coder → evolver → architect → tester). P2 must pre-register a k, or a schedule, under which C reaches a second coder call. E02 measures the realised schedule.
- **To P2 confirmation:** the confirmation protocol needs deferred scoring (commit submission hashes, then score).

Process diagnostics:
- 1 session, 1 result (E01), 3 live invocations per result;
- 12 commits since adoption, 8 tagged;
- no tripwire hits.

Reviewer note: arm C landed without a live four-arm run, so the next result must be E02 (all arms live), not more controller work. P1 plan: `docs/plans/2026-10-14-p1-calibration-suite.md`.
**Addendum:** 2026-10-14 — P1 gate (design for P2).

Reviewers were Codex `gpt-6.1-sol` (high) and a fresh-context Claude subagent, each in an empty temp dir with copies of the inputs. The subagent also simulated the implemented classifier: whole-family percentile bootstrap, 37 families × 4 tasks.

Decisions, all made before any confirmation data exists:

- **Budget.** "Same budget" in the Objective and Metric means an **equal invocation cap per task (k)**. Realised spend per arm (invocations, tokens, provider time) is reported beside every claim, and claims are worded "at an equal invocation cap". In E05, C used about 3× D's calls and tokens, because D stops at public-green.
- **Claim rule.**
  - **SUPPORTED** requires the primary comparison (C vs D) **and** the secondary comparison (C vs B) to be SUPPORTED (intersection–union).
  - **RULED OUT**, i.e. falsification, is decided by C vs D alone.
  - Completion's "RULED OUT for the seed or minimal configuration" refers to that primary comparison.
- **Power target, corrected.** SUPPORTED needs a point lift of at least 10 pp, so P(SUPPORTED) is about 50% at a true 10 pp for any n. The earlier "80% power at 10 pp" described a different event. The targets are now P(SUPPORTED | true 15 pp) ≥ 0.8 and P(RULED OUT | true 0) ≥ 0.8.
  - At 37 × 4 × 1 replicate the simulation gives P(SUPPORTED | 15 pp) ≈ 0.82–0.94, but P(RULED OUT | 0) only ≈ 0.50–0.74 (paired-difference ICC 0–0.3).
  - At 2 replicates with per-arm agreement ≈ 0.79, P(RULED OUT | 0) ≈ 0.96. The replicate count is set from E06.
  - The class rule is unchanged.
- **Classification with exclusions.** Classes are computed over complete tuples when at most 5% of planned tuples are excluded (voided, aborted, missing an arm); above that, the run is Invalid. Provider timeouts are arm failures, never voids. Hidden-name hits or any coder tool call turn the run into IntegrityReview.
- **Submission tie-break.** Most public passes, then compiled, then the **latest** attempt (was: earliest). This was decided on design grounds before any attempt-level hidden data was examined. For the feedback arms, a later equal attempt has absorbed more feedback, and A²D keeps its latest code. It is a treatment change, re-piloted by E06.
- **Pilot set.** calibration-v2 serves as the P2 pilot (E05, E06) instead of the development tasks: it shares the confirmation template, while the development set is 3 saturated legacy tasks. This is its third use (E04–E06), against "not reused", but it never supports a claim.
- **Sample extension.** The single allowed extension is **one more replicate pass over the same Confirmation-1 tasks** (no new authoring), analysed pooled at the family level. The 25 families added to Confirmation-1 are part of the initial sample, not an extension.
- **Independent reference (required for P1 exit, § P1 Oracles).** Today's references share an author with their tests. Before P2, Codex `gpt-6.1-sol` solves Confirmation-1 blind from the requirements and public tests (arm D, output under `$A2D_HOLDOUT_DIR/audit/`). A Codex adjudicator repairs any task where that solution fails a hidden test the spec does not determine. Rules:
  - audit outputs are never experiment data;
  - flagged tasks are repaired, never dropped;
  - re-validation happens before the hash is taken;
  - auditor and coder share a vendor, a recorded limitation.
  - Confirmation-2 gets the same audit before P4.
- **Validation now enforces:**
  - mutants that compile and fail a hidden assertion;
  - at least 4 mutants, of which at least 2 are public-green;
  - at least 3 public and at least 20 hidden tests;
  - a deterministic reference;
  - tests that compile against the reference **as a separate crate** (public API only).
- **Run hygiene.** No other Codex jobs run on the account during a registered run, so holdout authoring is paused. Global cap = the planned maximum; wall clock = 2× the expected time. The codex CLI version is recorded.
- **Content-free ledger.** 0 of 450 hidden-only test names appear in E05's ledger. The pre-score anchor is `a2d lift manifest` (no argv or paths), committed before `a2d lift score --expect-head`.
- **Deferred:**
  - automated operator-mutation score (as a covariate);
  - an exploratory token-matched D′;
  - family-grouping sensitivity for near-sibling families (base32-codec and varint-codec vs stream-b64).

Process diagnostics:
- 1 session, 5 results, 25 commits up to E05 (21 tagged), about 627 live invocations (3% of the cap);
- T7 at 1 of 2; no tripwire hits.

Reviewer note: the cadence was good. The drift was analytical: a power formula never checked against `classify`, replicate value inferred from per-arm agreement, and a pilot-set change without an addendum. All three are corrected here. Lesson: plan with a simulation of the implemented decision rule, not a textbook formula.
**Addendum:** 2026-10-14 — **P1 exit.**
- **Confirmation-1:** 148 tasks, 37 families, hash `7bd1b669051c93c7…`.
- **Confirmation-2:** 48 tasks, 12 families, hash `25435ab324375a0f…`.

Both were authored and validated blind by Codex, and both pass an independent-solve audit (Codex `gpt-6.1-sol` from the requirements and public tests: 148/148 and 48/48), which meets § P1 Oracles' independent-reference requirement. The suite review is recorded in the P1-gate Addendum; the power planning is in E05 and E06.

Process diagnostics at exit:
- 1 session, 6 results (E01–E06);
- 778 coder invocations (3.9% of the cap), 6.7M tokens;
- 42 commits since adoption (38 tagged);
- T7 at 1 of 2; no tripwire hits.

Process changes adopted:
- simulate the implemented classifier for power planning;
- never run holdout authoring alongside a registered run;
- wait on run artifacts, not on `pgrep -f` of a pattern that matches the waiting loop itself.

Next: E07 (P2 confirmation).
**Addendum:** 2026-10-15 — **framing from the owner: autoresearch.**

> "the whole point of it is having a coding agent iterate and experiment on a self-improving, self-iterating, self-experimenting coding agent harness and process, with added human insights (the major difference with a2). Think … Karpathy's autoresearch, applied to agentic tools and software development workflows."

The roadmap already supplies the parts. The **harness** is the experimental object. `a2d lift` on development tasks is the fast experiment, with hidden pass rate as the metric and every run in the ledger. Changes are kept or discarded by that metric. Blind confirmation sets are the honest final claims. E07 is the baseline: the seed harness ≈ the plain repair loop, at 3.7× the tokens.

Consequences:
- **P3 becomes the outer research loop.** The developing agent is the researcher, taking human insights as they come. It proposes harness changes (ablations first, then edits), runs short registered experiments on development data, and keeps what moves the metric, with spend reported.
- **P4 asks whether A²D's own inner loop** (architect editing the harness-as-data, `docs/plans/2026-10-15-p4-autopoietic-lift.md`) improves further on the outer loop's best harness.
- Claims still come only from the confirmation sets: Confirmation-2, extended blind before P4.
- Unchanged: oracle and controller isolation, pre-registration, T1–T7.
**Addendum:** 2026-10-15 — **P2 exit: the seed lift is RULED OUT.**
- Pooled E07 + E07b, the pre-registered single extension: Confirmation-1, 296 tuples in 37 families.
- C vs D +1.4 pp, 95% CI [−5.4, +8.8]. C vs B +3.4 pp, CI [−2.7, +9.8].
- C used 3.7× D's invocations and tokens.
- That is half of "Completed (falsified)". Completion now depends on P4. Next: P3, the outer research loop, then P4.
**Addendum:** 2026-10-15 — **P2 gate.**

**Reviewers.** Codex `gpt-6.1-sol` (high) and a fresh-context Claude subagent, each in an empty temp dir with copied inputs. P2 is accepted with the narrow claim wording and the robustness checks recorded under LOG E07b. Decisions:

1. **Confirmation-1 becomes development data.** Its single registered scoring, including the one allowed extension, is spent.
   - Split by a committed seeded hash of family names (`docs/experiments/c1-split.json`): 17 train families (68 tasks) go to `tasks/c1-train/` (visible, committed); 20 validation families (80 tasks) go to `$A2D_HOLDOUT_DIR/c1-validation/`.
   - Validation is read only through run `summary.json`. Per-task and per-family validation statistics are never computed.
   - Limitation: Confirmation-1 and Confirmation-2 share an author and a template.
2. **P3 is rewritten as the autoresearch loop** (`docs/plans/2026-10-15-p3-research-loop.md`), and supersedes `2026-10-14-p3-ablation.md`.
   - **Base harness:** D's behaviour, because D is what must be beaten.
   - **Mechanisms are ranked by whether they add a signal beyond the public tests.** 92% of D's remaining failures are public-green, so only a non-public signal can help.
   - **Offline falsification on frozen candidate pools comes first.** Inertness is shown by unit test, not by live ablation.
3. **Harness = fixed interpreter + evolvable JSON configuration.** Every step's output must have a declared consumer, and that reachability is checked at registration. Prompt changes that also apply to D are reported with a "D + same prompt" control.
4. **Hidden-test boundary (clarifying the standing goal rule).** Hidden-test *content* never reaches any arm or enzyme. In P4 only, *aggregate pass/fail labels on train families* may reach the proposer. Validation and confirmation outcomes never reach any enzyme.
5. **Confirmation-2 is extended blind now** towards 37 families × 4, before any candidate is evaluated on it.
   - The final P3/P4 confirmation is one registered run (D, B, the best P3 harness, the P4 harness), scored once.
   - SUPPORTED must hold against both B and D.
   - Power is planned by simulating the joint decision with the new harness's development discordance.
6. **Development fast lane.** Development runs use `--unregistered`, a one-row LOG pre-registration (id, mechanism, kill threshold, budget) and the ledger. The full ceremony (freeze, manifest, deferred scoring) applies only to confirmation.
7. **Shared Codex account (standing condition).** The sibling A² session shares the account. This is pre-registered as a run condition, mitigated by seeded tuple and arm order, and no longer logged as a deviation per run. Holdout authoring by this session never overlaps a registered run.
8. **Budget envelope** for the remaining ≈ 15k invocations: P3 ≤ 3k, P4 ≤ 5k, Confirmation-2 run ≈ 5k, reserve ≈ 2k.
9. **Completion reporting** keeps three outcomes separate: the seed threshold is excluded (P2, done); an improved harness is found (P3); the self-improvement process is supported, inconclusive or ruled out (P4).

The consolidated current rules are in `docs/PROTOCOL.md`.

Process diagnostics:
- About 4.4k experiment invocations, plus about 200 audit invocations.
- 8 results.
- Spend: the P2 confirmation was 82% of experiment spend, justified as the falsification.

Lesson: never spend confirmation data without development evidence at the claim threshold.
**Addendum:** 2026-10-15 — **P3 exit and the P3 → P4 gate.**
- **P3 exits "not promoted."** It ran six mechanisms or revisions (S1, S1b, S2, S3, S4 and S4b). S1–S4 failed their pre-registered rules. S4b passed its development screen: four researcher-written lessons on a repaired base gave +11.1 pp over that base and +5.6 pp over D on 72 train tasks the lessons were not derived from. That survival authorises P4 spend only.
- **Two classes of finding.**
  - **Within-task self-signals** (spec tests, self-review, agreement selection) do not beat the repair loop at this regime.
  - **Cross-task lessons** are the only lever with evidence.
- **Plumbing defects** dominated several early readings:
  - the missing `main()` clause;
  - triplicated input;
  - JSON-escaped repair inputs;
  - an evolver that rewrites prompts.

  All are fixed and unit-tested (`docs/solutions/best-practices/inspect-first-prompts-of-a-new-harness-2026-10-15.md`).
- **Frozen P4 base: H0 v5.** An evolver-free coder enzyme with plain rendering, previous-attempt routing, the task rendered once, and a stop at public-green.
- **P4 plan v3** (`docs/plans/2026-10-15-p4-autopoietic-lift.md`), reviewed by Codex `gpt-6.1-sol` and a fresh Claude subagent. Every item has a disposition.
  - **Searches:** lessons written by A²D itself from per-task train labels, starting empty, in 2 informed and 2 uninformed-control searches. Each generation re-evaluates the incumbent fresh and accepts only at ≥ 4 net tasks.
  - **Validation go/no-go:** ≥ +10 pp over D.
  - **Confirmation-2:** one registered multi-harness run at r = 2. Completion requires C(H\*) to be SUPPORTED against C(H0 v5), D and B separately.
  - **Arms:** a researcher-lessons arm and a D + lessons arm, both descriptive only.
- **Completion wording:** if P4 is a no-go at validation, Confirmation-2 is not spent, and P4 is reported as "not supported at the development screen", never as "ruled out".
**Addendum:** 2026-10-15 — **P4 exit and closing gate: Completed (falsified).**
- **P4**, from the LOG:
  - E10 failed its never-green criterion. H0 v6 is parity with D; E10b passed.
  - E11: 4 searches × 3 generations. 4 of 12 candidates were accepted (2 informed, 2 uninformed control), about the rate under the null.
  - E12 (validation): H\* = i1, 68.8% vs D 70.0% (−1.2 pp [−12.5, +10.0]). **This is the pre-registered no-go.** The control tied (u1 68.8%), so the "learning from failures" attribution is withdrawn.
  - Confirmation-2 (144 tasks, `cb7cb496…`) **was not spent and stays unopened** apart from the disclosed `spreadsheet-eval` exclusion.
- **Completion:** the falsified condition is met.
  - The seed is RULED OUT on confirmation under the registered analysis: E07 + E07b, +1.4 pp [−5.4, +8.8]; two-look upper bound +9.8.
  - No P4 hypothesis is SUPPORTED within T7.
  - Disclosure: T7 used 1 of 5 slots, on a narrowed hypothesis 1. Hypotheses 2 and 3 were never run.
- **Three outcomes, kept separate:**
  - **seed:** ruled out;
  - **improved harness:** none promoted. The parity base's parity on fresh families is not established (C:h0 −12.5 pp on validation, exploratory);
  - **self-improvement process:** failed its development screen, unconfirmed, and not ruled out.
- **Closing gate:** reviewed by Codex `gpt-6.1-sol` (high) and a fresh Claude subagent in copies-only temp dirs. Both approved the label subject to wording corrections, which were applied.
  - The fresh Claude reviewer found a classifier boundary bug (floating point at exactly 10 pp). It is fixed with a test; no registered decision changes.
- **Process diagnostics** (deutero): in the report § Process diagnostics. They cover spend by phase, decisions made by rule vs adaptively, forking paths, and the asymmetry of defect hunts.
- **Pivot:** report § Recommended pivot. In order:
  1. check the carrier at zero calls;
  2. calibrate measurement (about 700 tuples per decision for ±5 pp; A/A and uninformed controls);
  3. one "D + X vs D" hypothesis in a stated regime;
  4. an owner decision on the instrument as a product.
**Supersedes as primary goal:** `swe-bench-pro-blind-evaluator-adapter.md` (parked), the Senior SWE-Bench retry chain (parked)
**Grounding:** `docs/research/2026-10-14-direction-audit.md`, `docs/research/2026-10-14-dgm-rqgm-relevance.md`
**Decision deadline:** P2 confirmation result recorded by **2026-11-30**. Missing it is a non-result, not a falsification: record "not measured by deadline", stop adding seed-cycle complexity, and continue per Completion.

## Objective

Find out whether A²D produces working software more reliably than simple uses of the same model at the same cost, and whether its self-modification increases that margin. Establish either answer cleanly.

Supported, ruled-out and inconclusive findings all complete this plan. Building infrastructure does not.

## Metric

All arms use the same coder model and the same resource budget. Every arm's final submission is scored once by hidden tests. Public tests are the only signal any arm may use while working.

| Arm | Definition |
|---|---|
| **A — one-shot** | pass@1. |
| **B — best-of-k** | k independent attempts; the submission is chosen by public tests, with frozen tie-breaking. |
| **D — repair loop** | up to k attempts: coder, public tests, failures fed back. Submit the best by public tests. |
| **C — A²D** | at most k provider invocations per task across all enzymes. Every enzyme uses the coder model at first (no portfolio). |

- **Primary comparison:** C vs the stronger of B and D. Which one is stronger is fixed from pilot results before confirmation.
- **Lift:** C's hidden-pass rate minus the comparator's, paired over tasks.
- **Budget:** matched invocations. Also report input, output and reasoning tokens, cost where known, test compute, wall-clock, and retries/fallbacks. A timeout or missing artifact counts as a failure. Infrastructure exceptions are predefined in the pre-registration.
- **Result classes:**
  - **SUPPORTED** — the 95% lower bound on lift is above 0 and the point estimate is at least 10 pp.
  - **RULED OUT** — the 95% upper bound is below 10 pp.
  - **INCONCLUSIVE** — anything else. At most one pre-registered sample extension is allowed. If the result is still inconclusive after that, treat it as NO-GO for adding complexity.
- **Statistics:** paired cluster bootstrap over task families, with replicates kept paired. Power-plan the confirmation n from pilot discordance, targeting 80% power at 10 pp and a cap of 150 tasks. Report exact McNemar alongside. Twenty to thirty tasks is a pilot, not a confirmation.

## Task sets

- **Calibration/pilot:** used for headroom checks and power planning. Not reused.
- **Development:** free use for building and debugging. Results from it are never claimed as findings.
- **Confirmation-1:** scored once, at the end of P2.
- **Confirmation-2:** fresh task families reserved for the final P4 claim, scored once.
- Hold out whole task families, not just individual tasks.
- Confirmation tasks and their hidden tests:
  - are authored or validated by a non-Claude provider;
  - live outside the repo (under `$A2D_HOLDOUT_DIR`), with only content hashes committed;
  - are never opened by the developing agent.
- Submission artifact hashes are committed before confirmation scoring.

## Controller (integrity)

`a2d lift` is the only path that runs arms and scores them. It writes an append-only JSONL ledger of registrations, invocations, submissions (artifact hashes) and scores.

- The pre-registration records the git hash of the controller and oracle files.
- Any change to those files between registration and run voids the registration. Check this mechanically with `git diff <reg-sha>..HEAD -- <controller paths>`.
- Self-modifying agents (evolver, architect, P4 harness search) may not edit the controller, the oracles, stopping rules or the ledger. Extend `CONSTITUTION.md` protected paths accordingly.

## Development loop (process)

Each unit of work is one **experiment**:

1. **Pre-register** in `docs/experiments/LOG.md` before any live run, using the template.
2. **Smallest change** that lets it run. Write the tests first; `cargo test` must be green.
3. **Run** through the controller. Artifacts go under `runs/<date>-<experiment-id>/`. Never cite `/tmp`.
4. **Record** the result class, the confidence interval and the artifact path. Negative and inconclusive results are results.
5. **Decide** by the metric. Overwrite HANDOFF (≤250 lines). Compound non-obvious learnings into `docs/solutions/`.

Commit messages carry the tag, e.g. `feat(a2d): [P0] split public and hidden oracles`.

### Tripwires

- **T1 — results cadence:** no more than 2 consecutive sessions, or 10 commits, without a new LOG result (pilot results count; "it compiled" does not). On a hit, the next action is a run.
- **T2 — blockers:** when blocked on external input (credentials, quota, access), log it under Blockers and switch to a track that is not blocked. Never build scaffolding around a blocker.
- **T3 — proportional safety:** new redaction, scrubbing or sandbox hardening needs a triggering incident in a run artifact. Exemption: oracle and controller isolation required by this plan.
- **T4 — evidence must exercise the change:** a constant-artifact replay certifies nothing. Plumbing is gated by `cargo test`; outcome claims by the controller.
- **T5 — budget:** every live run has pre-registered invocation and wall-clock caps; abort at 2× and record why. The global cap for this plan is 20,000 provider invocations, re-assessed at each phase gate.
- **T6 — goal phrasing:** goals are stated as outcomes, never as constraints.
- **T7 — bounded search:** at most 5 P4 hypotheses in total, and at most 2 P1 suite redesigns. There is no reset-on-success counter.

### Model diversity

Phase gates and key designs (oracle split, suite, P4 search space) get independent review from at least two non-Claude providers when available. When fewer are available, record which ones were and proceed; never wait.

Reviewers run in an empty temporary directory with copies of the inputs, never with `-C <repo>`. `codex exec -s read-only` is not enforced on this machine (see `docs/solutions/` sandbox note). Delegate bounded implementation to Codex. Use Claude subagents for repo navigation and parallel independent tasks.

### Deutero layer (process learning)

At each phase gate, compute process diagnostics from git and the LOG:

- results per session;
- wall-clock and invocations per result;
- share of commits tagged to an experiment;
- tripwire hits.

These are **diagnostics, not gates**: commit counts are editable, and fewer tripwire hits can mean evasion. Process changes are proposed as dated **Addenda** stating the outcome they target (results per unit of cost), reviewed by a non-Claude provider, and revisited at the next gate. Stopping rules, the controller and oracles are outside process self-modification.

## Phases

### P0 — Oracles, controller, first real baseline

1. Split each challenge into `public_tests` and `hidden_tests`. The live metabolism, portfolio ranking and `failure_report` see only public tests. Today they see `scoring_benchmark()` (`crates/a2d-cli/src/main.rs:5377`). Unit tests must prove hidden tests cannot reach any arm.
2. Build `a2d lift`: arms A, B, D and C, plus `--k`, `--model` (must accept a Codex-backed model such as `codex/gpt-6-luna`; the CLI registry has no Codex provider and no coder override today, only `A2D_TESTER_PROVIDER` and `A2D_ARCHITECT_PROVIDER`), the ledger, a summary with paired confidence intervals, and result classification.
3. Add holdout loading from `$A2D_HOLDOUT_DIR` with hash verification.
4. Extend the protected paths to cover the controller and oracles.
5. **Tiny real run:** arm A on about 5 development tasks with a live, available model. Mocks miss extraction and provider failures.

You may move `lift` and the oracle code into a module or crate if `main.rs` build time slows the loop.

**Exit:** the hidden-test isolation tests pass, all four arms run end to end on mocks, and the tiny real run is recorded in the LOG.

### P1 — Calibrate a non-saturated suite

- **Model:** Luna via Codex, `gpt-6-luna`, with reasoning effort pinned in the pre-registration. If `gpt-6.1-luna` appears in Codex's model list, re-register with it rather than switching mid-experiment. If Luna saturates the pilot (B above 85%), drop to `gpt-5.6-luna` or a lower reasoning effort, recorded as a suite or model redesign under T7.
- **Suite:** fresh, bounded, std-only Rust micro-crate task families. Examples: streaming codecs and parsers, dependency resolution, a transactional key-value store, schedulers, and seeded bug-repair variants of reference crates.
- **Oracles:** validate each oracle against an independent reference implementation and against deliberately faulty mutants (an oracle that passes a mutant is rejected).
- **Plumbing and pilot only:** the Polyglot Rust subset has 30 exercises and high contamination risk, so it is not used for confirmation.
- **Optional capped feasibility pilot:** Rust-SWE-bench (arXiv 2602.22764; verify first). SWE-Bench Pro stays parked.
- **Headroom:** on the pilot, A must pass 20–70% of tasks and B must pass no more than 85%, so C has room above B. Calibrate on whole families; never select tasks by keeping individual one-shot failures.

**Exit:**
- Confirmation-1 and Confirmation-2 families are generated and validated outside the repo, and their hashes are logged.
- The non-Claude review of the suite is recorded.
- Power planning is done.

### P2 — Seed lift test

- Run arms A, B, D and C at k = 4 (pre-register any other value with its reason). Verify that every evaluated component actually executes within the budget.
- Run the pilot on development tasks, fix the primary comparator, then score Confirmation-1 once.

**Exit:** a result class is recorded. Continue to P3 regardless of the class.

### P3 — Exploratory ablation and simplification

- Measure each component's marginal effect against arm C on development tasks: evolver, architect, escalation rungs 4–6, `failure_report`, lineage topology vs seed, and portfolio (with equal resource access).
- Specify how any freed budget is reallocated, and check interactions.
- Turning a component off by default needs a recorded estimate. Deleting its code needs an upper confidence bound below meaningful effect, or a significant build-time or complexity cost.

**Exit:** a minimal configuration, recorded as the frozen harness for P4.

### P4 — Autopoietic lift

- Use the development tasks, split into train (the signal for self-modification) and validation (the signal for accepting a generation).
- Compare complete independent evolution runs against the frozen harness, and account for search cost in the budget.
- Hypotheses, in order, bounded by T7:
  1. **Harness self-modification:** the architect edits the coder harness (prompt templates, candidate generation and ranking, attempt history). This is where DGM found its gains.
  2. **Archive vs ratchet:** DGM-style parent selection from the lineage archive, against the current fitness ratchet.
  3. **Co-evolved evaluator:** RQGM-style, with the tester enzyme frozen per epoch and swapped only when it beats the incumbent on a separate labelled anchor set. Only after a fixed-oracle lift exists.

**Exit:** the best evolved harness and the frozen harness are both scored once on Confirmation-2, each against the stronger of B and D.

### P5 — Observer, offline, off the critical path

- Run `observer::observe` on Third Thoughts / Middens traces, using only features available at a prefix of the session. Validate on held-out projects and time periods.
- Pre-register: AUC of at least 0.65 and a net simulated quality/cost benefit of early kill.
- **Pass:** feed the observer real tool-event traces and run kill-and-respawn as a P3-style ablation.
- **Fail:** drop the "mechanical cortex" claim and leave the observer off.

## Completion

- **Success:** an A²D configuration, seed or evolved, is SUPPORTED against max(B, D) on a confirmation set, **and** in P4 the evolved harness is SUPPORTED against the frozen harness on Confirmation-2. Write it up in `examples/runs/` and `docs/solutions/`, then mark this plan **Completed**.
- **Lift without autopoiesis:** the seed or minimal configuration is SUPPORTED, but no P4 hypothesis is SUPPORTED within T7. Report it, then mark this plan **Completed (no autopoietic lift)**.
- **Falsified:** the confirmation results are RULED OUT for the seed or minimal configuration, and no P4 hypothesis is SUPPORTED within T7. Write a negative-result report with the evidence and a recommended pivot (for example, the research-instrument direction from the audit), then mark this plan **Completed (falsified)**. INCONCLUSIVE after the allowed extension is reported as inconclusive, not as falsified.
- **Not measured by deadline:** the P2 confirmation result is missing on 2026-11-30. Record why (build volume, blockers), with a recommendation, then mark this plan **Completed (not measured by deadline)**. A successor plan must explain how it avoids the same miss.

## Blockers

None open.

- **2026-10-14 — provider lanes (resolved by owner decision).** Kimi, Z.ai/GLM, MiniMax and Gemini subscriptions were stopped intentionally, so don't re-probe or plan around them. Perplexity's agent quota is also exhausted. Working lanes: Codex (`gpt-6.1-sol`, `gpt-6-luna`, `gpt-6-sol`, `gpt-6-astra`, `gpt-5.6-*`) and Claude.
- **Coder:** `codex/gpt-6-luna` (see Addendum).
- **Reviews:** the "≥2 non-Claude providers" rule cannot be met with one non-Claude family. Use Codex Sol plus a fresh-context Claude subagent, and record the reduced diversity.
- **Pi:** using Luna via Pi needs the `openai-codex` provider authenticated in `pi`. That's optional, because Codex already serves Luna.

## Appendix — goal prompt

The standing goal that drives this plan. If you change it, add a dated Addendum explaining why.

```text
A²D (skunkworks/a2d-autopoietic-autocatalysis-deutero): establish, with pre-registered controller-scored evidence, whether A²D builds working software more reliably than its own coder model used directly (one-shot, best-of-k, plain repair loop) at the same budget — and whether A²D's self-modification widens that margin.

Done only when docs/plans/2026-10-14-lift-first-roadmap.md is marked Completed in one of its outcomes (success / lift without autopoiesis / falsified / not measured by deadline), each backed by confirmation results in docs/experiments/LOG.md. Infrastructure is never "done" on its own.

Each session:
1. Read docs/HANDOFF.md and the roadmap. Run cargo test. Probe provider lanes (trivial prompt, short timeout).
2. Take the next experiment of the current phase. Pre-register it in LOG.md before any live run.
3. Make the smallest change that lets it run, tests first. Delegate bounded implementation to Codex. Run independent work in parallel subagents.
4. Run it through `a2d lift`, with durable artifacts under runs/. Record the result class with its CI. Negative and inconclusive results count.
5. Decide by the metric. Commit atomically with a [P<n>] tag. Overwrite HANDOFF (≤250 lines). Compound non-obvious learnings.

Rules:
- Never pause for direction. If blocked on something external (quota, credentials, access), log it under the roadmap's Blockers, switch to an unblocked track and keep going. Never build scaffolding around a blocker.
- Tripwires: a new LOG result at least every 2 sessions or 10 commits. No new redaction/sandbox/scrub work without an incident in a run artifact. Evidence must exercise the change. Budget caps on every run. At most 5 P4 hypotheses.
- Hidden tests never reach any arm or enzyme. Confirmation sets never reach you and are scored once.
- Phase gates and key designs get independent review: Codex (Sol) plus a fresh-context Claude subagent, and other model families if they become available. Run reviewers in an empty temp dir, never with the repo as cwd, and record who was consulted.
- Deutero: at each phase gate, compute process diagnostics (results per session, cost per result, tripwire hits), get them reviewed, and amend the roadmap with a dated Addendum. The loop that builds A²D is held to the same standard as A²D.
- Deadline: if the P2 seed result is not recorded by 2026-11-30, record it as not measured by deadline (a non-result, not a falsification) and complete per the roadmap.
```
