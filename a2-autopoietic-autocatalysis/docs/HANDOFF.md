# A² Handoff — Read This First

**Last updated:** 2026-04-05
**Update this file:** before context compaction, at session end, or when significant state changes.

## What Is This

A² (Autopoietic Autocatalysis) is an autonomous software factory that modifies its own source code. It uses AI model CLIs (Claude, Codex, Gemini, OpenCode) as "food set" models that edit code in git worktrees, then the system verifies, applies, and recompiles itself.

## Current Numbers

| Metric | Value |
|--------|-------|
| Tests | 61 |
| Sentinels | 6/6 PASS |
| Benchmark (Claude) | untested on new tasks |
| Benchmark (Gemini) | 5/5 |
| Benchmark (OpenCode) | partial (killed — fought with workspace) |
| Germline mutations | 12+ self-authored |
| Crates | 11 |

## Verify State

```bash
cd /Users/thomas/Projects/lightless-labs/skunkworks/a2-autopoietic-autocatalysis
cargo test
cargo run -p a2ctl -- sentinel --workspace .
cargo run -p a2ctl -- bench --model gemini
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

1. ~~**Fibonacci bench task**: FIXED 2026-04-05 — revert workspace before git apply so diff context matches clean HEAD.~~
2. ~~**Gemini worktree**: FIXED 2026-04-05 — was stale benchmark tasks, not Gemini. Gemini now 5/5 on fresh tasks.~~
3. ~~**Lineage persistence**: FIXED 2026-04-05 — Governor.with_lineage_store(Arc<dyn LineageStore>), wired to lineage.sqlite in a2ctl run.~~
4. ~~**Stagnation detector**: FIXED 2026-04-05 — StrategyChange enum + auto-switch in run loop when SwitchModel recommended.~~
5. **Benchmark staleness**: FIXED 2026-04-05 — bench-baseline tag + WorktreeCatalyst::with_base_ref(). Benchmark now creates worktrees from a pinned commit.
6. ~~**Benchmark residue**: FIXED 2026-04-05 — removed --apply from bench entirely. Benchmark is now purely observational (like autoresearch's evaluate_bpb). -219 lines removed.~~

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

1. Test OpenCode models on benchmark with bench-baseline → find which are viable
2. Auto-generate benchmark tasks from codebase gaps → raise ceiling continuously
3. SWE-bench Lite integration → real-world evaluation (the proof that A² > single-pass)
4. Run Claude on new benchmark tasks → establish baseline score
5. Wire lineage store into bench command (currently only in run command)
6. Query lineage data for strategy decisions (which model works best on which task type)

## Decision Log

| Date | Decision | Why |
|------|----------|-----|
| 2026-04-01 | Colony-of-workcells over monolithic agent | Codex proposal strongest in adversarial review |
| 2026-04-01 | RAF closure as metric not invariant | Catalysis predicate undefined for software |
| 2026-04-02 | WorktreeCatalyst over GeneralistCatalyst | Models can't produce git-apply-compatible diffs |
| 2026-04-02 | Benchmark-driven loop over task-count loop | 14 overnight rounds with zero capability improvement |
| 2026-04-04 | OpenCode models added to worktree catalyst | Codex quota exhausted, need alternatives |
| 2026-04-05 | Apply-path fix: revert workspace before git apply | Diff context from worktree mismatched modified workspace |
| 2026-04-05 | Benchmark tasks 001-005 retired, 006-010 added | Old tasks solved in HEAD, every model said "already there" |
| 2026-04-05 | Gemini promoted from 1/5 to 5/5 | Was stale benchmarks, not model failure |
| 2026-04-05 | StrategyChange enum replaces static string | Enables auto-adaptation when stagnant |
| 2026-04-05 | bench-baseline tag for worktree pinning | Prevents benchmark staleness |
| 2026-04-05 | Lineage persistence wired into Governor | Auto-persists to lineage.sqlite |
| 2026-04-05 | Stagnation auto-adaptation in run loop | SwitchModel rotates providers |
| 2026-04-05 | Benchmark made purely observational | Removed --apply, -219 lines. Benchmark is evaluation, not mutation. |
