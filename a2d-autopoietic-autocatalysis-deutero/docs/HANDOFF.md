# A²D Handoff Document

**Last updated:** 2026-06-10 (session 27 — synced pickup after score-artifact replay/testable-core coverage)
**Update this document:** before context compaction, at session end, or when significant state changes.

## System State

310 commits at monorepo HEAD. Latest committed change is `Sync handoff for new pickup`. Latest A²D implementation/test change is `Cover architect protected patch routing`; latest A²D challenge/acceptance change is `Add challenge artifact scoring replay`; latest challenge replay result is `Record chess artifact replay results`. 221 tests passing (2 ignored integration) after adding holdout-backed artifact replay, CLI integration coverage, chess replay documentation, post-change escalation regression validation, and metabolism-level protected-patch routing coverage. 3 crates (a2d-core, a2d-providers, a2d-cli). 40 compound learnings.

## Clean-session pickup

### What is working

- **End-to-end metabolism:** requirements route through coder/tester/sandbox/evolver/architect with RAF checks, fitness ratchet, lineage, feedback reports, and provider circuit breaking.
- **Sudoku can reach full fitness:** latest completed `sudoku 5` live run reached best fitness 100% (6/6): no code → 83% → 100% → 100% → 100%.
- **Deutero-learning is real now, not theoretical:** the evolver accepted 6 mutations in the `sudoku 5` run; lineage-loaded germline has 7 RAF-closed enzymes.
- **Autopoiesis is gated:** architect patches go through `SystemPatch` + self-sandbox, provider cwd isolation, protected-file rejection, and explicit mechanism-file eligibility.
- **OpenCode parsing is more robust:** current text events, legacy text events, and `write` tool payloads are recovered.
- **Empty provider output is diagnosable:** `InvocationResponse` now carries optional raw provider stdout; no-materialized-output failures include sanitized parsed/raw previews, and malformed `SystemPatch` rejections include parsed artifact previews.
- **Architect can abstain explicitly:** architect output contract now accepts `{"action":"noop","reason":"..."}` as a valid no-change decision, separate from malformed/empty provider output. Legacy bare `SystemPatch` JSON remains accepted.
- **Seed-vs-evolved comparisons are mechanical:** `A2D_GERMLINE=seed` forces the hardcoded 4-enzyme seed germline instead of loading lineage.
- **Topology comparisons are now one command:** `a2d compare-topologies <challenge> <cycles>` runs seed then lineage-loaded evolved topology side by side without lineage commits or patch application, reporting best fitness, cycle-to-full-fitness, wall-clock, invocations, provider failures, caps, mutations, and patches.
- **Generated artifacts can be replayed against hidden holdouts:** `a2d score-artifact <challenge> <path|->` scores saved Rust artifacts through the same challenge helper used by live runs. `Challenge::scoring_benchmark()` now attaches hidden acceptance tests centrally, raw benchmark/acceptance fields are private, baseline/topology/provider-policy/escalation paths use the helper, diagnostics stay redacted by default, and failed replay exits 2.
- **Coder is no longer starved by speculative decomposition:** before code exists, ready invocations prioritize enzymes producing `code`; after mechanical fitness exists, feedback metabolism runs first (`enzyme_defs` → `system_patch` → `test_results` → coder retry). Any failed/killed invocation ends the current cycle before lower-priority ready enzymes run, so provider fallback happens on the next cycle instead of wasting remaining budget on auxiliary products.
- **Coder dispatch is now a fitness-scored portfolio:** coder invocations run assigned + unassigned fallback providers concurrently by default (`A2D_PARALLEL_CODER=0` disables). Every materialized code candidate is evaluated by the current benchmark/sandbox; the highest-fitness candidate is routed onward, and all candidate fitness/error records are stored in lineage and printed by topology comparison. Note: scoped threads still wait for slow losers before returning, so slow providers must be kept out of the coder portfolio.
- **Coder no longer starves feedback metabolism after success:** a successful code-producing invocation ends the cycle so benchmark feedback can become the next cycle's food. Scheduler priority is dynamic: coder first before code exists; once mechanical fitness exists, evolver → architect → tester → coder.
- **Evolver consumes mechanical fitness directly:** seed and loaded lineage germlines now use `fitness_report` as the evolver reactant; `test_results` is optional supporting evidence, not a gate. This routes sandbox outcome evidence directly into adaptation.
- **Provider health is metabolic food:** metabolism emits a mechanical `provider_health_report` artifact with unavailable providers, cooldown/failure counters, recent invocation outcomes, and coder candidate portfolio evidence. Seed/loaded germlines route it to evolver and architect so provider-role degradation is visible to the system, not just to humans reading logs.
- **Provider policy is now typed, gated, durable, comparison-gated, and live-validated:** `provider_policy` serializes active role-provider assignments; enzymes that produce it can propose assignment changes, but the metabolism accepts only known-enzyme/registered-provider changes and records accepted/rejected policy records in lineage. Current policy is routed to evolver/architect as catalyst context. Accepted non-regressing policy now must also pass a bounded current-vs-proposed provider-policy comparison before `provider-policy.json` is persisted in lineage. A live probe accepted a real runtime `provider_policy` proposal in memory, rejected durability for missing fitness evidence, and left no `.a2d/lineage/provider-policy.json`.
- **GLM is off the coder/evolver critical path, and tester/architect can now be overridden experimentally:** coder default is Kimi k2.6 with DeepSeek v4 flash fallback; evolver is explicitly assigned to Kimi k2.6 after GLM evolver timeouts starved feedback metabolism. Non-parallel evolver fallback is role-isolated, so it should not route to tester/architect GLM after Kimi/DeepSeek cooldowns. GLM 5.1 remains assigned to tester/architect by default, but `A2D_TESTER_PROVIDER` and `A2D_ARCHITECT_PROVIDER` can runtime-override those roles to registered providers after lineage policy loading, without persisting provider policy.
- **Failed rung-2 consultation is bounded:** if consultation times out/fails, the workcell fails immediately instead of spending a second full provider timeout on the primary invocation.
- **Timeouts are bounded but provider-specific:** GLM 5.1 now gets 900s by default; other CLI providers default to 300s; `A2D_PROVIDER_TIMEOUT_SECS` overrides.
- **Autopilot autonomy hardening landed and repair-diversity is live-validated through commit:** repair attempt 1 escalates from Pi to the configured alternate maintainer provider when available, repair prompts and monitor events record provider topology/attempt metadata, project state refreshes after committed iterations, and checkbox-completed todos are skipped by task selection. An opt-in fault-injection harness (`A2D_AUTOPILOT_FAULT_INJECTION=attempt0_parse_failure`) now forces a repairable parse failure for live validation. `A2D_AUTOPILOT_REPAIR_PROVIDER` / `--repair-provider` can target a specific registered repair provider. Run `run-1780125199376-0` validated Pi primary → fault-injected parse failure → DeepSeek repair output → path gate → temp `cargo test` → real-tree `cargo test` → local commit `ab43b71`.
- **Autopilot markdown outputs now have repo-reference validation:** temp-worktree validation scans markdown replacements and `handoff_update` for repo-path claims (`crates/...`, `docs/...`, `todos/...`, `examples/...`, `research/...`, and root manifest/doc files), normalizes anchors/line suffixes, and rejects paths that do not exist after patch application. This catches the exact semantic gap from `run-1780125199376-0` before real-tree apply/commit.
- **Escalation rungs 4–6 are implemented, inspectable, live-smokable, and scope-probeable:** `Metabolism::invoke_scheduled` escalates beyond prompt shaping at rungs 4+. Rung 4 performs ephemeral provider swap with history; rung 5 adds clean-session failure-context stripping; rung 6 invokes a bounded provider consensus (`A2D_RUNG6_MAX_PROVIDERS`, default 3), records candidate evaluations, selects highest code fitness under benchmarks, and falls back deterministically for non-code outputs. Default rung-6 eligibility is assigned + unassigned providers while excluding providers assigned to other enzymes; opt-in `A2D_RUNG6_PROVIDER_SCOPE=broad` includes all healthy registered providers for bounded probes. Invocation lineage, provider-health reports, topology comparison output, and `validate-escalation` JSON expose `escalation_rung`, `provider_swap`, and `clean_session`.
- **Deterministic escalation validation harness exists:** `a2d validate-escalation <challenge> [enzyme]` forces rungs 4/5/6 through a diagnostic-only in-memory API, runs the real runtime registry with persistence disabled, checks non-empty failure-history marker visibility, reports provider-policy immutability, and records rung-6 candidate evaluations.

### What is not working / unproven

- **Provider latency/quality remains bad and topology results are noisy:** 2026-05-22 bounded comparison after moving evolver to Kimi had seed 83% vs evolved 67% (`/tmp/a2d-topology-compare-sudoku3-evolver-kimi-20260522.log`). Rerun with lineage details had seed 67% vs evolved 50% and exposed a fallback leak: seed cycle 3 routed evolver to GLM (`/tmp/a2d-topology-compare-sudoku3-lineage-details-20260522.log`). After role-isolated evolver fallback, evolved beat seed 83% vs 50% and evolver stayed on Kimi (`/tmp/a2d-topology-compare-sudoku3-role-isolated-evolver-20260522.log`). Remaining slow failures were architect/tester provider windows.
- **Architect output contract is brittle:** Kimi architect still produced two no-materialized-`system_patch` failures. Need raw-output previews and/or a valid no-op patch contract.
- **Evolved topology value is unknown:** the 7-enzyme germline is RAF-closed and reached 100%, but may add latency/invocation overhead rather than useful capability.
- **Compounding self-modification is unproven:** the only live architect patch in `sudoku 5` was irrelevant (`prime.rs`) and was reverted; relevance gate now prevents that class.
- **Escalation rungs 4–6 still need quality validation, not mechanism validation:** bounded live smokes now prove provider swap, clean-session stripping, rung-6 candidate evaluation, and default-vs-broad provider eligibility under the real registry. A 30s forced-rung quality smoke was inconclusive: default timed out on Kimi+DeepSeek, broad succeeded via DeepSeek while also spending GLM+Pi timeout windows; because DeepSeek is in both scopes, this is provider variance/noise rather than evidence that broad eligibility helps. Whether rungs improve challenge outcomes and whether concurrency is worthwhile remain unproven.
- **Benchmark live coverage is still narrow:** sudoku is validated live. Chess/Rubik's hidden acceptance suites are now stronger, but post-expansion live runs have not yet proven whether providers can satisfy them.
- **Autopilot semantic validation is still partial:** markdown repo-path claims are now checked mechanically, but broader documentation/planning truth claims remain outside the gate. Mechanical path/temp/real gates plus repo-reference checks still do not validate causal correctness, performance claims, or design adequacy.
- **Provider-policy usefulness remains unproven:** the runtime/durability safety path is live-validated, but no benchmark-useful provider-policy proposal has yet shown improved challenge outcomes.

### Best next moves

1. **Use the new escalation harness as a regression lane:** run `A2D_PROVIDER_TIMEOUT_SECS=1 A2D_MAX_CYCLE_SECS=1 A2D_RUNG6_MAX_PROVIDERS=2 cargo run -q -p a2d -- validate-escalation sudoku coder` after rung/provider-routing changes.
2. **Run bounded architect/tester override comparisons:** runtime-only overrides now exist and forced-role validation can invoke tester/architect directly. Compare default GLM tester/architect against `A2D_TESTER_PROVIDER=opencode/kimi-for-coding/k2p6` and/or `A2D_ARCHITECT_PROVIDER=opencode/kimi-for-coding/k2p6` (or DeepSeek) using a direct/seeded role comparison or a challenge run that actually reaches those roles before changing defaults or durable provider policy.
3. **Only continue rung-6 quality work with replicated evidence:** a single 30s default-vs-broad forced-rung smoke is noisy. Repeat only if comparing controlled runs; otherwise avoid mechanism-only escalation work.
4. **Decide what to do with the evolved 7-enzyme topology:** latest bounded runs are noisy (seed 83/67/50 vs evolved 67/50/83). Run repeated comparisons or isolate lineage-added decomposition enzymes to distinguish topology value from provider randomness.
5. **Find a benchmark-useful provider-policy proposal path:** runtime proposal safety is validated, but the live 7-enzyme topology still has no default policy-management enzyme; add one only if bounded tests show it does not starve coder/feedback metabolism.
6. **Use replay before repeating slow chess/Rubik's smokes:** hidden acceptance suites now cover chess castling/en-passant/checkmate/legal-move invariants and Rubik's scramble-solve roundtrip. If a provider yields a saved candidate, run `a2d score-artifact chess <path>` or `a2d score-artifact rubiks <path>` to isolate holdout quality from provider latency before spending another full live challenge window.

### What works

- Catalytic cycle runs autonomously across 3 model providers (OpenCode, Gemini, Claude)
- **Feedback loop closed:** sandbox compile errors and test failures route back to coder and evolver as `failure_report` artifact. Coder prompt includes "PREVIOUS ATTEMPT FAILED" with specific errors.
- **True autopoiesis:** architect enzyme reads system source code, proposes modifications, self-sandbox validates (cargo test in temp dir), accepted patches applied to real source tree.
- **Constitutional boundary:** protected files (germline, raf, sandbox, benchmark, self_sandbox, CONSTITUTION.md) mechanically rejected before filesystem check.
- **Architect relevance gate:** automated `SystemPatch` is now restricted to explicit A²D mechanism files. Incidental domain/demo modules (`prime.rs`, `email.rs`, etc.) are excluded from architect context and patch eligibility.
- Sandbox compiles Rust code with `rustc --edition 2024` and runs tests mechanically
- Fitness ratchet prevents regressions (won't commit germline with lower fitness)
- RAF detection verifies catalytic closure after each mutation
- Lineage archive persists germline evolution in a git repo
- Challenges (sudoku, chess, rubiks) with hidden acceptance tests
- Provider subprocess timeouts — no silent hangs. GLM 5.1 gets a 15-minute default window (900s); other CLI providers default to 5 minutes (300s). Override all with `A2D_PROVIDER_TIMEOUT_SECS`.
- **Architect context pyramid:** `system_code` is now Tier 0 purpose + Tier 1 signatures by default, with full source only for files mentioned in `failure_report`; `A2D_ARCHITECT_FULL_CONTEXT=1` preserves the old full-context fallback.
- **Provider circuit breaker:** provider invocation failures/timeouts temporarily cool down the failing provider and route subsequent invocations to healthy alternatives; cooldown expiry makes the original provider eligible again.
- **CLI provider filesystem isolation:** CLI providers now run in an empty temp cwd, so coding tools cannot directly mutate the repo outside the `SystemPatch` + self-sandbox path.
- **OpenCode write-output recovery:** OpenCode NDJSON parsing now handles current `/part/text`, legacy top-level `/text`, and `write` tool payloads. If the model writes the artifact then says only `Done.`, A²D recovers the written content instead of wasting the invocation.
- **Outer autopilot loop is live for project work:** `a2d autopilot` builds project state, selects a task, invokes Pi as maintainer, requires typed `ProjectPatchset` JSON, path-gates changes, temp-worktree validates, real-tree validates, updates handoff, commits locally, and writes monitor JSONL/artifacts under `.a2d/autopilot/`.
- **Autopilot repair is bounded and provider-diverse:** parse/path/temp/real/provider failures route to repair prompts with original context, previous output, mechanical failure evidence, and provider-attempt metadata; `--repair-attempts N` / `A2D_AUTOPILOT_REPAIR_ATTEMPTS` controls budget, with repair attempt 1 using the configured alternate maintainer provider when available.
- **Provider-policy comparisons are now inspectable:** `a2d compare-provider-policy <challenge> <cycles> [policy-json|@path]` runs current and proposed policies with persistence disabled, prints policy deltas, and reports a gate decision.

### What happened this session

- **Hidden-holdout artifact replay landed.** Added `Challenge::scoring_benchmark()` and `Challenge::score_artifact()` in `crates/a2d-core/src/challenges.rs`; made raw challenge `benchmark` / `acceptance_test` private so callers cannot accidentally score visible checks only; migrated live challenge, topology comparison, provider-policy comparison, escalation validation, and baseline scoring to the central hidden-acceptance helper. Added `a2d score-artifact <challenge> <path|->` in `crates/a2d-cli/src/main.rs`; it prints case-level fitness, redacts sandbox diagnostics by default to preserve the hidden-test barrier, and exits 2 unless fitness is perfect. Documented learning: `docs/solutions/runtime-bugs/challenge-scoring-must-use-hidden-holdouts-2026-06-10.md`.
- **Score-artifact validation:** full `cargo test` passes (220 passing, 2 ignored). Negative smoke with `/tmp/a2d-bad-sudoku-artifact.rs` scored 83% (5/6): visible API/local tests passed, hidden acceptance failed `all_tests_pass`, diagnostics were captured but not printed, and shell exit status was 2. Output artifact: `/tmp/a2d-score-artifact-negative-20260610.out`; stderr artifact: `/tmp/a2d-score-artifact-negative-20260610.err`. Added CLI integration coverage in `crates/a2d-cli/tests/score_artifact.rs` for both file-path and stdin replay, including nonzero exit and diagnostic redaction assertions; the path test uses a `Drop` cleanup guard for its temp artifact.
- **Existing chess artifacts replayed against current holdouts.** `chess_engine_single.rs` scored 33% (3/9), exit 2; `chess_engine_temp.rs` scored 22% (2/9), exit 2. Both failed compilation against the current expanded chess contract before hidden behavioral quality could be assessed. Documented in `examples/runs/2026-06-10-chess-artifact-replay.md`; updated `examples/README.md` to mark the old score card as historical/pre-expanded-holdout.
- **Escalation regression rerun after score-artifact changes.** `A2D_PROVIDER_TIMEOUT_SECS=1 A2D_MAX_CYCLE_SECS=1 A2D_RUNG6_MAX_PROVIDERS=2 cargo run -q -p a2d -- validate-escalation sudoku coder` passed mechanically after challenge scoring centralization. Artifact: `/tmp/a2d-validate-escalation-post-score-artifact-20260610.json`; stderr: `/tmp/a2d-validate-escalation-post-score-artifact-20260610.err`. JSON confirmed `escalation_rung` 4/5/6, rung 4 marker visible, rung 5/6 clean-session marker stripped, provider policy unchanged, and rung 6 recorded two Kimi+DeepSeek candidate evaluations. The per-result `accepted: true` means the validation harness found an invocation record for the requested rung; it does **not** mean the provider output succeeded (the 1s provider calls intentionally timed out). Note for scripts: read `escalation_rung`, not legacy/internal `rung`.
- **Testable-core review tightened with metabolism protected-patch coverage.** Added `metabolism::tests::architect_system_patch_to_protected_file_is_rejected_through_metabolism` in `crates/a2d-core/src/metabolism.rs`. It verifies a mock architect `SystemPatch` flows through `apply_system_patch` → `self_sandbox::validate_patch`, is rejected by the protected-file gate for `crates/a2d-core/src/germline.rs`, records a patch rejection in lineage/report counters, and does not enter `pending_patches()`. Updated `todos/testable-core.md`: protected rejection through the live metabolism path is now covered; remaining architect gap is eligible non-noop patch acceptance into `pending_patches()`. Full `cargo test` passes (221 passing, 2 ignored).
- **Foundry reviewer used for replay design.** Wrote PromptEnvelope `.a2d/dispatch/session-20260610/score-artifact-reviewer.json` and dispatched via `foundry_team` without an explicit agent after `agent:"reviewer"` was unavailable. The child reviewed risks and recommended centralizing challenge scoring so replay/baseline paths cannot forget hidden acceptance tests.

- **Chess and Rubik's hidden acceptance coverage expanded.** Created completed plan `docs/plans/challenge-acceptance-test-expansion.md`. Tightened `crates/a2d-core/src/challenges.rs` API contracts for chess and Rubik's so appended holdout tests can target behavior rather than implementation guesses. Chess hidden tests now cover legal-move safety, kingside castling, en passant, and Fool's mate/no-escape. Rubik's hidden tests now cover solved initialization, rotation inverse/order invariants, known inverse roundtrip, solver-on-known-scrambles, and seeded scramble replayability/solve roundtrip. Added challenge-definition unit tests to keep these acceptance dimensions present.
- **Foundry child reviewers used for acceptance-test design.** Wrote and validated PromptEnvelope artifacts under `.a2d/dispatch/session-20260608/`, dispatched chess and Rubik's reviewer agents via `foundry_team`, and used their recommendations to shape the concrete hidden tests without leaking implementation code.
- **Validation:** pickup `cargo test` passed before changes (211 passing, 2 ignored). Post-change `cargo test -p a2d-core challenges::tests -- --nocapture` passes (2 focused tests). Full `cargo test` passes (213 passing, 2 ignored).
- **Post-expansion chess live smoke was inconclusive for holdout quality.** Ran seed `chess 1` with `A2D_PROVIDER_TIMEOUT_SECS=180` and `A2D_MAX_CYCLE_SECS=300`; coder timed out before producing code, so the expanded chess acceptance tests did not execute. Log: `/tmp/a2d-chess1-expanded-acceptance-20260609221901.log`. A separate trace-only 1s probe confirmed the coder portfolio did launch both Kimi k2p6 and DeepSeek v4 flash in parallel; both timed out under the artificial 1s bound. Log: `/tmp/a2d-chess1-expanded-acceptance-trace-20260609222406.log`. Next validation should isolate provider quality or replay candidate chess code against the hidden holdouts instead of repeating the same one-cycle smoke.

- **Rung-6 provider eligibility scope is now mechanically probeable.** Added `A2D_RUNG6_PROVIDER_SCOPE=broad` in `crates/a2d-core/src/metabolism.rs` for opt-in bounded experiments that include all healthy registered providers. The default remains assigned + unassigned providers while excluding providers assigned to other enzymes; broad scope does not mutate provider assignments or durable `provider_policy`. Added unit coverage for scope parsing and default-vs-broad eligibility.
- **Default vs broad scope smoked through the real validation harness.** Default bounded smoke (`A2D_PROVIDER_TIMEOUT_SECS=1 A2D_MAX_CYCLE_SECS=1 A2D_RUNG6_MAX_PROVIDERS=2 cargo run -q -p a2d -- validate-escalation sudoku coder`) recorded rung-6 candidates Kimi k2p6 + DeepSeek. Broad smoke (`A2D_RUNG6_PROVIDER_SCOPE=broad A2D_RUNG6_MAX_PROVIDERS=4 ...`) recorded Kimi k2p6 + DeepSeek + GLM 5.1 + Pi. JSON artifacts: `/tmp/a2d-validate-escalation-default-scope-20260605.json` and `/tmp/a2d-validate-escalation-broad-scope-20260605.json`. This validates eligibility mechanics, not outcome quality.
- **Escalation plan/todo updated.** Updated `docs/plans/escalation-rungs-4-6.md` and `todos/escalation-rungs-4-6.md` with exact default scope semantics, broad-scope probe command, and next validation targets.
- **Committed and pushed.** Scope-probe implementation committed as `0409195 Add rung 6 provider scope probe`; handoff/todo sync committed as `77707a0 docs: sync a2d handoff before push` and pushed to `origin/main`.
- **30s default-vs-broad quality smoke was inconclusive.** Default forced-rung run (`A2D_PROVIDER_TIMEOUT_SECS=30 A2D_MAX_CYCLE_SECS=1 A2D_RUNG6_MAX_PROVIDERS=2`) timed out on Kimi + DeepSeek. Broad forced-rung run (`A2D_RUNG6_PROVIDER_SCOPE=broad A2D_RUNG6_MAX_PROVIDERS=4`) timed out on Kimi, GLM, and Pi, but DeepSeek materialized a 6/6 sudoku solution. Because DeepSeek is in both scopes and differed only by stochastic provider behavior, this is not evidence that broad eligibility improves quality; it does show broad can consume extra other-role timeout windows. JSON artifacts: `/tmp/a2d-validate-escalation-default-scope-quality-20260605.json`, `/tmp/a2d-validate-escalation-broad-scope-quality-20260605.json`.
- **Validation:** initial pickup `cargo test` passed before changes (205 passing, 2 ignored). Post-change `cargo test` passes (207 passing, 2 ignored). Focused `cargo test -p a2d-core rung_6_provider_scope` passed after commit.

- **Runtime-only architect/tester provider overrides landed.** Created `docs/plans/architect-tester-provider-latency.md` and `todos/architect-tester-provider-latency.md`. Added `A2D_TESTER_PROVIDER` and `A2D_ARCHITECT_PROVIDER` overrides in `crates/a2d-cli/src/main.rs`, applied after lineage provider-policy loading so they are true runtime experiments and not durable policy. Unknown providers are rejected visibly; non-experimental roles are rejected by the same provider-policy validation gate. Defaults remain GLM for tester/architect when unset.
- **Override mechanism smoked through a real registry-building command.** Initial `status` probe was discarded because `status` does not build the runtime registry. Corrected smokes used `validate-escalation`: invalid `A2D_TESTER_PROVIDER=missing` printed visible rejection lines; valid tester+architect Kimi overrides printed accepted override lines. Artifacts: `/tmp/a2d-invalid-tester-provider-override-validate-20260605.err`, `/tmp/a2d-valid-tester-architect-provider-override-validate-20260605.err`.
- **Forced tester/architect validation now reaches the intended roles.** `validate-escalation` now uses a validation-only single-enzyme germline plus non-empty seeded inputs for tester/architect/evolver. This avoids coder/evolver priority starving diagnostic role validation. 10s forced-role smokes invoked tester/architect directly (`/tmp/a2d-validate-tester-default-20260605.json`, `/tmp/a2d-validate-tester-kimi-20260605.json`, `/tmp/a2d-validate-architect-kimi-20260605.json`) but timed out, so they validate mechanism not quality.
- **Latency comparison attempts remained inconclusive.** `compare-topologies sudoku 2` with 20s provider bounds did not reach tester/architect because coder timed out first in both default and override runs. Forced tester validation with 30s bounds produced valid JSON for default and Kimi override paths, but every provider candidate timed out: default rungs 4/5 swapped to Kimi and rung 6 used GLM; Kimi override rungs 4/5 swapped to DeepSeek and rung 6 used Kimi. This validates routing/override mechanics but gives no quality evidence for changing defaults. Logs/artifacts: `/tmp/a2d-latency-default-glm-tester-architect-20260605.log`, `/tmp/a2d-latency-kimi-tester-architect-20260605.log`, `/tmp/a2d-validate-tester-default-30s-20260605.json`, `/tmp/a2d-validate-tester-kimi-30s-20260605.json`.
- **Validation:** `cargo test` passes (211 passing, 2 ignored). Focused override tests pass: `cargo test -p a2d runtime_provider_overrides`; default registry test passes: `cargo test -p a2d live_registry_keeps_glm_off_coder_and_evolver_critical_path`; diagnostic isolation test passes: `cargo test -p a2d escalation_validation_germline_isolates_requested_enzyme`.

- **Deterministic escalation validation harness landed.** Added diagnostic-only `Metabolism::force_escalation_rung_for_validation()` for rungs 4–6 and CLI `a2d validate-escalation <challenge> [enzyme]`. The command runs fresh real-registry metabolisms with persistence disabled, emits JSON using the external `escalation_rung` contract, checks non-empty failure-history marker visibility, records rung-6 candidate evaluations, and reports provider-policy immutability.
- **Bounded live escalation smoke passed mechanically.** `A2D_PROVIDER_TIMEOUT_SECS=1 A2D_MAX_CYCLE_SECS=1 A2D_RUNG6_MAX_PROVIDERS=2 cargo run -q -p a2d -- validate-escalation sudoku coder` proved rung 4 provider swap preserved the seeded failure marker, rung 5 clean session stripped it, and rung 6 recorded two Kimi + DeepSeek candidate evaluations. The provider calls timed out under the intentional 1s bound; this validates mechanism/observability, not outcome quality.
- **Escalation docs and field contract cleaned up.** Updated `docs/plans/escalation-rungs-4-6.md`, `todos/escalation-rungs-4-6.md`, and escalation solution notes to treat internal counter names as implementation details and external reports as `escalation_rung`-based. Added learning `docs/solutions/best-practices/escalation-validation-harness-2026-06-04.md`.
- **Validation:** `cargo test` passes (205 passing, 2 ignored).

- **Escalation rung 6 bounded provider consensus landed.** `Metabolism::invoke_scheduled` now routes escalation rung 6 through a bounded role-isolated provider portfolio (`A2D_RUNG6_MAX_PROVIDERS`, default 3). Candidate outputs are materialized and recorded in lineage; code candidates are selected by benchmark fitness when available; non-code candidates use deterministic fallback (first materialized success → first success → first error). Shared candidate-selection logic now backs both the existing coder portfolio and rung 6. Documented learning: `docs/solutions/architectural-insights/escalation-rung-6-bounded-provider-consensus-2026-06-01.md`.
- **Escalation plan/todo completed for rungs 4–6.** Updated `docs/plans/escalation-rungs-4-6.md` and `todos/escalation-rungs-4-6.md`: rung 6 is implemented; next work is bounded live validation and eligibility/concurrency tradeoff evaluation. **Superseded 2026-06-04:** bounded live mechanism validation now exists via `validate-escalation`; remaining work is quality/eligibility evaluation.
- **Validation:** post-rung-6 `cargo test` passes (200 passing, 2 ignored).

- **Escalation rung 5 lineage/prompt clarity landed.** `InvocationLineage` now records `escalation_rung`, `provider_swap`, and `clean_session`; clean-session invocations record provider-visible inputs after `failure_report` stripping; provider-health `recent_invocations` carries the same escalation fields; and topology comparison output prints flags such as `{rung 5, swap, clean}`. Added explicit rung-5 tests for swapped-provider routing, clean-session lineage, and prompt contents. Documented learning: `docs/solutions/architectural-insights/escalation-rung-5-clean-swapped-session-lineage-2026-06-01.md`.
- **Escalation plan/todo updated for rung 5.** Updated `docs/plans/escalation-rungs-4-6.md` and `todos/escalation-rungs-4-6.md`: rung 5 became implemented; at that point rung 6 multi-model consensus remained next. **Superseded later this session by rung 6 implementation above.**
- **Validation:** initial pickup `cargo test` passed before changes (195 passing, 2 ignored). Post-change `cargo test` passes (198 passing, 2 ignored).

- **Escalation rung 4 landed.** Added ephemeral provider-swap helpers in `crates/a2d-core/src/provider.rs` and wired `Metabolism::invoke_scheduled` in `crates/a2d-core/src/metabolism.rs` so escalation rung 4 routes the current invocation to a non-assigned provider without mutating assignments or durable `provider_policy`. Rung 4 preserves failure history; rung 5+ strips failure context. Consultation is skipped at rung 4+ because the alternate provider is now the primary intervention. Added mock coverage for swap firing, non-firing below threshold, reset returning to assigned provider, prompt contents, assignment immutability, and role-isolated swaps.
- **Escalation plan/todo updated.** Created `docs/plans/escalation-rungs-4-6.md`, updated `todos/escalation-rungs-4-6.md`, and documented learning `docs/solutions/architectural-insights/escalation-rung-4-ephemeral-provider-swap-2026-05-31.md`.
- **Validation:** `cargo test` passes (195 passing, 2 ignored). `cargo run -q -p a2d -- status` confirms lineage-loaded 7-enzyme RAF remains 100% closed.

- **Autopilot markdown repo-reference validation landed.** Temp-worktree validation now scans markdown replacements and `handoff_update` for repo-path references, normalizes anchors/line suffixes, and rejects invented or unsafe repo paths before real-tree apply/commit. Maintainer and repair prompts now warn that absent `crates/...`, `docs/...`, `todos/...`, `examples/...`, and `research/...` claims fail validation. Added unit coverage for the previous failure shape (`metabolism_workcell.rs`, `provider_registry.rs`) and an accepted existing-path case. Documented learning: `docs/solutions/runtime-bugs/autopilot-markdown-reference-validation-2026-05-31.md`.
- **Validation:** initial pickup `cargo test` passed before changes (187 passing, 2 ignored). Post-change `cargo test` passes (189 passing, 2 ignored). `cargo run -q -p a2d -- autopilot --iterations 1 --dry-run` correctly stopped on dirty tree; `--dry-run --allow-dirty` built project state, selected `todos/escalation-rungs-4-6.md`, emitted monitor artifacts, and stopped before provider invocation.

- **Autopilot repair prompt tightened against empty patchsets.** Maintainer and repair prompts now explicitly state that `replacements` must be non-empty, that empty patchsets fail the path gate, and that docs/todo/plan work should update the selected markdown file or another approved markdown file with complete content. Added unit coverage (`maintainer_prompt_forbids_empty_replacements`) and strengthened repair prompt assertions.
- **Alternate-provider repair path reached commit gate.** Ran `A2D_AUTOPILOT_FAULT_INJECTION=attempt0_parse_failure A2D_AUTOPILOT_REPAIR_PROVIDER=opencode/opencode/deepseek-v4-flash-free A2D_PROVIDER_TIMEOUT_SECS=180 cargo run -q -p a2d -- autopilot --iterations 1 --repair-attempts 1`. Attempt 0 invoked `pi/default`, fault injection forced parse failure, repair attempt 1 used DeepSeek, returned a typed patchset with one replacement, passed path gate, passed temp `cargo test`, passed real-tree `cargo test`, and committed `ab43b71`. Monitor run: `.a2d/autopilot/runs/run-1780125199376-0/`; console log: `/tmp/a2d-autopilot-repair-deepseek-tightprompt-20260530071316.log`.
- **Autopilot semantic gap found and corrected manually.** The committed todo update named non-existent files (`metabolism_workcell.rs`, `provider_registry.rs`). Corrected `todos/escalation-rungs-4-6.md` to point to actual mechanism files: `crates/a2d-core/src/metabolism.rs` and `crates/a2d-core/src/provider.rs`. This shows mechanical gates validate parse/path/tests/commit, not semantic truth of planning claims.
- **Validation:** `cargo test` passes (187 passing, 2 ignored).

- **Autopilot repair-diversity fault injection landed.** Added opt-in `A2D_AUTOPILOT_FAULT_INJECTION=attempt0_parse_failure`, which preserves the live provider call but replaces attempt 0's parsed output with malformed `ProjectPatchset` text after provider return. The monitor logs an `autopilot_fault_injected` event with provider, attempt, fault, and original output size. Added unit coverage for attempt-scoped fault selection.
- **Live Pi → alternate-provider repair routing validated.** Ran `A2D_AUTOPILOT_FAULT_INJECTION=attempt0_parse_failure A2D_PROVIDER_TIMEOUT_SECS=90 cargo run -q -p a2d -- autopilot --iterations 1 --repair-attempts 1`. Attempt 0 invoked `pi/default`; fault injection forced `patchset_parse_failed`; repair attempt 1 escalated to `opencode/kimi-for-coding/k2p6` with `escalated: true` and primary/topology metadata; Kimi timed out after 90s; `repair_budget_exhausted` stopped cleanly with no partial apply/commit. Monitor run: `.a2d/autopilot/runs/run-1780061191713-0/`; console log: `/tmp/a2d-autopilot-repair-diversity-20260529132612.log`.
- **Learning documented:** `docs/solutions/runtime-bugs/autopilot-repair-diversity-live-validation-2026-05-29.md`. Updated `todos/autonomous-project-loop.md` and `docs/plans/autonomous-project-loop.md`. Important remaining gap: successful alternate-provider repair output has not passed normal gates because Kimi timed out.
- **Configurable autopilot repair provider landed.** Added `ProviderRegistry::provider_named`, `A2D_AUTOPILOT_REPAIR_PROVIDER`, and `a2d autopilot --repair-provider <registered-provider-name>`. Repair attempt 1 now uses the configured registered provider when it differs from the primary; otherwise it falls back to the previous alternate-provider behavior. Monitor logs include `configured_repair_provider`. Unit coverage: provider lookup, CLI flag/env plumbing, and configured DeepSeek repair selection.
- **Configured DeepSeek repair probes ran.** `run-1780062413070-0` with 120s timeout: primary Pi timed out, repair attempt 1 routed to `opencode/opencode/deepseek-v4-flash-free`, DeepSeek returned a typed patchset with zero replacements, and the path gate rejected it cleanly. `run-1780062590484-0` with 300s timeout: primary Pi returned, fault injection forced parse failure, repair attempt 1 routed to DeepSeek, then DeepSeek timed out. No partial apply/commit occurred.
- **Validation:** `cargo test` passes (186 passing, 2 ignored).

- **Provider-policy runtime proposal gate live validation completed.** Temporarily installed a probe lineage germline whose `maintainer` enzyme produced a real `provider_policy` artifact via `pi/default`, proposing `maintainer: pi/default -> opencode/kimi-for-coding/k2p6`. Runtime accepted the policy in memory (`Provider policy accepted: 1`), ran the bounded current-vs-proposed durability gate, rejected lineage persistence for missing fitness evidence, and left no `.a2d/lineage/provider-policy.json`. Log: `/tmp/a2d-provider-policy-runtime-proposal-20260528162751.log`.
- **Provider-policy topology-gate todo/plan completed.** Updated `todos/provider-policy-topology-gate.md` and `docs/plans/provider-policy-topology-gate.md`; added learning `docs/solutions/architectural-insights/provider-policy-runtime-proposals-must-stay-comparison-gated-2026-05-28.md`.
- **Validation:** `cargo test` passes (183 passing, 2 ignored). Also ran `cargo run -q -p a2d -- status` and a bounded explicit `compare-provider-policy sudoku 1` proposal; the latter rejected a slower proposed policy and confirmed no provider-policy lineage file was written.

- **Provider-policy topology gate landed.** Added `docs/plans/provider-policy-topology-gate.md`, `a2d compare-provider-policy`, current-vs-proposed policy comparison runs, deterministic gate decisions, and `commit_provider_policy_if_gate_accepts`. Runtime provider-policy lineage persistence now requires bounded comparison evidence after in-memory schema/provider/enzyme acceptance and non-regression. Gate rejects missing fitness evidence, worse best fitness, zero-fitness inconclusive comparisons, material invocation increases, and material wall-clock increases. Provider-policy snapshots are filtered to current germline enzymes so outer-loop-only assignments such as `maintainer` are not durably written as challenge-metabolism policy.
- **Provider-policy gate validation:** `cargo test` passes (183 passing, 2 ignored). CLI smoke passed: `A2D_PROVIDER_TIMEOUT_SECS=1 A2D_MAX_CYCLE_SECS=1 cargo run -q -p a2d -- compare-provider-policy sudoku 1` printed `current`/`proposed` policy modes, `no assignment changes`, and rejected due missing fitness evidence without lineage persistence.
- **Autopilot autonomy hardening remains landed.** `a2d autopilot` records maintainer provider topology, sends provider-attempt metadata in repair prompts, routes repair attempt 1 to the configured alternate maintainer provider when one exists (Pi primary → Kimi alternate in the current registry), refreshes `project_state` after commits, and skips checkbox-completed todos.

- **Autonomous project loop gap captured and corrected toward self-modification.** The missing loop is now explicit: A²D has an inner challenge metabolism, but not the outer repo-maintenance loop currently performed by the human/coding assistant (`read handoff/todos → choose task → self-modify code/docs → run gates → repair/escalate → commit → update handoff → repeat`). Added plan `docs/plans/autonomous-project-loop.md` and todo `todos/autonomous-project-loop.md` for a bounded `a2d autopilot` command with typed `project_state`, `project_task`, `project_patchset`, validation reports, repair/escalation, handoff, and commit gates. Important correction: self-modification is the target behavior, not a non-goal; the safety property is gated self-modification, not absence of self-modification.
- **First executable autopilot surface landed.** `a2d autopilot` now parses `--iterations`, `--dry-run`, and `--allow-dirty`; builds `ProjectState` from handoff/todos/plans/git/status; selects `todos/autonomous-project-loop.md` first; emits a maintainer prompt that explicitly allows eligible source self-modification; defines typed `ProjectPatchset` JSON; parses fenced JSON; and path-gates patchsets, rejecting traversal/protected files while accepting eligible source self-modification with required cargo-test gating. Non-dry-run can invoke the maintainer provider and gate the returned patchset, but temp-worktree validation/application/repair/commit remains the next slice.
- **Autopilot monitor logs landed.** Every autopilot run now emits aggregate and per-run JSONL logs under `.a2d/autopilot/`, plus prompt/output artifacts under `.a2d/autopilot/runs/<run-id>/`. Events include run start, project-state collection, task selection, maintainer prompt construction, provider invocation/output, patchset parse failures, path-gate outcomes, temp-worktree validation outcomes, and stop reasons. This gives an external monitor/steerer enough evidence to evaluate both model outputs and mechanical outcomes.
- **Autopilot temp-worktree validation landed.** After maintainer output parses and passes the path gate, autopilot copies the project to a temp worktree, applies replacements there, injects `cargo test` for eligible source self-modification, runs only allowlisted validation commands, writes `validation-report.json`, and logs `temp_worktree_validation_completed`.
- **Autopilot real-tree apply/commit gate landed.** Patchsets that pass temp validation are applied to the real tree, validation commands rerun, `handoff_update` is appended to `docs/HANDOFF.md` when no explicit handoff replacement is present, touched paths are `git add`ed and committed locally, and validation/commit failures restore original file contents plus reset touched paths. Events/artifacts: `real_tree_apply_started`, `real_tree_apply_completed`, `apply-report.json`. Git dirtiness is scoped to the current project path (`git status --short -- .`) so sibling workspace changes do not block A²D autopilot.
- **First non-dry autopilot run exposed a provider-context/provider-choice gap.** Run `run-1779708405080-0` invoked Kimi/OpenCode on `todos/autonomous-project-loop.md`; the provider ran in isolated cwd, tried to read repo files with tools, got denied/not found, and returned empty parsed output. Monitor logs captured raw output (`raw_output_bytes=15409`) and `patchset_parse_failed`. A prompt/context fix alone was insufficient: run `run-1779708596310-0` still tried tool reads and returned empty output.
- **Maintainer moved to Pi.** Added a `CliProvider::pi` integration and assigned the outer-loop `maintainer` role to `pi/default` instead of falling through to OpenCode/Kimi. Pi is invoked in non-interactive ephemeral artifact mode (`--print --no-session --no-tools --no-context-files --no-extensions --no-skills --no-prompt-templates --no-themes`) so it emits a typed patchset while A²D remains responsible for gated application. Maintainer prompt still includes full selected task content.
- **Pi maintainer live validation passed.** Non-dry `cargo run -q -p a2d -- autopilot --iterations 1` on this project selected `todos/autonomous-project-loop.md`, invoked `pi/default`, produced a valid patchset, passed temp `cargo test`, passed real `cargo test`, and committed `dc864dd Autopilot: capture bounded repair contract`. Monitor run: `.a2d/autopilot/runs/run-1779711206673-0/`.
- **Autopilot bounded repair loop landed.** `a2d autopilot` now supports `--repair-attempts N` / `A2D_AUTOPILOT_REPAIR_ATTEMPTS` (default 1). Parse failures, path-gate rejections, temp-worktree validation failures, real-tree validation/apply failures, and provider invocation failures are converted into repair prompts containing the original task/context, previous output, and mechanical failure report. Repair attempts are logged with `repair_attempt_started`, `repair_output_received`, and `repair_budget_exhausted`; per-attempt artifacts live under `iteration-N/attempt-M/`.
- **Repair-enabled autopilot live validation passed.** Non-dry `cargo run -q -p a2d -- autopilot --iterations 1 --repair-attempts 1` selected `todos/autonomous-project-loop.md`, invoked `pi/default`, did not need repair because attempt 0 passed, temp/real `cargo test` passed, and autopilot committed `0c3fc0d Autopilot: capture provider-diverse repair escalation contract`. Monitor run: `.a2d/autopilot/runs/run-1779713261084-0/`.
- **Validation:** latest full `cargo test` passes (178 passing, 2 ignored).

- **Provider policy lineage persistence landed.** `LineageArchive` now reads/writes `provider-policy.json` beside `germline.json`; normal runtime loads persisted provider policy through the same mechanical gate used for model-proposed policy artifacts; accepted non-regressing provider-policy changes are committed to lineage. `A2D_GERMLINE=seed` bypasses persisted policy, while topology comparison keeps seed on defaults and lets evolved mode include lineage policy. Plan updated: `docs/plans/provider-policy-artifact.md`. Learning: `docs/solutions/architectural-insights/provider-policy-lineage-persistence-2026-05-23.md`.
- **Validation:** `cargo test` passes (163 passing, 2 ignored). `cargo run -q -p a2d -- status` confirms lineage-loaded 7-enzyme RAF remains 100% closed.
- **Known follow-up:** provider-policy durability is still gated only by schema/provider/enzyme validation plus non-regression; durable policy should next be gated with bounded topology comparisons.

- **Provider policy artifact landed.** Added typed `ProviderPolicy` (`assignments: enzyme → provider`), `ProviderRegistry::current_policy/apply_policy`, `provider_policy` artifact sync, in-metabolism mechanical gating, provider-policy lineage records, and cycle counters. Seed/loaded germlines now include `provider_policy` as food/catalyst context for evolver and architect. Plan: `docs/plans/provider-policy-artifact.md`. Learning: `docs/solutions/architectural-insights/provider-policy-must-be-gated-metabolic-mechanism-2026-05-22.md`.
- **Validation:** `cargo test` passes (160 passing, 2 ignored). `cargo run -q -p a2d -- status` confirms lineage-loaded 7-enzyme RAF remains 100% closed.

- **Evolver moved off GLM.** Live registry now assigns evolver to Kimi k2.6 while keeping tester/architect on GLM 5.1. Unit coverage renamed/expanded to `live_registry_keeps_glm_off_coder_and_evolver_critical_path`. Documented in `docs/solutions/runtime-bugs/evolver-glm-critical-path-timeouts-2026-05-22.md`.
- **Bounded topology validation completed after evolver provider change.** `A2D_PROVIDER_TIMEOUT_SECS=90 A2D_MAX_CYCLE_SECS=300 cargo run -p a2d -- compare-topologies sudoku 3` completed. Seed best: 83% (5/6), 337.0s, 5 invocations, 2 failures. Evolved best: 67% (4/6), 270.4s, 4 invocations, 2 failures. Log: `/tmp/a2d-topology-compare-sudoku3-evolver-kimi-20260522.log`. The result removed the known GLM-evolver assignment bottleneck but did not validate evolved-topology value.
- **Topology comparison now prints per-invocation lineage details.** Each cycle lists `[enzyme via provider] OK/FAIL/KILL` with single-line truncated errors before coder candidate portfolios. This makes bounded comparisons actionable without `A2D_TRACE=1`. Smoke log: `/tmp/a2d-topology-compare-sudoku1-lineage-details-smoke-20260522.log`. Updated `docs/solutions/best-practices/topology-comparison-harness-2026-05-20.md`.
- **Lineage-details rerun exposed and fixed an evolver fallback leak.** `A2D_PROVIDER_TIMEOUT_SECS=90 A2D_MAX_CYCLE_SECS=300 cargo run -p a2d -- compare-topologies sudoku 3` with details had seed best 67% and evolved best 50%. It showed seed cycle 3 routing `evolver` to GLM after Kimi/DeepSeek cooldowns. Provider registry now has role-isolated fallback; non-parallel evolver uses it and will fail on assigned Kimi rather than consuming tester/architect GLM. Log: `/tmp/a2d-topology-compare-sudoku3-lineage-details-20260522.log`.
- **Role-isolated evolver fallback live validation passed.** Bounded rerun had seed best 50% (3/6), evolved best 83% (5/6). Evolver invocations stayed on Kimi k2.6 in seed cycles 2–3 and evolved cycle 3; no evolver→GLM fallback occurred. Remaining failures were architect GLM timeout and tester Kimi timeout. Log: `/tmp/a2d-topology-compare-sudoku3-role-isolated-evolver-20260522.log`.
- **Provider health became metabolic food.** The metabolism now emits `provider_health_report` as mechanical JSON; seed and loaded germlines include it as food and route it as a catalyst to evolver/architect. Specialized prompts include provider health when available, closing `provider failures/timeouts → provider_health_report → evolver/architect`. Documented in `docs/solutions/architectural-insights/provider-health-must-be-metabolic-food-2026-05-22.md`. `cargo run -p a2d -- status` confirmed loaded 7-enzyme RAF remains 100% closed.
- Full `cargo test` passes: 157 passing, 2 ignored.

- **Provider empty-output diagnostics landed.** `InvocationResponse` now preserves optional raw stdout from CLI providers. No-materialized-output workcell failures include sanitized parsed/raw previews, and malformed `SystemPatch` rejections include parsed artifact previews. Documented in `docs/solutions/runtime-bugs/provider-empty-output-diagnostics-2026-05-16.md`.
- **Architect no-op contract landed.** Architect prompt now admits `{"action":"patch", ...}` and `{"action":"noop", "reason":"..."}`; no-op records `NOOP: <reason>` without self-sandbox or rejection, while legacy bare `SystemPatch` remains accepted. Documented in `docs/solutions/runtime-bugs/architect-noop-contract-2026-05-16.md`.
- **GLM 900s validation completed and failed usefully.** `A2D_TRACE=1 cargo run -p a2d -- challenge sudoku 1` with lineage-loaded 7-enzyme topology invoked `analyze_requirements` first; GLM timed out after 900s, the cycle wall-clock-capped before coder ran, and best fitness was 0%. Log: `/tmp/a2d-sudoku1-20260518-after-noop.log`. This shows the longer GLM window is not enough when slow evolved decomposition enzymes can starve coder.
- **Seed germline switch landed.** `A2D_GERMLINE=seed` forces the hardcoded 4-enzyme topology even when lineage exists. Verified with `A2D_GERMLINE=seed cargo run -p a2d -- status`: RAF 100%, closed, 4 enzymes.
- **Scheduler priority fix landed.** Ready invocations now prioritize direct artifact progress: `code` → `test_results` → `enzyme_defs` → `system_patch` → auxiliary products. Smoke validation with `A2D_TRACE=1 A2D_PROVIDER_TIMEOUT_SECS=1 A2D_MAX_CYCLE_SECS=1 cargo run -p a2d -- challenge sudoku 1` showed ready order `["coder", "analyze_requirements"]` and invoked coder first. Log: `/tmp/a2d-sudoku1-20260518-priority-smoke.log`.
- **Fail-fast cycle advance landed.** Failed/killed invocations now end the current cycle before lower-priority ready enzymes execute. Two-cycle smoke with `A2D_PROVIDER_TIMEOUT_SECS=1` showed cycle 1 GLM coder timeout, then cycle 2 coder routed to Kimi fallback; no auxiliary decomposition ran after coder failure. Log: `/tmp/a2d-sudoku2-20260518-fallback-smoke.log`.
- **Bounded 60s post-fix validation:** `A2D_TRACE=1 A2D_PROVIDER_TIMEOUT_SECS=60 A2D_MAX_CYCLE_SECS=180 cargo run -p a2d -- challenge sudoku 2` spent cycle 1 on GLM coder timeout and cycle 2 on Kimi coder timeout; no lower-priority decomposition ran after coder failures. Best fitness 0%, but budget was spent on the critical enzyme/fallback path. Log: `/tmp/a2d-sudoku2-20260518-evolved-failfast-60s.log`.
- **Parallel coder race landed.** Coder now invokes assigned + unassigned fallback providers concurrently; role-specific tester/evolver providers are excluded. Documented in `docs/solutions/best-practices/parallel-cheap-coder-race-2026-05-19.md`. 60s live smoke showed GLM and Kimi spawned concurrently and both timed out in ~60s, proving wall-clock is max(timeout) rather than serial sum(timeout). Log: `/tmp/a2d-sudoku1-20260519-parallel-coder-60s.log`.
- **Topology comparison harness landed.** New CLI command: `a2d compare-topologies <challenge> <cycles>` (alias `benchmark-topologies`). It runs seed and lineage-loaded evolved topologies in one process, disables lineage commits and patch application, and prints side-by-side fitness/cycle/wall-clock/invocation/provider-failure/cap/mutation/patch metrics. Documented in `docs/solutions/best-practices/topology-comparison-harness-2026-05-20.md`.
- **Harness smoke validation completed:** `A2D_TRACE=1 A2D_PROVIDER_TIMEOUT_SECS=1 A2D_MAX_CYCLE_SECS=1 cargo run -p a2d -- compare-topologies sudoku 1` ran seed with 4 enzymes and evolved with 7 enzymes. Both prioritized coder first and raced GLM/Kimi in parallel; both timed out as expected under 1s provider budgets. No lineage commits or patches applied.
- **Real topology comparison partially completed and found runtime bugs:** `A2D_TRACE=1 A2D_PROVIDER_TIMEOUT_SECS=180 A2D_MAX_CYCLE_SECS=240 cargo run -p a2d -- compare-topologies sudoku 3` ran seed to 0% (three coder failures) and evolved to 67% in cycle 1 via Kimi fallback before the outer command timed out in evolved cycle 3. Log: `/tmp/a2d-topology-compare-sudoku3-20260520.log`.
- **Failed-consultation double-timeout fixed:** rung-2 consultation failure now fails the workcell immediately instead of invoking primary afterward. Documented in `docs/solutions/runtime-bugs/failed-consultation-double-timeout-2026-05-20.md`. 5s live smoke confirmed cycle 3 spends one timeout, not two. Log: `/tmp/a2d-topology-compare-sudoku3-consultfail-5s-20260520.log`.
- **Coder provider assignment changed twice based on live data:** GLM is assigned to tester/evolver/architect and excluded from coder races. MiniMax highspeed + Kimi k2.5 still timed out under 60s, so direct provider smokes selected Kimi k2.6 + DeepSeek v4 flash as the coder race pool. Documented in `docs/solutions/runtime-bugs/coder-glm-critical-path-timeouts-2026-05-20.md`. One-invocation topology smoke: seed 0%, evolved 67% (4/6). Log: `/tmp/a2d-topology-compare-sudoku1-fastpool-oneinvoke-20260520.log`.
- **Fitness-scored coder portfolio landed:** parallel coder dispatch now evaluates every materialized code candidate with the hidden benchmark/sandbox and selects by highest fitness, not first materialized/provider order. Candidate provider/materialization/error/fitness records are attached to invocation lineage and printed by topology comparison. Documented in `docs/solutions/best-practices/fitness-scored-coder-portfolio-2026-05-20.md`. Smoke log: `/tmp/a2d-topology-compare-sudoku1-portfolio-smoke-20260520.log`.
- **Portfolio topology comparison completed:** `A2D_PROVIDER_TIMEOUT_SECS=180 A2D_MAX_CYCLE_SECS=300 cargo run -p a2d -- compare-topologies sudoku 3` completed. Seed best 67% in ~930s; evolved best 67% in ~810s. Exposed same-cycle coder retry starvation. Log: `/tmp/a2d-topology-compare-sudoku3-portfolio-20260520.log`.
- **Scheduler feedback-metabolism fix landed:** successful code production advances the cycle and dynamic priority puts tester/evolver/architect before coder once code exists. Documented in `docs/solutions/runtime-bugs/coder-retry-starves-feedback-metabolism-2026-05-21.md`. One-cycle live validation: seed 83%, evolved 67%, no same-cycle coder retry or stale auxiliary after code. Log: `/tmp/a2d-topology-compare-sudoku1-cycleadvance-20260521.log`.
- **Mechanical-fitness evolver landed:** evolver reactant is now `fitness_report`, loaded lineage is normalized, and scheduling prioritizes evolver before tester once fitness exists. Documented in `docs/solutions/runtime-bugs/evolver-must-consume-mechanical-fitness-2026-05-21.md`. Live `sudoku 2` validation: seed cycle 2 ready order `evolver`, `architect`, `tester`, `coder`; seed evolver timed out on GLM; evolved reached 100% at cycle 2. Log: `/tmp/a2d-topology-compare-sudoku2-mechanical-evolver-20260521.log`.
- Full `cargo test` passes: 154 passing, 2 ignored.

### What happened in prior sessions

- **Architect context pyramid landed.** `format_system_code_snapshot` no longer concatenates every modifiable source file by default. It emits one-line purpose + signatures for every file and appends full source only for files named by `failure_report` (for example `metabolism.rs:510`). Old behavior remains available with `A2D_ARCHITECT_FULL_CONTEXT=1`.
- **Tests added:** two unit tests cover default body elision and failure-targeted full-source inclusion.
- **Live validation partially completed:** `a2d challenge sudoku 1` with `A2D_TRACE=1` measured `system_code snapshot = 15 files, 17,702 bytes` and architect Gemini prompt args at 37,533 bytes — well under the <60 KB target and far below the prior ~400 KB prompt.
- **Latency did not improve enough:** first architect invocation still took 272.95 s and failed; tester Gemini also took 274.60 s and failed; the run hit the harness 900 s timeout during a second architect invocation. Context size is fixed, but Gemini CLI latency/failure remains a bottleneck.
- **Runtime panic fixed:** live run exposed a Unicode char-boundary panic in `strip_module` when coder output contained `×`; fixed by iterating with `char_indices` and added a regression test.
- **Provider timeout restored:** CLI provider default timeout was restored to 300 s (override via `A2D_PROVIDER_TIMEOUT_SECS`), matching the then-current 5-minute policy. Superseded for GLM by the later 900s default.
- **Cycle wall-clock budget added and live-validated:** `Metabolism` now has a default 600 s between-invocation wall-clock budget and reports `CycleReport.wall_clock_capped`; CLI override is `A2D_MAX_CYCLE_SECS` (`0` disables for explicit experiments). Full `sudoku 1` now completes instead of hitting the 900 s harness timeout: 4 invocations, `[wall-clock-capped]`, Fitness 100% (6/6). In-flight provider calls are still governed by provider timeout.
- **Provider circuit breaker added:** provider failures now set a temporary provider-level cooldown (default 600 s, exponential backoff to 3600 s, `A2D_PROVIDER_COOLDOWN_SECS` override). Subsequent invocations avoid cooled-down providers, but the provider is retried after cooldown instead of permanently banned. Documented in `docs/solutions/runtime-bugs/provider-circuit-breaker-temporary-cooldown-2026-04-23.md`.
- Full `cargo test` passes: 128 passing, 2 ignored.
- **Live circuit-breaker probe:** `A2D_TRACE=1 cargo run -p a2d -- challenge sudoku 1` confirmed provider failures now record cooldowns (`provider failure: gemini/... → cooldown 600s`). The run did not reach a subsequent Gemini-assigned tester/architect reroute because the second Kimi coder invocation timed out after 300 s and the 600 s cycle wall-clock cap fired first. Fitness was 83% (5/6), wall-clock-capped.
- **Default provider switched from Gemini/Kimi mix to GLM 5.1.** Gemini is intentionally not registered in the default live CLI registry for now because quota failures consume ~5-minute timeout windows before the circuit breaker can help later invocations. Coder, tester, evolver, and architect now use GLM 5.1 by default; Kimi remains registered only as a fallback alternative if GLM is cooled down.
- **Boundary violation found and fixed:** GLM/OpenCode modified `crates/a2d-core/src/challenges.rs` directly during provider execution even though the cycle reported `0 patches`. The change was reverted and CLI providers now run in isolated temp directories. See `docs/solutions/runtime-bugs/cli-providers-must-run-in-isolated-cwd-2026-04-28.md`.
- **Empty provider output now fails:** GLM/OpenCode can return successfully with no parsed text/output under isolated cwd. A successful provider call that materializes no required products is now a failed invocation and opens provider cooldown instead of silently producing no fitness signal.
- **All-GLM isolated live run completed cleanly:** `A2D_TRACE=1 cargo run -p a2d -- challenge sudoku 1` with GLM default + isolated cwd completed with 6 invocations, `[wall-clock-capped]`, Fitness 100% (6/6), and clean `git status`. Circuit breaker rerouting was observed live: GLM architect timeout cooled down GLM, then coder/tester/architect routed to Kimi fallback.
- **OpenCode parser hardened:** provider parser now recovers artifact content from OpenCode `write` tool NDJSON when final assistant text is empty/generic (`Done.`), while still preferring substantive final text. Added 4 provider unit tests for current text shape, legacy text shape, write fallback, and final-text precedence. Documented in `docs/solutions/runtime-bugs/opencode-write-tool-output-recovery-2026-05-01.md`.
- **`sudoku 5` live validation completed:** `A2D_TRACE=1 cargo run -p a2d -- challenge sudoku 5` finished all 5 cycles. Best fitness: 100% (6/6). Fitness curve: cycle 1 no code (GLM coder timeout), cycle 2 83% (5/6), cycles 3–5 100% (6/6). Log: `/tmp/a2d-sudoku5-20260501.log`.
- **Deutero-learning exercised:** evolver accepted 6 germline mutations during `sudoku 5`; current lineage-loaded germline has 7 enzymes (`analyze_requirements`, `create_design`, `make_plan` added) and RAF closure remains 100%.
- **New autopoiesis bug found and fixed:** architect accepted an irrelevant patch to `crates/a2d-core/src/prime.rs` during the live run. The generated patch was reverted. Self-sandbox now has an explicit automated-modifiable allowlist and excludes incidental domain/demo modules from architect context and patch eligibility. Documented in `docs/solutions/runtime-bugs/architect-patches-need-relevance-gate-2026-05-01.md`.
- Full `cargo test` passes: 137 passing, 2 ignored.
- **GLM timeout extended:** default timeout for `opencode/zai-coding-plan/glm-5.1` is now 900s (15 minutes). Other providers remain at 300s. `A2D_PROVIDER_TIMEOUT_SECS` still overrides all provider defaults. Added 2 provider unit tests.

### What hasn't been validated yet

- **Whether the architect's self-modifications compound:** `sudoku 5` applied one architect patch, but it was irrelevant and reverted manually. Need a run where patch N+1 builds on a useful patch N.
- **Escalation ladder rungs 4–6 live validation:** Rungs 0–3 are implemented/unit-tested/live-observed; rungs 4–6 are unit-tested and lineage-visible but need bounded live validation under the real registry. See `todos/escalation-rungs-4-6.md`.
- **Post-relevance-gate live validation:** Need a short live run to confirm architect context now excludes `prime.rs`/`email.rs` and rejects ineligible patches mechanically.

### Known issues

- **Rungs 0–3 detect degradation but don't halt it.** On sudoku, all four rungs fired cleanly but coder/evolver kept emitting near-identical output. Rungs 4–6 are now implemented as model swap, clean swap, and bounded consensus, and their mechanics are live-smoked; they still need outcome-quality validation against this failure mode. See `docs/solutions/architectural-insights/escalation-ladder-detects-but-doesnt-halt-degradation-2026-04-17.md`.
- **Provider quality remains uneven.** `sudoku 5` saw 5 GLM timeouts at the old 300s limit (mostly coder/architect/create_design) and 2 Kimi architect no-materialized-output failures. GLM 900s revalidation on 2026-05-18 still timed out on `analyze_requirements` before coder ran, so longer timeout alone is not a fix.
- **Architect empty-output/no-op handling needs live revalidation.** OpenCode write-output recovery helped a known parser gap, raw parsed/stdout previews are now attached to no-materialized-output failures, and a typed no-op contract now exists. Live `sudoku 5` still had two architect invocations with no materialized `system_patch`; next run should inspect whether the new diagnostics/no-op contract separates parser failures from legitimate abstention.
- **Evolver accepted mutations, but quality is mixed.** `sudoku 5` lineage germline grew to 7 enzymes and stayed RAF-closed; new enzymes increased invocations and wall-clock. Need compare against seed topology and evaluate whether mutations add value.
- Evolver prompt still says `food set contains: ["requirements"]` — should include all food artifacts.

## Historical Score Card

These chess rows are pre-2026-06-08 expanded holdouts and should not be compared directly with current `a2d score-artifact chess <path>` replay results, which use the stricter current 9-case scoring contract.

| System | Sudoku (6 acceptance tests) | Chess (historical contract) |
|--------|---------------------------|--------------------------|
| Gemini 3 Pro one-shot | **100%** (7/7) | **100%** (9/9) |
| Codex gpt-5.4 one-shot | **100%** (6/6) | **100%** (11/11) |
| A²D + Codex coder (pre-feedback) | 83% (5/6) | 50% (4/8) |
| A²D + Kimi k2.5 coder (pre-feedback) | 83% (5/6) | not completed |
| A²D + feedback loop + architect | **100% (6/6)** single cycle; **100% best over sudoku 5** (fitness curve: no code → 83% → 100% → 100% → 100%) | **NOT YET RUN** |

## Current Enzyme Topology

```
Current lineage-loaded germline: 7 enzymes, RAF 100% closed.
Use `A2D_GERMLINE=seed` to force the hardcoded 4-enzyme seed topology for comparison.
Baseline food: design, failure_report, fitness_report, plan, provider_health_report, provider_policy, requirements, system_code

Analyze requirements: {requirements} → {spec}                    catalysts: {requirements}
Create design:        {design, spec} → {architecture}             catalysts: {design}
Make plan:            {architecture, plan} → {implementation_plan} catalysts: {plan}
Coder:                {design, plan, requirements} → {code}       catalysts: {enzyme_defs, failure_report, requirements}
Tester:               {code} → {test_results}                     catalysts: {requirements}
Evolver:              {fitness_report} → {enzyme_defs}            catalysts: {enzyme_defs, failure_report, fitness_report, provider_health_report, provider_policy}
Architect:            {failure_report, fitness_report} → {system_patch} catalysts: {provider_health_report, provider_policy, system_code}

Note: seed germline in `a2d-cli/src/main.rs` is still the 4-enzyme topology plus provider-health food/catalysts; CLI loads the evolved 7-enzyme topology from lineage when present and normalizes evolver/architect feedback catalysts.
```

## Current Provider Configuration

```
Coder:     Kimi k2.6             (opencode, kimi-for-coding/k2p6, default provider)
Coder race fallback: DeepSeek v4 flash (opencode, opencode/deepseek-v4-flash-free)
Tester:    GLM 5.1               (opencode, zai-coding-plan/glm-5.1)
Evolver:   Kimi k2.6             (opencode, kimi-for-coding/k2p6)
Architect: GLM 5.1               (opencode, zai-coding-plan/glm-5.1)
```

Gemini is temporarily disabled from the default live registry due repeated capacity/quota failures. Codex quota exhausted until April 8th. GLM is deliberately excluded from coder races, evolver assignment, and evolver fallback because live runs showed repeated coder/evolver timeouts and scoped parallelism waits for slow losers.

Pi provider availability for assistant/delegation work now includes Kimi k2p6 (`kimi-for-coding`) and Minimax 3. These are not yet wired into A²D's default runtime registry; verify exact Pi model IDs/capabilities before using them for provider-policy or architect/tester latency experiments.

## Critical Path — What to Do Next

### 1. Keep escalation validation in the regression loop

Rungs 4–6 now provide ephemeral provider swap, clean swapped session, and bounded provider consensus. Use `validate-escalation` after provider-routing or rung changes to confirm real-registry JSON still shows provider swap, clean-session stripping, candidate evaluations, provider-policy immutability, and the external `escalation_rung` field contract.

### 2. Run bounded architect/tester provider override comparisons

Runtime-only overrides now exist: `A2D_TESTER_PROVIDER=<registered-provider-name>` and `A2D_ARCHITECT_PROVIDER=<registered-provider-name>`. They apply after lineage policy loading and do not persist. Forced-role validation can seed tester/architect inputs directly, but 10s and 30s tester smokes timed out and topology comparison did not reach those roles because coder timed out first. Use a longer bounded direct role comparison, a cheaper provider, or a successful coder cycle to test whether moving tester/architect off GLM reduces timeout waste before changing defaults or provider policy. Example candidates: Kimi k2.6 and DeepSeek v4 flash. Pi Kimi/Minimax lanes remain unverified for A²D runtime; `pi --help` exceeded a 30s probe window this session, so do not wire new Pi model IDs blindly.

### 3. Evaluate rung-6 eligibility/concurrency tradeoffs only with replicated evidence

Current rung 6 uses sequential bounded invocation (`A2D_RUNG6_MAX_PROVIDERS`, default 3). Default eligibility is assigned + unassigned providers while excluding other-role assignments; `A2D_RUNG6_PROVIDER_SCOPE=broad` opt-in includes all healthy registered providers. A single 30s smoke was inconclusive because DeepSeek is in both scopes. Repeat controlled comparisons before considering timeout-bounded concurrency.

### 4. Evaluate evolved 7-enzyme topology

The evolver accepted 6 mutations in `sudoku 5`, producing a 7-enzyme RAF-closed topology. Determine whether these extra enzymes improve outcomes or just add latency/invocation overhead. Use `a2d compare-topologies <challenge> <cycles>` for bounded, non-persistent seed-vs-evolved runs.

### 4. Live-validate expanded chess/Rubik's acceptance tests

Chess hidden tests now cover castling, en passant, legal-move safety, and Fool's mate/no-escape. Rubik's hidden tests now cover rotation inverse/order invariants, known inverse roundtrip, solver-on-known-scrambles, and seeded scramble replayability/solve roundtrip. Next: run bounded `chess` and `rubiks` challenge smokes to see whether the stronger holdouts produce useful provider signal.

## Architecture Overview

```
Requirements ──→ Coder ──→ Code ──→ Tester ──→ Test Results
                   ↑
                   ├──── failure_report (from sandbox) ──────────────────────────
                   └──────────────────── (catalysts) ─────────────────────────┐
                                                                              │
                   Code ──→ Sandbox (rustc compile + run tests) ──→ Fitness Score
                                                                       │      │
                                                                       ├→ failure_report → Coder (next cycle)
                                                                       ├→ fitness_report + failure_report → Evolver → Enzyme Defs
                                                                       ↓
                                                              Fitness Ratchet
                                                                       │
                                                                       ↓
                                                              Lineage Archive

Failure Report + Fitness + System Code ──→ Architect ──→ System Patch
                                                              │
                                                     Self-Sandbox (cargo test)
                                                              │
                                                     Accept → Apply to source tree
                                                     Reject → Feed back to architect
```

## Key Files

| File | Purpose |
|------|---------|
| `crates/a2d-core/src/metabolism.rs` | Cycle orchestration, enzyme invocation, fitness evaluation, architect/patch routing |
| `crates/a2d-core/src/benchmark.rs` | Fitness scoring, sandbox integration, acceptance tests, **diagnostic capture** |
| `crates/a2d-core/src/sandbox.rs` | rustc compilation + test execution |
| `crates/a2d-core/src/self_sandbox.rs` | **NEW:** System code modification validation (copy tree, apply patch, cargo test) |
| `crates/a2d-core/src/challenges.rs` | Challenge definitions + acceptance tests |
| `crates/a2d-core/src/germline.rs` | Enzyme set management, mutation, RAF |
| `crates/a2d-core/src/types.rs` | EnzymeDef (with prompt_template), ArtifactType |
| `crates/a2d-providers/src/cli.rs` | CLI providers (Codex, Gemini, OpenCode) |
| `crates/a2d-cli/src/main.rs` | CLI binary, provider registry, seed germline, **patch application** |
| `docs/plans/true-autopoiesis.md` | Plan for self-modification capability |

## Quick Reference

```bash
# Run a challenge
a2d challenge sudoku 3        # 3 cycles of sudoku
A2D_GERMLINE=seed a2d challenge sudoku 3  # force 4-enzyme seed topology
a2d compare-topologies sudoku 3           # seed vs evolved without persistence
a2d compare-provider-policy sudoku 1      # current vs proposed provider policy without persistence
a2d score-artifact chess ./candidate.rs   # replay saved artifact against hidden holdouts (exit 2 on failure)
a2d challenge chess 3
a2d challenge rubiks 3

# Check system state
a2d status                    # RAF closure
a2d enzymes                   # List enzyme definitions (now includes architect)
a2d lineage                   # Git log of germline evolution

# Run tests
cargo test                    # 205 tests passing (2 ignored integration)
cargo test -- --ignored       # Run integration tests (slow, compiles in temp dir)

# Check OpenCode model IDs
opencode models | grep -i 'kimi\|glm\|minimax'
```

## Compound Learnings Index

Search `docs/solutions/` before implementing. Key findings:

- **architectural-insights/**: cycle bottleneck is the cycle not the model, evolver adds no value, feedback loop broken, **harness engineering patterns** (Fowler), **landscape patterns to investigate** (Symphony/Gas Town/StrongDM/Schillace)
- **best-practices/**: acceptance test coverage, fitness-gated evolution, multi-model dispatch
- **runtime-bugs/**: provider timeouts, NDJSON parsing, fitness ratchet leak, cross-cycle dedup, architect relevance gate

## Todos

- `todos/provider-policy-topology-gate.md` — completed; live runtime proposal was rejected durably without fitness evidence. Future work is a benchmark-useful provider-policy proposal path, not the safety gate itself.
- `todos/autonomous-project-loop.md` — checkbox-complete for current slice; live repair-diversity routing validated with fault injection, successful DeepSeek repair reached commit, and markdown repo-reference validation now catches invented source-file claims; broader semantic claim validation remains partial
- `todos/bounded-live-benchmarks.md` — `sudoku 5` completed with 100% best fitness; remaining provider waste: GLM timeouts + architect no-materialized-output
- `todos/escalation-rungs-4-6.md` — model swap → clean swap → multi-model consensus (rungs 4–6 implemented/unit-tested; bounded mechanism validation now covered by `validate-escalation`; quality/eligibility work remains)
- `todos/architect-pyramid-summaries.md` — implemented; prompt-size validated; latency still bad due provider/scheduling
- `todos/test-evolution.md` — test evolution surface; latest addendum records expanded chess/Rubik's hidden acceptance coverage; remaining work is live validation and challenges beyond sudoku/chess/rubiks
- `todos/testable-core.md` — separate pure orchestration from provider I/O to enable deterministic replays

## Autopilot update 1779711298225

2026-05-25: Advanced the autonomous project loop task by making the remaining bounded repair/escalation contract explicit in todos/autonomous-project-loop.md, including repair budget, hard-stop behavior, report requirements, and no-partial-application invariants.


## Autopilot update 1779713332097

Advanced autonomous project loop task by specifying the provider-diverse repair escalation contract: repair attempts must record the attempted provider topology, may escalate to an alternate provider/model after the primary maintainer provider fails, preserve the same typed project_patchset contract and safety gates, and stop with a machine-readable report when escalation budget is exhausted.


## Autopilot update 1780125323289

todos/escalation-rungs-4-6.md: added Implementation Status section noting rung 4-6 code still absent from invocation pipeline and pointing to next-action targets. **Superseded:** 2026-06-01 — rung 4 ephemeral swap, rung 5 clean swapped-session lineage, and rung 6 bounded provider consensus are now implemented/unit-tested; remaining work is live validation.

