# A² Session Resume — 2026-04-04

## State

**Tests:** 57 pass | **Sentinels:** 6/6 PASS | **Benchmark:** 4/5 (Claude), 1/5 (Gemini)
**Germline mutations:** 12+ self-authored | **Crates:** 11, all implemented

## Verify

```bash
cd /Users/thomas/Projects/lightless-labs/skunkworks/a2-autopoietic-autocatalysis
git log --oneline -5
cargo test
cargo run -p a2ctl -- sentinel --workspace .
cargo run -p a2ctl -- bench --model claude  # the real fitness signal
```

## What Works

```bash
# Single task with auto-apply + self-recompilation
cargo run -p a2ctl -- task "title" "desc" --model claude --apply

# Batch with round-robin providers
echo "task1\ntask2" | cargo run -p a2ctl -- run --provider codex,gemini --apply

# Benchmark (X/5 verified tasks)
cargo run -p a2ctl -- bench --model claude
```

## Architecture

**WorktreeCatalyst** is the working catalyst (not GeneralistCatalyst). Creates git worktree → model agent edits files directly → `git diff` → `git apply` → `verify_and_rebuild` (check/test/clippy/build --release). Supports claude, codex, gemini, opencode.

**Benchmark-driven loop pattern:**
1. Sentinel gate → fix if failing
2. Commit pending changes
3. Run `a2ctl bench` → get score X/5
4. If < 5/5 → generate tasks targeting failures
5. If 5/5 → generate harder benchmarks
6. Repeat

## Model Performance

| Model | Benchmark | Quota Status |
|-------|-----------|-------------|
| Claude Sonnet 4.6 | 4/5 | Subscription limited |
| Codex gpt-5.4 | untested | Resets Apr 8 |
| Gemini 3.1 Pro | 1/5 | Available |
| OpenCode GLM/Minimax/Kimi | untested | Available (coding plans) |

## Known Issues

1. **Fibonacci bench task** fails `git apply` — path mismatch between worktree and repo root
2. **Gemini worktree** edits often don't land (1/5 vs Claude's 4/5)
3. **Lineage persistence** not wired — governor doesn't persist to SQLite yet
4. **Stagnation detector** warns but doesn't auto-adapt strategy yet

## What's Next

1. Get benchmark to 5/5 (fix the apply path issue)
2. Test OpenCode models on benchmark
3. Add harder benchmark tasks once 5/5 achieved
4. Wire stagnation detector into benchmark loop for auto-adaptation
5. Lineage persistence
6. SWE-bench Lite integration (eval harness ready in bench/)

## Don't Ask Thomas

The project name is the instruction. Read the benchmark score, improve it, repeat.
