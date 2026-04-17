# A² Handoff — Read This First

**Last updated:** 2026-04-16
**Update this file:** before context compaction, at session end, or when significant state changes.

## What Is This

A² (Autopoietic Autocatalysis) is an autonomous software factory that modifies its own source code. It uses AI model CLIs (Claude, Codex, Gemini, OpenCode) as "food set" models that edit code in git worktrees, then the system verifies, scores, and optionally applies the patches to its own germline.

## Current Numbers (as of 2026-04-07)

| Metric | Value |
|--------|-------|
| Tests | 62 |
| Sentinels | 5/6 PASS (lockfile_check fails — pre-existing, Cargo.lock unchanged locally) |
| Crates | 11 |
| Benchmark (OpenCode/GLM via A²) | 5/5 (with 100k token / 1800s budget) |
| Benchmark (OpenCode/GLM raw, no A²) | 5/5 |
| Benchmark (Gemini 3.1 Pro) | 5/5 |
| Benchmark (Claude) | untested on current task set |
| A² value-add on single-pass tasks | None measurable |
| 4-provider smoke (2026-04-16) | 4/4 PASS (gemini, glm-5.1, minimax-2.7, kimi k2.5) post ContextPack wiring |

## Verify State (run these first)

```bash
cd /Users/thomas/Projects/lightless-labs/skunkworks/a2-autopoietic-autocatalysis
cargo test                                    # expect 61 pass
cargo run -p a2ctl -- sentinel --workspace .  # expect 6/6 PASS
```

If any fail, read `docs/solutions/` for known issues before touching anything.

## Read Before Working

**Session 2026-04-05/07 produced 14 compound learning docs in `docs/solutions/`.** Read the ones relevant to your task:

**Process (always applicable):**
- `workflow-issues/handoff-editorial-creep-20260404.md` — handoffs state facts, not interpretation
- `workflow-issues/single-run-conclusions-20260404.md` — N≥3 before reporting any benchmark number
- `workflow-issues/user-questions-as-design-signals-20260404.md` — short user questions are refactor triggers
- `workflow-issues/explore-the-code-before-opining-20260416.md` — HANDOFF is a starting point, not a verdict
- `best-practices/autopoietic-no-pausing-20260404.md` — the project name is the instruction
- `best-practices/contextpack-is-a-load-bearing-extension-point-20260416.md` — empty extension points silently invalidate downstream benchmarks

**Design:**
- `best-practices/evaluation-must-not-touch-the-germline-20260404.md` — soma vs germline separation
- `best-practices/benchmark-the-loop-not-the-model-20260404.md` — single-pass bench ≠ loop bench
- `best-practices/observations-to-decisions-not-messages-20260404.md` — typed decisions over advisory strings

**Technical (read if touching relevant area):**
- `logic-errors/git-apply-context-mismatch-when-baseline-diverges-20260405.md` — before modifying apply logic
- `best-practices/worktree-catalyst-base-ref-pattern-20260405.md` — before modifying WorktreeCatalyst
- `best-practices/observational-evaluation-no-mutation-20260405.md` — before touching bench command
- `best-practices/strategy-change-enum-over-string-20260405.md` — before touching stagnation
- `best-practices/budget-variance-as-noise-floor-20260405.md` — before tuning token/timeout limits
- `best-practices/clippy-collapsible-if-with-let-chains-20260405.md` — Rust let-chain idiom
- `integration-issues/monorepo-nested-worktree-paths-20260405.md` — before writing scripts that shell out to git worktree
- `workflow-issues/benchmark-staleness-and-apply-path-20260405.md` — full archaeology

## How A² Works (current state)

```
Task → WorktreeCatalyst(base_ref: bench-baseline | HEAD)
     → model edits files in worktree
     → cargo test in worktree
     → SeedEvaluator builds FitnessRecord
     → Governor decides PromoteGermline | Discard
     → [run mode only] try_apply_patch → verify_and_rebuild
     → lineage.sqlite persists every outcome
```

**Two distinct modes:**
- **`a2ctl bench`** — purely observational. Score = promotion_decision from worktree tests. Never touches workspace.
- **`a2ctl run`** — actually mutates the germline. Applies patch, verifies, rebuilds. Has stagnation auto-switch between providers.

## Commands

```bash
# Bench (observational, safe to run anytime)
cargo run -p a2ctl -- bench --model gemini     # or claude/codex/opencode

# Compare A² vs raw baseline on the same tasks
./bench/baseline.sh zai-coding-plan/glm-5.1

# Run (actually mutates germline — be intentional)
echo "fix X" | cargo run -p a2ctl -- run --provider gemini,opencode --apply

# Run with a specific OpenCode submodel (added 2026-04-16)
echo "fix X" | cargo run -p a2ctl -- run --provider opencode/kimi-for-coding/k2p5
echo "fix X" | cargo run -p a2ctl -- run --provider opencode/minimax-coding-plan/MiniMax-M2.7

# Sentinel gate
cargo run -p a2ctl -- sentinel --workspace .
```

## Available Models (quota state)

| Provider | Model | Status | Notes |
|----------|-------|--------|-------|
| claude | claude-sonnet-4-6 | Available | Burns subscription quota — use sparingly |
| codex | gpt-5.4 | **OUT OF QUOTA** | Don't use until reset |
| gemini | gemini-3.1-pro-preview | Available | 5/5 on current bench, ~67s/task |
| opencode/glm | zai-coding-plan/glm-5.1 | Available | 5/5 on current bench, 10-15min/task (slow) |
| opencode/kimi | kimi-for-coding/k2p5 | Available | 2026-04-16 smoke PASS (75s, 12k tokens); sometimes empty historically |
| opencode/minimax | minimax-coding-plan/MiniMax-M2.7 | Available | 2026-04-16 smoke PASS (72s, 31k tokens) |

## Config constants (a2ctl/src/main.rs)

```rust
const DEFAULT_BENCH_MAX_TOKENS: u64 = 100_000;   // was 50k, too tight
const DEFAULT_BENCH_TIMEOUT_SECS: u64 = 1800;    // was 300, too tight for GLM
const DEFAULT_STAGNATION_WINDOW: usize = 3;
```

## Architecture Quick Reference

| Crate | What it does |
|-------|-------------|
| `a2_core` | Protocol objects, traits, typed IDs, errors |
| `a2_constitution` | Invariants INV-1..5, bootstrap profiles B0/B1/B2 |
| `a2_workcell` | Runtime, WorktreeCatalyst (with_base_ref supported) |
| `a2_membrane` | Tool ACLs, network allowlists, deny-overrides-allow |
| `a2_broker` | 4 model providers (claude/codex/gemini/opencode), token parsing |
| `a2_eval` | SeedEvaluator, 6 sentinels (compile/test/unsafe/clippy/doc/lockfile) |
| `a2_archive` | SQLite lineage store + promotion journal |
| `a2_sensorium` | Signal ingestion, quarantine, risk tiers |
| `a2_raf` | Petgraph causal graph, repair coverage, bottleneck detection, max_depth |
| `a2d` | Governor + StagnationDetector + StrategyChange enum |
| `a2ctl` | CLI: task, run, bench (observational), sentinel, status |

## Current Benchmark Tasks

`bench/tasks/011_015_*.toml` — 5 cross-crate multi-file tasks. Both Gemini and GLM score 5/5. These are measuring model capability, not A² loop value. The baseline (raw model on same tasks in a worktree) also scores 5/5.

**The benchmark is measuring the wrong thing.** See `docs/solutions/best-practices/benchmark-the-loop-not-the-model-20260404.md`. Single-pass tasks don't exercise the loop, so the system's machinery (Governor, evaluator, promoter, stagnation detector) adds no measurable value.

## What To Do Next

ContextPack is now wired (2026-04-16, c32b657) — the catalyst sees prior attempts on the same task. That unblocks loop-shaped benchmarks.

**Known gotchas before building them:**
- `crates/a2d/src/governor.rs` is dead code (not declared as a module in lib.rs). Delete or promote.
- `StrategyChange::DecomposeTask` and `RaiseTemperature` are returned but never acted on. Only `SwitchModel` branches in a2ctl (main.rs:368).
- Prior motifs currently render model + pass/fail/tokens/duration but not *rationale* or *what changed*. Lineage stores `patch_id` but the diff/rationale are on `PatchBundle`, not persisted with lineage. Enriching this will matter for self-correction — the model needs to know *why* the previous attempt failed, not just *that* it did.

The #1 priority is designing a benchmark that A² should actually win at — one that exercises the loop:

1. **Multi-round benchmark**: N iterations on the same task, measure improvement over rounds. Needs stagnation detector + provider rotation + lineage to improve score.
2. **Self-correction benchmark**: inject a bug, measure whether A² finds and fixes it autonomously without being told what's wrong.
3. **Cross-task transfer**: solve task A, measure if solving related task B is faster/better because A² learned from task A's lineage.
4. **SWE-bench Lite integration**: real-world multi-step problems where single-pass models struggle.
5. **Adversarial drift**: can A² detect and reject a "promotion" that actually degrades the system? (Fontana Level 0 test.)

Until one of these is implemented, benchmark scores are not evidence that A² works as designed.

**Secondary:**
- Auto-generate benchmark tasks from codebase gaps → raise ceiling continuously
- Query lineage data for strategy decisions (which model works best on which task type)
- Test Claude on current bench (untested)

## What NOT To Do

- Don't ask for direction. The project name is the instruction. See `autopoietic-no-pausing`.
- Don't draw conclusions from a single benchmark run. Variance across runs is huge (5/5, 4/5, 3/5 observed on same model/tasks). See `single-run-conclusions`.
- Don't put interpretation in HANDOFF.md. State facts. See `handoff-editorial-creep`.
- Don't burn Claude quota on routine coding tasks — use Gemini or OpenCode.
- Don't assume budget/timeout failures are capability failures. Recalibrate first. See `budget-variance-as-noise-floor`.
- Don't run multiple benchmarks concurrently with manual editing — workspace residue fights with edits (if this ever regresses: make benchmark purely observational, see `observational-evaluation-no-mutation`).

## bench-baseline Tag

The `bench-baseline` git tag pins worktree branching point for the bench command. Currently at the commit that added tasks 011-015 but before their implementations. When adding new benchmark tasks, re-tag: `git tag -f bench-baseline <commit-before-impls>`.

## Decision Log

| Date | Decision | Why |
|------|----------|-----|
| 2026-04-01 | Colony-of-workcells over monolithic agent | Codex proposal strongest in adversarial review |
| 2026-04-01 | RAF closure as metric not invariant | Catalysis predicate undefined for software |
| 2026-04-02 | WorktreeCatalyst over GeneralistCatalyst | Models can't produce git-apply-compatible diffs |
| 2026-04-02 | Benchmark-driven loop over task-count loop | 14 overnight rounds with zero capability improvement |
| 2026-04-04 | OpenCode models added | Codex quota exhausted |
| 2026-04-05 | Apply path fix: revert before apply | Diff context mismatch with modified workspace |
| 2026-04-05 | WorktreeCatalyst::with_base_ref() | Pin benchmarks to stable baseline |
| 2026-04-05 | StrategyChange enum | Actionable stagnation response |
| 2026-04-05 | Governor.with_lineage_store() | Auto-persist lineage records |
| 2026-04-05 | Benchmark made purely observational (-219 lines) | Evaluation must not touch the germline |
| 2026-04-05 | Hard bench tasks 011-015 replace easy 006-010 | Both models hit 5/5 on easy tasks, need differentiation |
| 2026-04-06 | Budget 50k→100k, timeout 300s→1800s | False negatives from arbitrary limits, not model/system |
| 2026-04-07 | Current bench doesn't measure A² value-add | A² = raw model on single-pass tasks. Need loop-shaped benchmark. |
| 2026-04-16 | ContextPack wired with prior lineage (c32b657) | Prior attempts + motifs now surface to the catalyst. Prerequisite for any loop-shaped benchmark — before this, multi-round and self-correction had no memory across rounds. |
| 2026-04-16 | a2ctl accepts `opencode/<model_id>` (b432129) | Minimax and Kimi were unreachable from the CLI even though the broker supported them. 4/4 providers smoke-clean post-wiring. |
