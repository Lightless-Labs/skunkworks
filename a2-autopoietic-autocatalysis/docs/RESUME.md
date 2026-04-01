# A² Session Resume — 2026-04-01

## State at Suspend

**Git:** `c0aa2d8` on `main`, pushed to `origin/main`
**Tests:** 36 pass, 0 fail, 0 warnings in `cargo check`
**Sentinel gate:** PASS (3/3)
**Germline mutations:** 8 self-authored
**Autonomous loop:** `a2ctl run` operational — reads tasks from stdin, runs through governor

## What Exists

11 Rust crates, all implemented:

| Crate | Status | Key Feature |
|-------|--------|-------------|
| `a2_core` | Complete | Protocol objects, traits, typed IDs |
| `a2_constitution` | Complete | Invariants INV-1..5, verifier registry, bootstrap profiles B0/B1/B2 |
| `a2_workcell` | Complete | Budget-bounded runtime, generalist catalyst with structured output |
| `a2_membrane` | Complete | Policy engine, tool ACLs, network allowlists |
| `a2_broker` | Complete | 4 providers: Claude, Codex, Gemini, OpenCode. Token parsing. |
| `a2_eval` | Complete | Seed evaluator (structural acceptance), hidden sentinel suite |
| `a2_archive` | Complete | SQLite lineage store + promotion journal |
| `a2_sensorium` | Complete | Signal ingestion with quarantine and risk tiers |
| `a2_raf` | Complete | Petgraph causal graph, repair coverage, bottleneck detection |
| `a2d` | Complete | Governor: task→workcell→evaluate→promote pipeline |
| `a2ctl` | Complete | CLI: task, sentinel, status, hello, run subcommands |

## What Works End-to-End

```bash
# Single task
a2ctl task "title" "description" --model claude

# Autonomous batch
echo "task 1\ntask 2\ntask 3" | a2ctl run --model claude

# Sentinel gate
a2ctl sentinel
```

## What's Next (Priority Order)

1. **Lineage persistence** — governor produces LineageRecords but doesn't persist to SQLite yet. Wire `Arc<dyn LineageStore>` into governor.
2. **Context enrichment** — catalyst doesn't read actual file contents. Add `ContextPack.relevant_files` reading.
3. **Round-robin providers** — the `--provider` cycling flag was promoted but may need verification.
4. **Stage 1 gate** — `//a2:self_host` target: A² can build itself from a clean checkout.
5. **Continuous loop** — `a2ctl run` with a task generator that inspects the codebase for TODOs, failing tests, and coverage gaps.

## How to Resume

```bash
cd /Users/thomas/Projects/lightless-labs/skunkworks/a2-autopoietic-autocatalysis

# Verify state
git log --oneline -5
cargo test
a2ctl sentinel

# Continue self-improvement
echo "Wire Arc<dyn LineageStore> into governor for persistence
Add file content reading to generalist catalyst ContextPack
Add continuous task generation from codebase analysis" | cargo run -p a2ctl -- run --model claude
```

## Key Learnings for Next Session

- **Codex** (`gpt-5.4`, `reasoning_effort=high`) is the most reliable for code generation tasks
- **Claude** produces the best-reasoned patches but uses more tokens
- **Gemini** works but its provider still returns 0 tokens (parsing issue)
- **OpenCode** provider exists but wasn't tested end-to-end yet
- Promotion rate across all rounds: ~50% (higher for Claude, lower for Gemini/Codex)
- The evaluator is deliberately lenient (structural: non-empty diff + tests pass = promote)
- All external CLI dispatch needs `wait` after backgrounding, `< /dev/null` for stdin
