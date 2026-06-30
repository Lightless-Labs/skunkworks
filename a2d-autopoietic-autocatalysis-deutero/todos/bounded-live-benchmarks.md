# Bounded Live Benchmarks Despite Slow Provider Failures

**Created:** 2026-04-23
**Partially implemented:** 2026-04-23 — added a per-cycle wall-clock budget (`CycleReport.wall_clock_capped`, default 600 s, CLI override `A2D_MAX_CYCLE_SECS`, disable with `A2D_MAX_CYCLE_SECS=0`) and provider failure circuit breaker (`A2D_PROVIDER_COOLDOWN_SECS`, default 600 s). Provider calls are still bounded separately by `A2D_PROVIDER_TIMEOUT_SECS` / provider default. Smoke validated with `A2D_PROVIDER_TIMEOUT_SECS=1 A2D_MAX_CYCLE_SECS=1`: cycle ended as `[wall-clock-capped]` after one timed-out provider call. Full validation before circuit breaker: `A2D_TRACE=1 cargo run -p a2d -- challenge sudoku 1` completed with 4 invocations, `[wall-clock-capped]`, Fitness 100% (6/6), instead of hitting the 900 s harness timeout. Circuit breaker live probe on 2026-04-28 confirmed provider cooldown recording but not live rerouting; the cycle capped after a second Kimi timeout before a subsequent Gemini-assigned invocation.
**Addendum:** 2026-05-18 — GLM's 900s default was live-tested with lineage-loaded 7-enzyme topology: `A2D_TRACE=1 cargo run -p a2d -- challenge sudoku 1` timed out on the first `analyze_requirements` invocation after 900s, wall-clock-capped, best fitness 0%. This suggests the evolved topology + GLM default is wasting the whole bounded run before coder can fire. Added `A2D_GERMLINE=seed` to mechanically force the 4-enzyme seed germline for seed-vs-evolved comparisons.
**Addendum:** 2026-05-18 — scheduler priority now orders ready enzymes by direct artifact progress (`code` → `test_results` → `enzyme_defs` → `system_patch` → auxiliary). Smoke validation with `A2D_TRACE=1 A2D_PROVIDER_TIMEOUT_SECS=1 A2D_MAX_CYCLE_SECS=1 cargo run -p a2d -- challenge sudoku 1` showed ready order `["coder", "analyze_requirements"]`, so lineage-added decomposition no longer starves coder by alphabetical ordering.
**Addendum:** 2026-05-18 — failed/killed invocations now end the current cycle before lower-priority ready enzymes execute. Two-cycle smoke with `A2D_PROVIDER_TIMEOUT_SECS=1` showed cycle 1 GLM coder timeout and cycle 2 Kimi coder fallback, with no auxiliary decomposition after coder failure. Log: `/tmp/a2d-sudoku2-20260518-fallback-smoke.log`.
**Addendum:** 2026-05-18 — bounded 60s evolved-topology validation (`A2D_TRACE=1 A2D_PROVIDER_TIMEOUT_SECS=60 A2D_MAX_CYCLE_SECS=180 cargo run -p a2d -- challenge sudoku 2`) confirmed the structural fix: cycle 1 spent budget on GLM coder, cycle 2 routed coder to Kimi fallback, and no auxiliary decomposition ran after coder failures. Both coder invocations timed out at 60s, best fitness 0%; next comparison needs a larger fallback window or a provider assignment change.
**Addendum:** 2026-05-19 — coder provider dispatch is now parallel by default for cheap/good-enough providers. Assigned + unassigned fallback providers race concurrently; tester/evolver assigned providers are excluded. `A2D_PARALLEL_CODER=0` disables for controlled runs. 60s live smoke showed GLM and Kimi spawned concurrently and both timed out after ~60s rather than serial ~120s. Log: `/tmp/a2d-sudoku1-20260519-parallel-coder-60s.log`.
**Addendum:** 2026-05-20 — added `a2d compare-topologies <challenge> <cycles>` (alias `benchmark-topologies`) to run seed and lineage-loaded evolved topologies side by side without lineage commits or patch application. The summary reports best fitness, cycle-to-full-fitness, wall-clock, invocation count, provider failures, caps, mutations, and accepted/rejected patches. Smoke validated with `A2D_PROVIDER_TIMEOUT_SECS=1 A2D_MAX_CYCLE_SECS=1`; seed ran with 4 enzymes, evolved with 7, and both prioritized coder first.
**Addendum:** 2026-05-20 — real `compare-topologies sudoku 3` with 180s provider timeout partially completed before the outer harness timeout. Seed: 0% best after three coder failures. Evolved: reached 67% (4/6) in cycle 1 via Kimi fallback, then stalled. The run exposed two more runtime issues: failed rung-2 consultation spent a second provider timeout on primary invocation, and GLM as a coder race candidate kept the critical path slow. Fixed both: failed consultation now ends the workcell immediately, and GLM is assigned only to tester/evolver/architect. MiniMax highspeed + Kimi k2.5 still timed out under 60s, so direct provider smokes selected Kimi k2.6 + DeepSeek v4 flash as the coder pool. One-invocation topology smoke with the fast pool: seed 0%, evolved 67% (4/6). Logs: `/tmp/a2d-topology-compare-sudoku3-20260520.log`, `/tmp/a2d-topology-compare-sudoku3-consultfail-5s-20260520.log`, `/tmp/a2d-topology-compare-sudoku1-minimax-60s-20260520.log`, `/tmp/a2d-topology-compare-sudoku1-fastpool-oneinvoke-20260520.log`.
**Addendum:** 2026-05-20 — parallel coder dispatch is now a fitness-scored portfolio rather than provider-order race. Every materialized code candidate is evaluated by the current benchmark/sandbox, the highest-fitness candidate wins, and candidate provider/materialization/error/fitness is stored in lineage and printed by topology comparison. This aligns the dispatch layer with learning/adaptation first, quality second, speed third.
**Addendum:** 2026-05-21 — real `compare-topologies sudoku 3` with the portfolio completed. Both seed and evolved reached 67% best; seed took ~930s, evolved ~810s. The run exposed scheduler starvation in the opposite direction: after a successful coder invocation, the cycle retried coder before tester/evolver/architect could metabolize feedback, and stale auxiliary work could run after code success. Fixed by ending the cycle after successful code production and making priority dynamic: coder first only before code exists; tester/evolver/architect before coder once code exists. One-cycle live validation: seed 83%, evolved 67%, one coder invocation each, no stale auxiliary after code. Log: `/tmp/a2d-topology-compare-sudoku1-cycleadvance-20260521.log`.
**Addendum:** 2026-05-21 — evolver now consumes mechanical `fitness_report` directly instead of being gated behind model-generated `test_results`; loaded lineage germlines are normalized to this contract. Scheduler priority after fitness exists is now evolver → architect → tester → coder. Live `compare-topologies sudoku 2` with 90s providers validated seed cycle 2 ready order `evolver`, `architect`, `tester`, `coder`; seed evolver timed out on GLM, while evolved reached 100% at cycle 2. Log: `/tmp/a2d-topology-compare-sudoku2-mechanical-evolver-20260521.log`.
**Addendum:** 2026-06-29 — `fitness_report` is now structured `a2d.fitness-evidence.v1` evidence with source cycle, redacted per-case results, non-regression status, and diagnostic presence. CLI durability/application gates require current benchmark evidence or a freshly consumed previous-cycle non-regressing fitness-evidence artifact before accepted mutations/provider-policy changes become durable or accepted `SystemPatch`es are applied. Unit coverage validates hidden-name redaction, stale evidence rejection, regressing evidence rejection, and fresh non-regressing evidence acceptance. Next bounded live benchmark task: run a short sudoku challenge/topology smoke and inspect live lineage/current artifacts for the new schema and freshness behavior.
**Addendum:** 2026-06-29 — bounded live inspection now has an opt-in export path. `A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260629-fitness-evidence A2D_GERMLINE=seed A2D_PROVIDER_TIMEOUT_SECS=120 A2D_MAX_CYCLE_SECS=180 cargo run -p a2d -- challenge sudoku 1` produced `runs/20260629-fitness-evidence/sudoku-solver-cycle-0-fitness-evidence.json`; jq validation confirmed `schema_version: a2d.fitness-evidence.v1`, `actual_tests_evaluated: true`, `cycle: 0`, `non_regressing: true`, nonnegative delta, reviewed fields only, and no non-public hidden case names. The run reached 67% (4/6), with hidden/acceptance aggregate `all_tests_pass: false`, so it proves export/inspection rather than solved performance. Export validation now fails closed on no evidence, stale/regressing/incomplete evidence, unknown fields, hidden-name leakage, and provider-produced fake `fitness_report` outputs.
**Addendum:** 2026-06-29 — multicycle export now records fresh previous-cycle evidence consumed/available in feedback cycles without pretending a new benchmark ran. `A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260629-fitness-evidence-multicycle A2D_GERMLINE=seed A2D_PROVIDER_TIMEOUT_SECS=90 A2D_MAX_CYCLE_SECS=120 cargo run -p a2d -- challenge sudoku 2` produced `runs/20260629-fitness-evidence-multicycle/sudoku-solver-cycle-0-fitness-evidence.json` and `runs/20260629-fitness-evidence-multicycle/sudoku-solver-cycle-0-consumed-by-cycle-1-fitness-evidence.json`. Both are non-regressing `a2d.fitness-evidence.v1` artifacts with `all_tests_pass: false` / 67% (4/6), so they prove evidence provenance across feedback metabolism, not solver completion. Run doc: `examples/runs/2026-06-29-fitness-evidence-export.md`.
**Addendum:** 2026-06-30 — comparison modes now share the same opt-in evidence export path. `A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260630-topology-fitness-evidence A2D_PROVIDER_TIMEOUT_SECS=90 A2D_MAX_CYCLE_SECS=120 cargo run -p a2d -- compare-topologies sudoku 1` produced labeled seed/evolved `a2d.fitness-evidence.v1` artifacts, both `actual_tests_evaluated: true`, `non_regressing: true`, `all_tests_pass: true`, fitness 100% (6/6), SHA-256 `6aa4f715aaa5dd155371519737ff569c3deb0233a01a18cc263e9ec0e2c62abe`. `compare-provider-policy sudoku 1` also exported labeled current/proposed artifacts under `runs/20260630-provider-policy-fitness-evidence/`; because that run had no assignment delta and one leg had `all_tests_pass: false`, it validates comparison export plumbing only, not a durable provider-policy change. Run doc: `examples/runs/2026-06-30-comparison-fitness-evidence-export.md`.
**Addendum:** 2026-06-30 — repeated seed Sudoku challenge evidence is now tracked. Five `A2D_GERMLINE=seed A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260630-sudoku-repeat-evidence/rN A2D_PROVIDER_TIMEOUT_SECS=90 A2D_MAX_CYCLE_SECS=120 cargo run -p a2d -- challenge sudoku 1` replicas exported structured `a2d.fitness-evidence.v1` actual-test records. All five are non-regressing; r1 was 67% (4/6), `all_tests_pass: false`, SHA-256 `bbf8547e01ffeba66022f336f0e8b017578732f0f175e82b8cb80d91fd4c4a4f`; r2-r5 were 100% (6/6), `all_tests_pass: true`, SHA-256 `6aa4f715aaa5dd155371519737ff569c3deb0233a01a18cc263e9ec0e2c62abe`. This upgrades older log-only full-fitness claims into tracked evidence, but also shows reliability is not yet one-shot-baseline parity because 1/5 bounded replicas failed aggregate acceptance. Run doc: `examples/runs/2026-06-30-sudoku-repeat-fitness-evidence.md`.
**Context:** Live validation of architect pyramid summaries (`A2D_TRACE=1 cargo run -p a2d -- challenge sudoku 1`) reduced architect context from ~400 KB to 17,702 bytes (`system_code`) / 37,533 bytes (Gemini prompt arg), but the run still exceeded the 900 s harness timeout.

## Observed failure mode

Trace excerpt from 2026-05-18 GLM 900s validation:

- Loaded germline from lineage: 7 enzymes
- Ready enzymes: `analyze_requirements`, `coder`
- `analyze_requirements` via `opencode/zai-coding-plan/glm-5.1`: timed out after 900s
- Cycle wall-clock cap fired before coder could run
- Result: 1 invocation, 0 mutations, 0 patches, RAF 100%, `[wall-clock-capped]`, best fitness 0%
- Log: `/tmp/a2d-sudoku1-20260518-after-noop.log`

Trace excerpt from 2026-04-23:

- Coder (Kimi/OpenCode): OK in 84.55 s
- Architect (Gemini): FAIL in 272.95 s with a 37.5 KB prompt
- Coder (Kimi/OpenCode): OK in 39.21 s
- Tester (Gemini): FAIL in 274.60 s with a 5.6 KB prompt
- Architect scheduled again with same 37.5 KB prompt class; harness timed out at 900 s

The context pyramid solved prompt volume. The wall-clock cap solved unbounded cycle wall time. It did not solve provider latency/failure: Gemini still consumed ~284 s before quota failure for architect and 300 s before timeout for tester.

## Problem

A one-cycle benchmark must complete in bounded wall-clock time. The current `max_invocations_per_cycle = 20` caps invocation count, but not wall-clock when individual Gemini invocations approach the provider timeout and multiple Gemini-backed enzymes can fire in the same cycle.

## Candidate fixes

1. **Wall-clock cycle budget** — IMPLEMENTED 2026-04-23
   - Added `max_cycle_wall_clock` to `Metabolism`.
   - Before scheduling each invocation, stop the cycle if elapsed time exceeds the budget.
   - Report `CycleReport.wall_clock_capped: bool`.
   - Caveat: in-flight provider calls are not interrupted by the cycle budget; the provider timeout is still the hard per-call bound.

2. **Provider failure escalation** — PARTIALLY IMPLEMENTED 2026-04-23
   - If a provider call fails or times out, increment that enzyme's escalation count immediately.
   - Provider-level circuit breaker cools down the failing provider and routes subsequent invocations to healthy alternatives.
   - Cooldown is temporary, so providers are retried after recovery instead of banned permanently.
   - Full rung 4+ history-aware model swap remains open.

3. **Architect frequency gate**
   - Architect should not fire on every `fitness_report`/`failure_report` revision in the same cycle.
   - Options: once per cycle, only on degradation, or only after coder/evolver escalation reaches a threshold.

4. **Provider assignment change**
   - Gemini appears slow/failing for architect and tester in this live run even with small prompts.
   - GLM with 900s default timed out on `analyze_requirements` before coder ran in the 2026-05-18 bounded validation.
   - Try a faster architect/tester/decomposition provider or use slow providers only for long-context cases where they add value.

5. **Seed-vs-evolved topology switch** — IMPLEMENTED 2026-05-18
   - `A2D_GERMLINE=seed` forces the hardcoded 4-enzyme seed germline instead of loading lineage.
   - Verified with `A2D_GERMLINE=seed cargo run -p a2d -- status`: RAF 100%, closed, 4 enzymes.
   - Use this for mechanical comparison against the lineage-loaded 7-enzyme topology.

6. **Scheduler priority for direct artifact progress** — IMPLEMENTED 2026-05-18
   - Ready enzymes producing `code` run before auxiliary/decomposition enzymes.
   - Then `test_results`, `enzyme_defs`, `system_patch`, and finally auxiliary products.
   - Prevents lineage-added `analyze_requirements` from consuming the whole bounded cycle before coder can run.

7. **Fail-fast cycle advance after invocation failure** — IMPLEMENTED 2026-05-18
   - If a provider failure, empty-output failure, or observer kill happens, the cycle stops before scheduling lower-priority ready work.
   - Provider cooldown persists into the next cycle, so the failed enzyme can route to fallback instead of letting auxiliary enzymes consume the remaining budget.

8. **Parallel cheap coder race** — IMPLEMENTED 2026-05-19
   - Coder invokes assigned + unassigned fallback providers concurrently by default.
   - Selection is mechanical: first provider-order response that materializes `code`, then sandbox fitness evaluates it.
   - Losing provider failures cool down the provider without escalating coder when another provider succeeded.
   - Disable with `A2D_PARALLEL_CODER=0`.

## Acceptance criteria

- `a2d challenge sudoku 1` completes under a configured wall-clock budget. **MET 2026-04-23**
- A slow/failed provider call does not cause repeated same-cycle retries. **MET 2026-04-28:** live all-GLM run observed `provider circuit breaker: routing ...` from GLM to Kimi after GLM timeout.
- Trace/CLI output clearly reports whether the cycle ended by invocation cap, wall-clock cap, or normal quiescence.
