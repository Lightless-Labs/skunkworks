# A² Handoff — Read This First

**Last updated:** 2026-04-04
**Update this file:** before context compaction, at session end, or when significant state changes.

## What Is This

A² (Autopoietic Autocatalysis) is an autonomous software factory that modifies its own source code. It uses AI model CLIs (Claude, Codex, Gemini, OpenCode) as "food set" models that edit code in git worktrees, then the system verifies, applies, and recompiles itself.

## Current Numbers

| Metric | Value |
|--------|-------|
| Tests | 57 |
| Sentinels | 6/6 PASS |
| Benchmark (Claude) | 4/5 |
| Benchmark (Gemini) | 1/5 |
| Benchmark (OpenCode) | untested |
| Germline mutations | 12+ self-authored |
| Crates | 11 |

## Verify State

```bash
cd /Users/thomas/Projects/lightless-labs/skunkworks/a2-autopoietic-autocatalysis
cargo test
cargo run -p a2ctl -- sentinel --workspace .
cargo run -p a2ctl -- bench --model claude
```

If any of these fail, fix them before doing anything else.

## How A² Works

```
Task → WorktreeCatalyst → model edits files in git worktree → git diff
→ git apply → cargo check → cargo test → clippy → cargo build --release
→ new binary on next run
```

**Key:** WorktreeCatalyst (not GeneralistCatalyst). The old one produced text diffs that never applied. The new one lets models edit files directly.

## The Autonomous Loop

```
sentinel gate → commit → benchmark (X/5) → target failures → improve → repeat
```

To run it:
```bash
# Benchmark-driven loop (the correct pattern)
cargo run -p a2ctl -- bench --model <provider>  # get score
# Then target the failures with improvement tasks
echo "fix description" | cargo run -p a2ctl -- run --provider <providers> --apply
```

Don't run the old pattern of repeating solved tasks — run benchmarks, target failures.

## Available Models

| Provider | Model | How to use | Notes |
|----------|-------|-----------|-------|
| claude | claude-sonnet-4-6 | `--model claude` | Best (4/5). Burns subscription quota. |
| codex | gpt-5.4 | `--model codex` | Reliable. Use `-c 'model_reasoning_effort="high"'`. Check quota. |
| gemini | gemini-3.1-pro-preview | `--model gemini` | 1/5. Worktree edits often don't land. |
| opencode | various | `--model opencode` | GLM, Minimax, Kimi via coding plans. Untested on benchmark. |

Check quota before choosing: `codex --version`, `gemini --version`, `opencode models`.

## Known Bugs (Fix These)

1. **Fibonacci bench task**: `git apply` fails — path mismatch between worktree root and repo root. Fix `try_apply_patch()` in `a2ctl/src/main.rs` to apply from workspace root.
2. **Gemini worktree**: Most edits don't land (1/5). Likely needs better prompt or `-s false` not taking effect.
3. **Lineage persistence**: Governor produces LineageRecords but doesn't persist to SQLite. Wire `Arc<dyn LineageStore>` into Governor.
4. **Stagnation detector**: Warns but doesn't auto-adapt. Should switch models or strategies when stagnant.

## Architecture Quick Reference

| Crate | What it does |
|-------|-------------|
| `a2_core` | Protocol objects, traits, typed IDs, errors |
| `a2_constitution` | Invariants INV-1..5, bootstrap profiles B0/B1/B2 |
| `a2_workcell` | Runtime, GeneralistCatalyst (legacy), **WorktreeCatalyst** (use this) |
| `a2_membrane` | Tool ACLs, network allowlists, deny-overrides-allow |
| `a2_broker` | 4 model providers, token parsing, CoreAdapter bridge |
| `a2_eval` | SeedEvaluator (structural), 6 sentinels (compile/test/unsafe/clippy/doc/lockfile) |
| `a2_archive` | SQLite lineage store + promotion journal |
| `a2_sensorium` | Signal ingestion, quarantine, risk tiers |
| `a2_raf` | Petgraph causal graph, repair coverage, bottleneck detection |
| `a2d` | Governor (task→workcell→evaluate→promote), StagnationDetector |
| `a2ctl` | CLI: task, run, bench, sentinel, status. --apply flag. verify_and_rebuild(). |

## Key Files

- `DESIGN.md` — architecture, theory, invariants (v0.3.0, 2 adversarial passes)
- `docs/plans/phase-1-implementation.md` — ordered build plan
- `bench/tasks/*.toml` — benchmark tasks (the fitness signal)
- `bench/eval.py`, `bench/score.py`, `bench/generate_tasks.py` — evaluation harness

## What NOT To Do

- Don't ask Thomas for direction. The project name is the instruction.
- Don't run the old GeneralistCatalyst — it produces text diffs that never apply.
- Don't optimize test count or promotion rate — optimize benchmark score.
- Don't burn Claude quota on routine tasks — use Codex/Gemini/OpenCode.
- Don't spin on solved tasks — if benchmark shows 5/5, generate harder benchmarks.

## What To Do Next

1. Fix the fibonacci apply path issue → 5/5 benchmark
2. Test OpenCode models on benchmark → find which are viable
3. Wire stagnation detector into benchmark loop → auto-adapt on plateau
4. Add harder benchmark tasks → raise the ceiling
5. SWE-bench Lite integration → real-world evaluation
6. Lineage persistence → track what actually worked

## Decision Log

| Date | Decision | Why |
|------|----------|-----|
| 2026-04-01 | Colony-of-workcells over monolithic agent | Codex proposal strongest in adversarial review |
| 2026-04-01 | RAF closure as metric not invariant | Catalysis predicate undefined for software |
| 2026-04-02 | WorktreeCatalyst over GeneralistCatalyst | Models can't produce git-apply-compatible diffs |
| 2026-04-02 | Benchmark-driven loop over task-count loop | 14 overnight rounds with zero capability improvement |
| 2026-04-04 | OpenCode models added to worktree catalyst | Codex quota exhausted, need alternatives |
