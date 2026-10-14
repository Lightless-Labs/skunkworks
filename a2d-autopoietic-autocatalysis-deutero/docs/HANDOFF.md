# A²D Handoff Document

**Last updated:** 2026-10-14 (session 142 — direction audit + handoff reset)
**Update this document:** before context compaction, at session end, or when significant state changes. Keep it short: current truth only. History lives in git and `docs/handoff-archive/`.

## Headline (read this first)

- **A²D has never beaten the one-shot baseline on any task or metric.** Best case it ties sudoku (one-shot already 100%) at 4–19 invocations vs 1; chess is 50% vs 100%. See `docs/research/2026-10-14-direction-audit.md`.
- **Hidden holdouts currently leak into selection.** Live runs build the metabolism with `challenge.scoring_benchmark()` (hidden acceptance attached, `crates/a2d-cli/src/main.rs:5377`), and coder-portfolio candidates plus `failure_report` feedback are driven by that score. A²D's numbers are oracle-selected best-of-N; the fair comparison is best-of-N one-shot, and the real fix is to separate the selection oracle (public tests) from the evaluation oracle (hidden tests).
- **Sessions 2026-06-29 → 07-10 drifted:** ~123 of 197 commits were Senior SWE-Bench / SWE-Bench Pro scaffolding, leak-hardening and evidence records; 2 touched outcome quality. 108 "baseline" fitness-evidence files replay the same hand-written `good-sudoku-artifact.rs` at fitness 1.0, so they certify nothing about the change they are attached to.
- **The SWE-Bench Pro "access" blocker is mostly self-imposed:** a public 731-task split with a runnable harness exists (verify before use). The sealed-manifest machinery is not required to measure lift.
- **Direction is pending the owner's decision on the audit.** Recommended: Option A (lift-first minimal loop on a non-saturated benchmark) now, Option B (offline observer test on Third Thoughts data) in parallel. Until decided, do not add SWE-bench/redaction/env-scrub scaffolding.

## Session Protocol

1. Read this file, then `docs/research/2026-10-14-direction-audit.md` if you are choosing work.
2. `cargo test` must pass before changing anything (464 tests, 2 ignored; a cold build of `a2d-cli` takes several minutes — `main.rs` is ~21K lines).
3. Update this file at session end. Replace stale facts; do not append session diaries.

## Current State

### Demonstrated (live)

- End-to-end metabolism: requirements → coder/tester/sandbox/evolver/architect with RAF checks, fitness ratchet, lineage, `failure_report` feedback, provider circuit breaking and timeouts.
- Sudoku reaches 100% (6/6 coarse cases: compiles, has_tests, all_tests_pass, three `has_*_fn` checks — all hidden acceptance tests collapse into `all_tests_pass`). Repeated one-cycle seed runs: 4/5 at 100%; the failing replica (r1, 67%) failed `has_tests` (missing test module), i.e. prompt/harness formatting rather than solver logic (`runs/20260630-sudoku-repeat-evidence/`).
- Evolver mutations are accepted (lineage germline: 7 RAF-closed enzymes), architect `SystemPatch` path is gated and atomic, provider CLI isolation (empty cwd, OpenCode `--pure` no-tools agent) works.

### Implemented and unit-tested, value unproven

| Component | Live outcome evidence |
|---|---|
| Evolver / deutero-learning | mutations accepted; no fitness gain shown; seed vs evolved noise (83/67/50 vs 67/50/83) |
| Architect self-modification | one live patch, irrelevant, reverted |
| Escalation rungs 0–3 / 4–6 | 0–3 detect but don't halt degradation; 4–6 mechanism smoke only (`validate-escalation`) |
| Fitness-scored coder portfolio | never compared against a single provider; selection uses hidden holdouts (see Headline) |
| Provider policy gate | only rejections observed (Pi-Minimax: +0pp, +2 invocations, +243.5s) |
| Observer / pre-failure kill | **never fires**: CLI providers emit a constant `[Think, Text]` trace (`crates/a2d-providers/src/cli.rs:410`) and kill states need ≥5 events (`crates/a2d-core/src/observer.rs:178-185`) |
| Autopilot project loop | produces commits; it reads only the first 2,000 chars of this file (`collect_project_state` in `main.rs`) — keep the Headline accurate |
| Senior SWE-Bench / SWE-Bench Pro chain (~25 subcommands) | one synthetic 22-line task with a grep-based evaluator; no real benchmark task evaluated |

### SWE-Bench Pro chain (parked)

Plan: `docs/plans/swe-bench-pro-blind-evaluator-adapter.md`. Slices 1–5 implemented against synthetic sealed evaluators (readiness gate, access-manifest inspection, sealed evaluator, redacted evidence export, retry feedback boundary incl. `cycle-input --swe-bench-pro-evaluation` pressure mode — commit `507183c`). Slice 6 (live smoke) needs a real evaluator. Parked pending the direction decision; if Pro is used, prefer the public split with tests kept out of coder context over more manifest machinery.

## Score Card

| System | Sudoku (coarse 6-case fitness) | Chess |
|---|---|---|
| Gemini 3 Pro one-shot (April, logs only) | 100% | 100% (historical 9-case contract) |
| Codex gpt-5.4 one-shot (April, logs only) | 100% | 100% (historical 11-case contract) |
| A²D + Codex coder, pre-feedback (April) | 83% | 50% (4/8) |
| A²D + feedback + architect (May, `sudoku 5`) | 100% best (curve: none → 83 → 100 → 100 → 100) | no chess code since holdout expansion (coder timed out) |
| A²D seed, 5× one-cycle repeat (June 30) | 4/5 at 100%, 1 at 67% | — |
| One-shot with A²D's actual coders (Kimi k2.6, DeepSeek v4 flash) | **not measured** | **not measured** |

Caveats: baselines used stronger models than A²D's current coders and have no durable artifacts; A²D scores are oracle-selected (see Headline). Several historical claims cite `/tmp` logs that no longer exist.

## Next Moves (recommended by the audit; pending owner decision)

1. **Separate the oracles.** Public/visible tests drive portfolio selection and `failure_report`; hidden holdouts score only the final artifact. Without this, no A²D-vs-baseline number is fair.
2. **Durable cheap baselines.** ~10 one-shot calls per coder model actually used (Kimi k2.6, DeepSeek v4 flash), saved under `runs/` and scored with `a2d score-artifact`.
3. **Pick a non-saturated benchmark** where one-shot pass@1 is 20–70%: chess perft exactness (e.g. Kiwipete, depth 4–5), larger Rust specs with 50+ hidden property tests, or the public SWE-Bench Pro split. Calibrate first.
4. **Pre-registered lift test,** n ≥ 20 tasks, matched invocation budget k: (a) one-shot pass@1, (b) best-of-k one-shot selected by public tests, (c) A²D seed with k invocations. Falsifier: (c) fails to beat (b) by ≥10 pp → the cycle is overhead.
5. **Ablate** evolver, architect, rungs 4–6, portfolio on that benchmark; delete what doesn't move the metric.
6. **Observer offline:** run `observer::observe` on Third Thoughts / Middens traces and measure failure-prediction AUC (zero provider cost). If no signal, drop the "mechanical cortex" claim.

Stop doing: SWE-bench/redaction/env-scrub scaffolding, constant-artifact sudoku replay as a commit gate (use `cargo test` for plumbing; require fitness evidence only when claiming an outcome effect, and it must exercise the change), separate "record evidence" commits.

## Enzyme Topology

```
Lineage-loaded germline: 7 enzymes, RAF closed. A2D_GERMLINE=seed forces the 4-enzyme seed.
Food: design, failure_report, fitness_report, plan, provider_health_report, provider_policy, requirements, system_code

Analyze requirements: {requirements} → {spec}
Create design:        {design, spec} → {architecture}
Make plan:            {architecture, plan} → {implementation_plan}
Coder:                {design, plan, requirements} → {code}        catalysts: enzyme_defs, failure_report, requirements
Tester:               {code} → {test_results}
Evolver:              {fitness_report} → {enzyme_defs}             catalysts: enzyme_defs, failure_report, fitness_report, provider_health_report, provider_policy
Architect:            {failure_report, fitness_report} → {system_patch}  catalysts: provider_health_report, provider_policy, system_code
```

Scheduling: one ready invocation at a time (highest priority), then the ready set is recomputed. Before code exists the coder goes first; once a `fitness_report` exists (with or without code) feedback runs first: evolver → architect → tester → coder. A successful code-producing or any failed invocation ends the cycle.

## Provider Configuration (as last recorded, 2026-07)

```
Coder:     Kimi k2.6 (opencode, kimi-for-coding/k2p6)   race fallback: DeepSeek v4 flash (opencode/deepseek-v4-flash-free)
Tester:    GLM 5.1   (opencode, zai-coding-plan/glm-5.1)
Evolver:   Kimi k2.6
Architect: GLM 5.1
```

Gemini disabled in the default registry (capacity/quota failures). GLM excluded from coder races and evolver (timeouts). Opt-in lanes and Pi provider names: see `CLAUDE.md` § Provider Configuration. Re-verify availability before live runs; this table is three months old.

## Architecture

```
requirements ─→ Coder ─→ code ─→ Sandbox (rustc + tests + hidden acceptance) ─→ fitness_report
                  ↑                                                    ├→ failure_report → Coder (next cycle)
                  └──────────── failure_report ────────────────────────├→ Evolver → enzyme_defs → Germline (RAF check, ratchet) → Lineage
                                                                       └→ Architect → system_patch → Self-sandbox (cargo test) → apply / reject
```

## Key Files

| File | Purpose |
|---|---|
| `crates/a2d-core/src/metabolism.rs` | cycle orchestration, scheduling, fitness, portfolio, architect/patch routing |
| `crates/a2d-core/src/benchmark.rs` | fitness scoring, sandbox integration, acceptance tests |
| `crates/a2d-core/src/observer.rs`, `workcell.rs` | behavioural state classification and kill decisions |
| `crates/a2d-core/src/self_sandbox.rs` | `SystemPatch` validation |
| `crates/a2d-cli/src/challenges.rs` | challenge catalog + hidden acceptance (`scoring_benchmark()`) |
| `crates/a2d-cli/src/main.rs` | CLI (~21K lines; over half is SWE-bench/retry/manifest/scrub code) |
| `crates/a2d-providers/src/cli.rs` | CLI providers (Codex, Gemini, OpenCode, Pi) |
| `CONSTITUTION.md` | organizational invariants |

## Quick Reference

```bash
cargo test
a2d challenge sudoku 3                    # A2D_GERMLINE=seed to force seed topology
a2d compare-topologies sudoku 3           # seed vs evolved, no persistence
a2d score-artifact chess ./candidate.rs   # replay against hidden holdouts (exit 2 on failure)
a2d validate-escalation sudoku coder      # with A2D_PROVIDER_TIMEOUT_SECS=1 A2D_MAX_CYCLE_SECS=1 A2D_RUNG6_MAX_PROVIDERS=2
a2d status | a2d enzymes | a2d lineage
opencode models | grep -i 'kimi\|glm\|minimax'
```

## Pointers

- Audit: `docs/research/2026-10-14-direction-audit.md`
- Prior art for the post-lift-test architecture (DGM archive search; RQGM anchor-gated co-evolved evaluator → tester enzyme): `docs/research/2026-10-14-dgm-rqgm-relevance.md`
- Full pre-reset handoff (sessions ≤141): `docs/handoff-archive/2026-07-10-session-141-HANDOFF.md`
- Learnings: `docs/solutions/` (search before implementing); todos: `todos/`
- Sibling A² ran the same self-diagnosis: `../a2-autopoietic-autocatalysis/docs/plans/2026-07-10-a2-remediation-roadmap.md` — avoid its value-last phase ordering.
