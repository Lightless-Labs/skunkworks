# A² Handoff — Read This First

**Last updated:** 2026-05-30
**Update this file:** before context compaction, at session end, or when significant state changes.

## What Is This

A² (Autopoietic Autocatalysis) is an autonomous software factory that modifies its own source code. It uses AI model CLIs (Claude, Codex, Gemini, OpenCode, Pi) as "food set" models that edit code in git worktrees, then the system verifies, scores, and optionally applies the patches to its own germline.

## Current Numbers (as of 2026-05-27)

| Metric | Value |
|--------|-------|
| Tests | 110 Rust + 12 self-correction Python tests |
| Sentinels | 6/6 PASS |
| Crates | 11 |
| Benchmark (OpenCode/GLM via A²) | 5/5 (with 100k token / 1800s budget) |
| Benchmark (OpenCode/GLM raw, no A²) | 5/5 |
| Benchmark (Gemini 3.1 Pro) | 5/5 |
| Benchmark (Claude) | untested on current task set |
| A² value-add on single-pass tasks | None measurable |
| Self-correction loop (Minimax/Kimi original compound fixtures) | each provider resolved/self-corrected 3/3 on `compound-hidden`, `compound-membrane-hidden`, and `compound-archive-hidden` after hidden candidate verifier wiring |
| Self-correction loop (Pi/ZAI GLM) | resolved/self-corrected 3/3 on `compound-hidden`, `compound-membrane-hidden`, `compound-archive-hidden`, and `compound-sensorium-same-crate-hidden` |
| 4-provider smoke (2026-04-16) | 4/4 PASS (gemini, glm-5.1, minimax-2.7, kimi k2.5) post ContextPack wiring |

## Verify State (run these first)

```bash
cd /Users/thomas/Projects/lightless-labs/skunkworks/a2-autopoietic-autocatalysis
cargo test                                    # expect pass (106 Rust tests)
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
- `logic-errors/worktree-agent-commits-hidden-from-diff-20260530.md` — before modifying WorktreeCatalyst diff capture
- `best-practices/autopoietic-no-pausing-20260404.md` — the project name is the instruction
- `best-practices/contextpack-is-a-load-bearing-extension-point-20260416.md` — empty extension points silently invalidate downstream benchmarks
- `best-practices/external-verification-failures-are-authoritative-20260508.md` — prior verifier failures update the task boundary

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
- `integration-issues/opencode-glm-insufficient-balance-hidden-by-timeout-20260522.md` — before treating GLM timeouts as budget/model evidence
- `logic-errors/verification-failure-stdout-hidden-by-stderr-20260503.md` — before changing verification failure rendering
- `logic-errors/verification-failure-focus-buried-by-passing-tests-20260508.md` — before changing prior failure motifs
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

# Run with ZAI through Pi (added 2026-05-22)
echo "fix X" | cargo run -p a2ctl -- run --provider pi/zai/glm-5.1 --apply

# Self-correction benchmark (isolated worktree; does not mutate germline)
bench/self_correction.py --fixture fibonacci --provider opencode/minimax-coding-plan/MiniMax-M2.7 --attempts 3
bench/self_correction.py --fixture compound-hidden --provider opencode/minimax-coding-plan/MiniMax-M2.7 --attempts 3
bench/self_correction.py --fixture compound-hidden --provider pi/zai/glm-5.1 --attempts 3
bench/self_correction_score.py bench/self-correction-results.jsonl

# Sentinel gate
cargo run -p a2ctl -- sentinel --workspace .
```

## Available Models (quota state)

| Provider | Model | Status | Notes |
|----------|-------|--------|-------|
| claude | claude-sonnet-4-6 | Available | Burns subscription quota — use sparingly |
| codex | gpt-5.4 | **OUT OF QUOTA** | Don't use until reset |
| gemini | gemini-3.1-pro-preview | **OUT OF CAPACITY** | 2026-04-28 self-correction smoke hit repeated 429 capacity errors; previous bench 5/5, ~67s/task |
| opencode/glm | zai-coding-plan/glm-5.1 | Not currently used | 2026-05-22 direct `opencode --print-logs` smoke returned `Insufficient balance or no resource package` before subscription restore; prefer Pi/ZAI route below. Previous bench was 5/5 when provider was funded, 10-15min/task. |
| pi/zai | zai/glm-5.1 | Available | Added 2026-05-22. Uses Pi's built-in ZAI provider and existing `~/.pi/agent/auth.json` `zai` API key. Fibonacci calibration passed attempt 1 with token accounting (`/tmp/a2-pi-zai-fibonacci-json-usage.jsonl`); the three original compound fixtures and the same-crate Sensorium fixture resolved/self-corrected 3/3 via Pi/ZAI (`/tmp/a2-compound-hidden-pi-zai-glm.jsonl`, `/tmp/a2-compound-membrane-pi-zai-glm.jsonl`, `/tmp/a2-compound-archive-pi-zai-glm.jsonl`, `/tmp/a2-sensorium-same-crate-pi-zai-glm.jsonl`). |
| opencode/kimi | kimi-for-coding/k2p5 | Available | 2026-04-16 smoke PASS (75s, 12k tokens); sometimes empty historically |
| opencode/minimax | minimax-coding-plan/MiniMax-M2.7 | Available | 2026-04-28 self-correction PASS attempt 1 (70s model time, 17.6k tokens); 2026-04-16 smoke PASS |

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
| `a2_broker` | model providers (claude/codex/gemini/opencode/pi), token parsing |
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

ContextPack is wired and self-correction harnesses exist. Minimax, Kimi, and Pi/ZAI GLM now have N=3 self-correction success on all three original compound fixtures after hidden candidate-worktree verifier wiring. `compound-sensorium-same-crate-hidden` was added on 2026-05-24 to move beyond visible-core-plus-hidden-second-crate regressions. Pi/ZAI GLM and Minimax resolved/self-corrected it 3/3; Kimi resolved 3/3 with pass@1 1/3 and self-corrected 2/3. `compound-raf-same-crate-hidden` was added on 2026-05-29 and smoke-only injection verified both RAF failures. Minimax resolved it 3/3 with pass@1 1/3 and self-corrected 2/3. Kimi and Pi/ZAI GLM resolved it 3/3 with pass@1 3/3. During the Pi/ZAI run, one resolved attempt had an empty captured patch; WorktreeCatalyst now captures committed worktree changes by diffing against the pre-agent base commit. A post-base-diff-fix Pi/ZAI RAF N=3 still produced one verifier-success attempt with empty patch stats; provider subprocesses now receive `PWD=<candidate worktree>` alongside `current_dir`. Post-PWD Pi/ZAI RAF N=3 had no empty verifier-success patch stats. `compound-eval-same-crate-hidden` was added on 2026-05-30 and smoke-only injection verified both eval failures. Minimax looped on it for N=3 without resolving. Remaining work is Kimi/Pi validation for the eval fixture, more fixture diversity, and ablations.

### Current loop status (2026-05-30)

**Working:**
- Prior lineage reaches retry attempts for a pinned `TaskId`.
- `a2ctl run --apply` reconciles post-apply `git apply` + `verify_and_rebuild` truth into persisted lineage.
- Prior motifs render structured `external_verification` blocks, stdout-first details, and `failure_focus` lines.
- WorktreeCatalyst prompt states prior `external_verification` failures are authoritative acceptance criteria.
- Self-correction JSONL records touched files and added/removed line counts.
- Structured external verification is persisted in `LineageRecord.external_verifications`.
- Retry task contracts include verifier-derived acceptance criteria.
- Retry context includes verifier-derived relevant files when verifier output names Rust source paths.
- Retry context includes `anti_repeat_retry` warnings when latest failed patch touched files do not overlap unresolved verifier-derived Rust source paths; repeated failed touched-file sets are counted.
- Anti-repeat retry can be disabled for ablation via `a2ctl run --disable-anti-repeat-retry`; `bench/self_correction.py --disable-anti-repeat` forwards the flag and records `anti_repeat_retry_enabled`/`ablation` in JSONL. `bench/self_correction_score.py` reports ablation cohorts when enabled and disabled records are scored together.
- `a2ctl autopilot` exists as the in-repo continuous self-iteration entrypoint. It accepts explicit tasks via repeated `--task` and `--task-file`; otherwise it discovers unchecked checklist items in `todos/` and `docs/plans/` plus code TODO/FIXME scan candidates. It pins stable task IDs from explicit task content or candidate source locations, logs JSONL events under `.a2/autopilot/runs/<run-id>/events.jsonl`, writes `run_summary.json` with per-iteration patch stats/verifier focus/model/apply fields plus `stop_reason`, appends aggregate records to `.a2/autopilot/run_index.jsonl`, updates `.a2/autopilot/latest_run.json`, supports `--dry-run`, and only mutates the workspace when `--apply` is explicit. `a2ctl autopilot-resident` wraps the normal autopilot command for repeated interval runs, supports `--max-runs` bounded operation or `0` for until-interrupted residency, and writes resident events plus per-run stdout/stderr under `.a2/autopilot/resident/<resident-id>/`.
- Autopilot checklist candidates now update their own source checklist only after verified application (`apply_ok && verify_ok`). The update is restricted to `todos/...:<line>` and `docs/plans/...:<line>` sources, converts the exact line from `- [ ]`/`* [ ]` to checked, logs a `checklist_update` event, and stores the update summary in `run_summary.json`.
- Task-specific verifier commands are represented on `TaskContract` and run inside candidate worktrees before promotion scoring; verifier results are stored on `PatchBundle.worktree_verifications` and copied into `LineageRecord.external_verifications`. Verifier commands are not rendered into the initial prompt; failures surface through structured lineage after an attempted patch.
- `bench/self_correction.py` passes each fixture's verifier command via JSONL `verification_commands` as of 2026-05-21. Verifier commands are system-side metadata and are not rendered in the initial prompt.
- A² supports `pi` and `pi/<model_id>` providers as of 2026-05-22. Default `pi` model is `zai/glm-5.1`; explicit form is `pi/zai/glm-5.1`. WorktreeCatalyst runs `pi --mode json --no-session --print` from the candidate worktree and parses final text plus token usage.
- `compound-hidden` with Kimi on 2026-05-21 after hidden candidate-worktree verifier wiring resolved 3/3 runs; pass@1 was 0/3; loop exercised 3/3; self-corrected 3/3. Results: `/tmp/a2-compound-with-hidden-worktree-verifier-kimi.jsonl`.
- `compound-hidden` with Minimax on 2026-05-21 after hidden candidate-worktree verifier wiring resolved 3/3 runs; pass@1 was 0/3; loop exercised 3/3; self-corrected 3/3. Attempt 1 touched only `a2_core/src/lib.rs` and was discarded by candidate verifier; attempt 2 touched both `a2_core/src/lib.rs` and `a2ctl/src/main.rs` and verified clean. Results: `/tmp/a2-compound-with-hidden-worktree-verifier-minimax.jsonl`.
- `compound-membrane-hidden` with Minimax on 2026-05-21 after hidden candidate-worktree verifier wiring resolved 3/3 runs; pass@1 was 0/3; loop exercised 3/3; self-corrected 3/3. In all three runs attempt 1 touched only `a2_core/src/lib.rs`; attempt 2 touched both `a2_core/src/lib.rs` and `a2_membrane/src/policy.rs` and verified clean. Results: `/tmp/a2-compound-membrane-with-hidden-worktree-verifier-minimax.jsonl`.
- `compound-membrane-hidden` with Kimi on 2026-05-21 after hidden candidate-worktree verifier wiring resolved 3/3 runs; pass@1 was 0/3; loop exercised 3/3; self-corrected 3/3. In all three runs attempt 1 touched only `a2_core/src/lib.rs`; attempt 2 touched both `a2_core/src/lib.rs` and `a2_membrane/src/policy.rs` and verified clean. Results: `/tmp/a2-compound-membrane-with-hidden-worktree-verifier-kimi.jsonl`.
- `compound-archive-hidden` with Minimax on 2026-05-22 after hidden candidate-worktree verifier wiring resolved 3/3 runs; pass@1 was 0/3; loop exercised 3/3; self-corrected 3/3. In all three runs attempt 1 touched only `a2_core/src/lib.rs`; attempt 2 touched both `a2_core/src/lib.rs` and `a2_archive/src/store.rs` and verified clean. Results: `/tmp/a2-compound-archive-hidden-minimax.jsonl`.
- `compound-archive-hidden` with Kimi on 2026-05-22 after hidden candidate-worktree verifier wiring resolved 3/3 runs; pass@1 was 0/3; loop exercised 3/3; self-corrected 3/3. In all three runs attempt 1 touched only `a2_core/src/lib.rs`; attempt 2 touched both `a2_core/src/lib.rs` and `a2_archive/src/store.rs` and verified clean. Results: `/tmp/a2-compound-archive-hidden-kimi.jsonl`.
- `compound-hidden` with Pi/ZAI GLM on 2026-05-22 resolved 3/3 runs; pass@1 was 0/3; loop exercised 3/3; self-corrected 3/3. In all three runs attempt 1 touched only `a2_core/src/lib.rs`; attempt 2 touched both `a2_core/src/lib.rs` and `a2ctl/src/main.rs` and verified clean. Results: `/tmp/a2-compound-hidden-pi-zai-glm.jsonl`.
- `compound-membrane-hidden` with Pi/ZAI GLM on 2026-05-24 resolved 3/3 runs; pass@1 was 0/3; loop exercised 3/3; self-corrected 3/3. In all three runs attempt 1 touched only `a2_core/src/lib.rs`; attempt 2 touched both `a2_core/src/lib.rs` and `a2_membrane/src/policy.rs` and verified clean. Results: `/tmp/a2-compound-membrane-pi-zai-glm.jsonl`.
- `compound-archive-hidden` with Pi/ZAI GLM on 2026-05-24 resolved 3/3 runs; pass@1 was 0/3; loop exercised 3/3; self-corrected 3/3. In all three runs attempt 1 touched only `a2_core/src/lib.rs`; attempt 2 touched both `a2_core/src/lib.rs` and `a2_archive/src/store.rs` and verified clean. Results: `/tmp/a2-compound-archive-pi-zai-glm.jsonl`.
- `compound-sensorium-same-crate-hidden` was added on 2026-05-24. It injects two regressions in `crates/a2_sensorium/src/ingest.rs`: visible `RiskTier::High` priority behavior and hidden title truncation behavior. Smoke-only injection verified both failures. Pi/ZAI GLM resolved 3/3 runs; pass@1 was 0/3; loop exercised 3/3; self-corrected 3/3. In all three runs attempt 1 touched `a2_sensorium/src/ingest.rs` with +1/-1 and failed the hidden verifier; attempt 2 touched the same file with +2/-2 and verified clean. Results: `/tmp/a2-sensorium-same-crate-pi-zai-glm.jsonl`.
- `compound-sensorium-same-crate-hidden` with Minimax on 2026-05-24 resolved 3/3 runs; pass@1 was 0/3; loop exercised 3/3; self-corrected 3/3. All runs touched `a2_sensorium/src/ingest.rs` on attempts 1 and 2. Results: `/tmp/a2-sensorium-same-crate-minimax.jsonl`.
- `compound-sensorium-same-crate-hidden` with Kimi on 2026-05-24 resolved 3/3 runs; pass@1 was 1/3; loop exercised 2/3; self-corrected 2/3. Two runs resolved on attempt 2 after prior lineage; one run resolved on attempt 1. Results: `/tmp/a2-sensorium-same-crate-kimi.jsonl`.
- `compound-raf-same-crate-hidden` was added on 2026-05-29. It injects two regressions in `crates/a2_raf/src/graph.rs`: visible single-node RAF connectivity behavior and hidden empty-graph repair coverage behavior. Smoke-only injection verified both failures. Result: `/tmp/a2-raf-fixture-smoke.jsonl`.
- `compound-raf-same-crate-hidden` with Minimax on 2026-05-29 resolved 3/3 runs; pass@1 was 1/3; loop exercised 2/3; self-corrected 2/3. One run resolved on attempt 1; two runs failed attempt 1 and resolved on attempt 2 after prior lineage. Result: `/tmp/a2-raf-same-crate-minimax-20260529T212431Z.jsonl`.
- `compound-raf-same-crate-hidden` with Kimi on 2026-05-30 resolved 3/3 runs; pass@1 was 3/3; loop exercised 0/3; self-corrected 0/3. Result: `/tmp/a2-raf-same-crate-kimi-20260530T071018Z.jsonl`.
- `compound-raf-same-crate-hidden` with Pi/ZAI GLM on 2026-05-30 resolved 3/3 runs; pass@1 was 3/3; loop exercised 0/3; self-corrected 0/3. One run resolved with empty captured patch stats and `lineage_reconciled_by_core=false`; this exposed the committed-worktree diff capture gap fixed in `WorktreeCatalyst`. Result: `/tmp/a2-raf-same-crate-pi-zai-glm-20260530T072430Z.jsonl`.
- WorktreeCatalyst diff capture now records committed worktree changes. It resolves the candidate worktree base commit before running the agent, then stages all changes and diffs against that base commit, covering uncommitted, staged, and committed edits. See `docs/solutions/logic-errors/worktree-agent-commits-hidden-from-diff-20260530.md`.
- Post-base-diff-fix Pi/ZAI GLM N=3 on `compound-raf-same-crate-hidden` on 2026-05-30 resolved 3/3 runs; pass@1 2/3; loop exercised 1/3; self-corrected 1/3. Two verifier-success attempts had populated patch stats; one pass@1 attempt still had `touched_file_count=0`, +0/-0, and `lineage_reconciled_by_core=false`. Result: `/tmp/a2-raf-same-crate-pi-zai-glm-post-diff-fix-20260530T073729Z.jsonl`.
- WorktreeCatalyst now sets `PWD` to the candidate worktree for Claude, Codex, Gemini, OpenCode, and Pi provider subprocesses so environment-based path resolution matches `current_dir`. See addendum in `docs/solutions/logic-errors/worktree-agent-commits-hidden-from-diff-20260530.md`.
- Post-PWD Pi/ZAI GLM N=3 on `compound-raf-same-crate-hidden` on 2026-05-30 resolved 3/3 runs; pass@1 2/3; loop exercised 1/3; self-corrected 1/3. All verifier-success attempts had `touched_file_count=1` and populated +2/-2 patch stats; empty verifier-success patch stats were 0/3 runs. Result: `/tmp/a2-raf-same-crate-pi-zai-glm-post-pwd-20260530T075028Z.jsonl`.
- `compound-eval-same-crate-hidden` was added on 2026-05-30. It injects two regressions in `crates/a2_eval/src/seed.rs`: visible failing-test scoring behavior and hidden token-budget scoring behavior. Smoke-only injection verified both failures. Result: `/tmp/a2-eval-fixture-smoke.jsonl`.
- `compound-eval-same-crate-hidden` with Minimax on 2026-05-30 resolved 0/3 runs; pass@1 was 0/3; loop exercised 3/3; self-corrected 0/3. Each run reached three attempts, touched `a2_eval/src/seed.rs`, and still failed both visible and hidden verifier tests. Result: `/tmp/a2-eval-same-crate-minimax-20260530T080351Z.jsonl`.
- Anti-repeat ablation on `compound-hidden` with Minimax on 2026-05-28 completed N=3 per cohort. Enabled cohort: resolved 3/3, pass@1 0/3, loop exercised 3/3, self-corrected 3/3; resolved attempts were 3, 2, 2. Disabled cohort: resolved 3/3, pass@1 0/3, loop exercised 3/3, self-corrected 3/3; all resolved on attempt 2. Result: `/tmp/a2-anti-repeat-ablation-compound-hidden-minimax-20260528T122327Z.jsonl`.
- Anti-repeat ablation on `compound-sensorium-same-crate-hidden` with Minimax on 2026-05-28 completed N=3 per cohort. Enabled cohort: resolved 3/3, pass@1 0/3, loop exercised 3/3, self-corrected 3/3; all resolved on attempt 2. Disabled cohort: resolved 3/3, pass@1 0/3, loop exercised 3/3, self-corrected 3/3; all resolved on attempt 2. All attempts touched `a2_sensorium/src/ingest.rs`; first attempts were +1/-1 and resolved attempts were +2/-2 except one enabled run at +4/-2. Result: `/tmp/a2-anti-repeat-ablation-sensorium-minimax-20260528T221811Z.jsonl`.
- The 2026-05-20 Minimax/Kimi reruns happened after candidate verifier code existed but before the self-correction harness passed verifier commands, so those reruns exercised post-apply verification/retry context rather than candidate-worktree verifier scoring.
- `compound-hidden` with Kimi on 2026-05-20 after anti-repeat + task-verifier code changes resolved 3/3 runs; pass@1 was 0/3; loop exercised 3/3; self-corrected 3/3. Results: `/tmp/a2-compound-after-task-verifier-kimi.jsonl`.
- `compound-hidden` with Minimax on 2026-05-20 after anti-repeat + task-verifier code changes resolved 3/3 runs; pass@1 was 0/3; loop exercised 3/3; self-corrected 3/3. In all three runs attempt 1 touched only `a2_core/src/lib.rs`; attempt 2 touched both `a2_core/src/lib.rs` and `a2ctl/src/main.rs` and verified clean. Results: `/tmp/a2-compound-after-task-verifier-minimax.jsonl`.
- `compound-hidden` with Minimax on 2026-05-16 resolved 3/3 runs; pass@1 was 0/3; loop exercised 3/3; self-corrected 3/3. In all three runs attempt 1 touched only `a2_core/src/lib.rs`; attempt 2 touched both `a2_core/src/lib.rs` and `a2ctl/src/main.rs` and verified clean.
- `compound-hidden` with Kimi on 2026-05-18 resolved 3/3 runs; pass@1 was 0/3; loop exercised 3/3; self-corrected 3/3. In all three runs attempt 1 touched only `a2_core/src/lib.rs`; attempt 2 touched both `a2_core/src/lib.rs` and `a2ctl/src/main.rs` and verified clean.

**Not yet validated:**
- `compound-eval-same-crate-hidden` with Kimi and Pi/ZAI GLM after Minimax 0/3.
- Additional anti-repeat ablation coverage beyond the two Minimax fixture cohorts (`compound-hidden` and `compound-sensorium-same-crate-hidden`).

**Structural solution direction:** Minimax, Kimi, and Pi/ZAI GLM now have N=3 validation on the three original compound fixtures and on the same-crate Sensorium fixture. Kimi had pass@1 1/3 on Sensorium, so its self-correction count there is 2/3 rather than 3/3. The same-crate RAF fixture has smoke-only verification and N=3 results for Minimax, Kimi, and Pi/ZAI GLM; Kimi and Pi/ZAI solved it on attempt 1 in every run. Remaining work is more fixture diversity and measuring anti-repeat contribution. Dedicated todos live in `todos/`.

### Completed prerequisites (2026-04-23)

- [x] **Persist `PatchBundle.rationale` and `diff` alongside `LineageRecord`.** Completed 2026-04-23. Lineage records now carry optional patch diff/rationale, SQLite init migrates legacy lineage tables, and prior-attempt motifs include bounded rationale/diff snippets.
- [x] **Delete or promote `crates/a2d/src/governor.rs`.** Completed 2026-04-23 by deleting the unreferenced shadow implementation; `a2d::Governor` lives in `crates/a2d/src/lib.rs`.
- [x] **Act on `StrategyChange::DecomposeTask` and `::RaiseTemperature`** (or remove them). Completed 2026-04-23 by shrinking `StrategyChange` to `{None, SwitchModel}`, matching the only strategy branch `a2ctl run` actually executes.
- [x] **Give `a2ctl run` a way to pin `TaskId` across invocations.** Completed 2026-04-23. JSONL `task_id` now sets the `TaskContract.id`; `task-<uuid>`/UUID values parse directly and arbitrary external keys map to deterministic typed IDs.
- [x] **Investigate lockfile sentinel failure.** Completed 2026-04-23 by regenerating and committing `Cargo.lock` with `cargo generate-lockfile --offline`; sentinel now passes 6/6.

### Immediate todos

- [x] **Make prior failure motifs harder to ignore.** Completed 2026-05-01. `a2_workcell::runtime::render_prior_motif` now detects persisted `[external verify: FAIL]` notes and renders them as a structured multiline `external_verification` block ahead of rationale/diff snippets.
- [x] **Move verification reconciliation out of the harness.** Completed 2026-05-01. `a2ctl run --apply` now reconciles persisted lineage after `try_apply_patch` + `verify_and_rebuild` via the Governor; `bench/self_correction.py` records whether core reconciliation ran and no longer patches SQLite directly.
- [x] **Instrument what models changed per attempt.** Completed 2026-05-01. `bench/self_correction.py` now records touched files plus added/removed line counts from the latest lineage patch diff for each attempt.
- [x] **Implement structured external verification.** Completed 2026-05-12. `LineageRecord.external_verifications` now stores typed post-apply verifier outcomes; SQLite persists/migrates the field; `a2ctl run --apply` writes structured stdout/stderr/failing-test data; prior motifs render structured records before falling back to legacy rationale markers. See `todos/structured-external-verification.md`.
- [x] **Promote verifier failures into retry task contracts.** Completed 2026-05-12. `a2d::Governor` derives retry acceptance criteria from failed structured external verification records and passes the enriched task into the workcell; `WorktreeCatalyst` renders acceptance criteria in prompts. See `todos/retry-task-contract-from-verification.md`.
- [x] **Populate verifier-derived relevant files.** Completed 2026-05-12. Failed structured verifier output containing Rust source paths now populates `ContextPack.relevant_files`, and `WorktreeCatalyst` renders those paths in prompts. See `todos/verifier-derived-relevant-files.md`.
- [x] **Add anti-repeat retry strategy.** Completed 2026-05-20. Retry context now emits an `anti_repeat_retry` motif when prior failed patch touched files do not overlap unresolved verifier-derived source paths; repeated touched-file sets are counted, and WorktreeCatalyst prompts explicitly warn not to repeat the prior patch shape alone. See `todos/anti-repeat-retry-strategy.md`.
- [x] **Design anti-repeat ablation benchmark.** Completed 2026-05-24. `a2ctl run --disable-anti-repeat-retry` disables only the anti-repeat retry motif; `bench/self_correction.py --disable-anti-repeat` forwards it; JSONL records carry `anti_repeat_retry_enabled`/`ablation`; the scorer prints cohorts when paired enabled/disabled runs are in one log.
- [x] **Run first N≥3 anti-repeat ablation cohort.** Completed 2026-05-28 on `compound-hidden` with Minimax. Enabled and disabled cohorts both scored resolved 3/3, pass@1 0/3, loop exercised 3/3, self-corrected 3/3. Result: `/tmp/a2-anti-repeat-ablation-compound-hidden-minimax-20260528T122327Z.jsonl`.
- [x] **Run same-crate anti-repeat ablation cohort.** Completed 2026-05-28 on `compound-sensorium-same-crate-hidden` with Minimax. Enabled and disabled cohorts both scored resolved 3/3, pass@1 0/3, loop exercised 3/3, self-corrected 3/3; all runs resolved on attempt 2. Result: `/tmp/a2-anti-repeat-ablation-sensorium-minimax-20260528T221811Z.jsonl`.
- [x] **Add in-repo autopilot entrypoint.** Completed 2026-05-25. `a2ctl autopilot` can discover project work from `todos/`, `docs/plans/`, and code TODO/FIXME comments, run bounded workcell iterations, optionally apply verified promoted patches, and write durable JSONL event logs under `.a2/autopilot/`. See `docs/plans/continuous-self-iteration.md`.
- [x] **Add dashboard-friendly aggregate autopilot logs.** Completed 2026-05-26. Completed autopilot runs append compact records to `.a2/autopilot/run_index.jsonl` and update `.a2/autopilot/latest_run.json` with summary metrics, stop reason, log paths, and compact iteration outcomes.
- [x] **Add resident autopilot wrapper.** Completed 2026-05-27. `a2ctl autopilot-resident` repeatedly invokes the normal autopilot command on `--interval-secs`, supports `--max-runs` for bounded smoke/cron operation or `0` for until-interrupted residency, forwards provider/budget/task/apply/dry-run/log options, and writes resident events plus per-run stdout/stderr under `.a2/autopilot/resident/<resident-id>/`.
- [x] **Run task-specific verifier in candidate worktrees before promotion scoring.** Completed 2026-05-20. `TaskContract.verification_commands` carries shell verifier commands; `WorktreeCatalyst` runs them in the candidate worktree, maps outcomes into `TestResults` plus structured `ExternalVerification`, and `run_workcell` persists those verifier records into lineage before promotion. `a2ctl bench` wires TOML `[verify]` commands, and JSONL run input accepts optional `verification_commands`. See `todos/worktree-task-verifier.md`.
- [x] **Run original compound fixtures N≥3 per available non-Claude provider after hidden candidate verifier wiring.** Minimax and Kimi scored resolved/self-corrected 3/3 on `compound-hidden`, `compound-membrane-hidden`, and `compound-archive-hidden`. Pi/ZAI GLM scored resolved/self-corrected 3/3 on `compound-hidden` (`/tmp/a2-compound-hidden-pi-zai-glm.jsonl`), `compound-membrane-hidden` (`/tmp/a2-compound-membrane-pi-zai-glm.jsonl`), and `compound-archive-hidden` (`/tmp/a2-compound-archive-pi-zai-glm.jsonl`). In all Pi/ZAI membrane/archive runs attempt 1 touched only `a2_core/src/lib.rs`; attempt 2 touched the hidden-regression crate and verified clean. Prior OpenCode GLM route timed out while ZAI balance was unavailable; prefer `pi/zai/glm-5.1`.
- [x] **Add a second compound fixture after one self-correction success.** Completed 2026-05-18. `bench/self_correction.py` now includes `compound-membrane-hidden`, which combines the visible `a2_core` Fibonacci regression with a hidden `a2_membrane` deny-overrides-allow regression. Smoke-only injection verified both failures. After hidden candidate-worktree verifier wiring, Minimax N=3 and Kimi N=3 on 2026-05-21 both scored resolved 3/3, pass@1 0/3, loop exercised 3/3, self-corrected 3/3.

### Loop-shaped benchmarks

1. **Self-correction benchmark** *(implemented 2026-04-28 as `bench/self_correction.py` + `bench/self_correction_score.py`)*: isolated git worktree, pinned task ID, core run-path lineage reconciliation, JSONL results. Fixtures: `fibonacci` (too easy: Minimax N=3 pass@1 3/3, loop 0/3; Pi/ZAI GLM passed attempt 1 on 2026-05-22), `compound-hidden` (harder: Minimax/Kimi runs before structured retry context failed attempts 1-3; after structured verifier records + retry acceptance criteria + verifier-derived relevant files, Minimax N=3 on 2026-05-16 and Kimi N=3 on 2026-05-18 resolved on attempt 2 in all runs; after hidden candidate-worktree verifier wiring, Minimax N=3 and Kimi N=3 on 2026-05-21 resolved on attempt 2 in all runs; Pi/ZAI GLM N=3 on 2026-05-22 resolved on attempt 2 in all runs), `compound-membrane-hidden` (added 2026-05-18; visible `a2_core` Fibonacci regression plus hidden `a2_membrane` deny-overrides-allow regression; Minimax N=3 on 2026-05-18 resolved on attempt 2 in all runs; after hidden candidate-worktree verifier wiring, Minimax N=3 and Kimi N=3 on 2026-05-21 resolved on attempt 2 in all runs; Pi/ZAI GLM N=3 on 2026-05-24 resolved on attempt 2 in all runs), `compound-archive-hidden` (added 2026-05-22; visible `a2_core` Fibonacci regression plus hidden `a2_archive` lineage ordering regression; smoke-only injection verified both failures; after hidden candidate-worktree verifier wiring, Minimax N=3 and Kimi N=3 on 2026-05-22 resolved on attempt 2 in all runs; Pi/ZAI GLM N=3 on 2026-05-24 resolved on attempt 2 in all runs), `compound-sensorium-same-crate-hidden` (added 2026-05-24; visible and hidden regressions both in `a2_sensorium/src/ingest.rs`; smoke-only injection verified both failures; Pi/ZAI GLM and Minimax N=3 on 2026-05-24 resolved/self-corrected 3/3; Kimi N=3 resolved 3/3 with pass@1 1/3 and self-corrected 2/3).
2. **Multi-round benchmark**: N iterations on the same task, measure score improvement over rounds. Can now reuse the self-correction harness pattern.
3. **Adversarial drift** (Fontana Level 0): can A² detect and reject a "promotion" that actually degrades the system? Philosophically load-bearing for the autopoiesis claim.
4. **Cross-task transfer**: solve task A, measure if task B is faster/better because lineage carried over.
5. **SWE-bench Lite integration**: real-world multi-step problems. Wide scope — probably last.

Single-pass benchmark scores remain non-evidence for A² loop value.

### Secondary

- Auto-generate benchmark tasks from codebase gaps → raise ceiling continuously
- Query lineage data for strategy decisions (which model works best on which task type)
- Test Claude on current bench (untested)

## What NOT To Do

- Don't ask for direction. The project name is the instruction. See `autopoietic-no-pausing`.
- Don't draw conclusions from a single benchmark run. Variance across runs is huge (5/5, 4/5, 3/5 observed on same model/tasks). See `single-run-conclusions`.
- Don't put interpretation in HANDOFF.md. State facts. See `handoff-editorial-creep`.
- Don't burn Claude quota on routine coding tasks — use OpenCode models or Pi/ZAI when available.
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
| 2026-04-23 | Persist patch diff/rationale in lineage | Prior motifs now include bounded rationale/diff snippets, giving multi-round/self-correction runs a reason to change approach instead of only seeing pass/fail metadata. |
| 2026-04-23 | Delete dead a2d governor shadow module | `crates/a2d/src/governor.rs` was not declared in `lib.rs` and had already drifted from the real `Governor`, including stale `WorkcellConfig` construction. |
| 2026-04-23 | Shrink `StrategyChange` to executed actions | Removed `DecomposeTask` and `RaiseTemperature` because no caller acted on them; stagnant windows now recommend the supported `SwitchModel` action. |
| 2026-04-23 | Pin `a2ctl run` tasks from JSONL `task_id` | Reusing the same `task_id` across invocations now retrieves prior lineage for the same typed `TaskId`, enabling multi-round/self-correction loops to use memory. |
| 2026-04-23 | Refresh Cargo.lock to satisfy sentinel | `cargo generate-lockfile --offline` updated cached compatible package versions; committing that drift restored `lockfile_check` and the sentinel suite is now 6/6. |
| 2026-04-28 | Add self-correction benchmark harness | `bench/self_correction.py` creates isolated bugged worktrees, repeats A² attempts with one pinned task ID, and records per-attempt JSONL including prior-lineage visibility. `fibonacci` showed pass@1, not loop value. `compound-hidden` exercised prior lineage but did not self-correct in 3 attempts. |
| 2026-05-01 | Render external verification failures as structured motifs | Prior-attempt motifs now split `[external verify: FAIL]` notes out of persisted rationale and render `external_verification` as multiline context before rationale/diff snippets, so self-correction attempts see the post-apply failure prominently. |
| 2026-05-01 | Reconcile apply/rebuild outcomes in `a2ctl run` | The run path now updates persisted lineage after `git apply` and `verify_and_rebuild`, replacing pre-apply somatic truth with post-apply germline gate truth. The self-correction harness no longer patches SQLite directly. |
| 2026-05-01 | Add self-correction diff stats | Self-correction JSONL records now include touched files and added/removed line counts from the latest lineage patch diff, making per-attempt behavior distinguishable without inspecting workspaces manually. |
| 2026-05-03 | Preserve stdout in verification failure messages | `cargo test` often writes actionable failing assertion details to stdout while stderr only carries the cargo-level failure summary. `a2ctl` now includes both streams in `command_failure_message()` so lineage motifs can surface exact failures. |
| 2026-05-08 | Put verification stdout before stderr | A Minimax run after stdout preservation still repeated the visible fix; the structured motif's compact detail began with stderr compiler/cargo noise. `command_failure_message()` now renders stdout first so failing assertions are near the front of the persisted note. |
| 2026-05-08 | Add verification failure focus to motifs | A Minimax run after stdout-first still repeated the visible fix because full `cargo test` stdout begins with many passing crates. Failed external verification motifs now include `failure_focus` lines extracted from failed tests/assertions before bounded raw detail. |
| 2026-05-08 | Make external verification authoritative in prompts | WorktreeCatalyst now tells models that prior `external_verification` failures are authoritative acceptance criteria, even when they reveal failures beyond the original task description. |
| 2026-05-12 | Persist structured external verification | `LineageRecord.external_verifications` now stores typed post-apply verifier outcomes, SQLite migrates/persists them, and prior motifs prefer typed verifier records over legacy `[external verify: ...]` rationale text. |
| 2026-05-12 | Promote verifier failures into retry task contracts | `a2d::Governor` now derives mandatory retry acceptance criteria from failed structured external verification records; `WorktreeCatalyst` renders task acceptance criteria in prompts. |
| 2026-05-12 | Populate verifier-derived relevant files | Failed structured verifier output containing source paths now populates `ContextPack.relevant_files`, making paths such as `crates/a2ctl/src/main.rs` visible in retry prompts. |
| 2026-05-16 | First `compound-hidden` N=3 self-correction success | Minimax after structured verifier retry context scored resolved 3/3, pass@1 0/3, loop exercised 3/3, self-corrected 3/3; all runs fixed `a2_core` on attempt 1 and fixed `a2ctl` on attempt 2. |
| 2026-05-18 | `compound-hidden` Kimi N=3 self-correction success | Kimi after structured verifier retry context scored resolved 3/3, pass@1 0/3, loop exercised 3/3, self-corrected 3/3; all runs fixed `a2_core` on attempt 1 and fixed `a2ctl` on attempt 2. |
| 2026-05-18 | Add second compound self-correction fixture | `compound-membrane-hidden` combines the visible `a2_core` Fibonacci regression with a hidden `a2_membrane` deny-overrides-allow regression so loop recovery can be tested beyond `a2ctl` scan-marker failures. |
| 2026-05-18 | `compound-membrane-hidden` Minimax N=3 self-correction success | Minimax scored resolved 3/3, pass@1 0/3, loop exercised 3/3, self-corrected 3/3; all runs fixed `a2_core` on attempt 1 and fixed `a2_membrane` on attempt 2. |
| 2026-05-20 | Add anti-repeat retry motif | Prior failed patch diffs are parsed for touched files. When unresolved verifier failures name Rust source paths not touched by the latest failed patch, retry context adds an `anti_repeat_retry` warning; repeated failed touched-file sets are counted. |
| 2026-05-20 | Run task verifiers in candidate worktrees | `TaskContract.verification_commands` lets bench/run inputs carry verifier commands without prompt-only text; `WorktreeCatalyst` executes them before cleanup, failed commands mark `TestResults.failed`, and structured outcomes persist through lineage. |
| 2026-05-20 | `compound-hidden` Minimax N=3 repeat after verifier changes | Minimax scored resolved 3/3, pass@1 0/3, loop exercised 3/3, self-corrected 3/3; all runs fixed `a2_core` on attempt 1 and fixed `a2ctl` on attempt 2. |
| 2026-05-20 | `compound-hidden` Kimi N=3 repeat after verifier changes | Kimi scored resolved 3/3, pass@1 0/3, loop exercised 3/3, self-corrected 3/3. |
| 2026-05-21 | Wire self-correction fixture verifiers into TaskContract | `bench/self_correction.py` now emits JSONL `verification_commands` so loop benchmarks exercise candidate-worktree verifier scoring, not only post-apply reconciliation. |
| 2026-05-21 | Keep task verifier commands out of initial prompts | Candidate verifier commands are system-side metadata, not task hints; initial prompts now state A² may run extra verifiers but do not reveal command strings. |
| 2026-05-21 | `compound-hidden` Minimax N=3 with hidden candidate verifier | Minimax scored resolved 3/3, pass@1 0/3, loop exercised 3/3, self-corrected 3/3; candidate verifier discarded attempt 1 before apply and attempt 2 passed. |
| 2026-05-21 | `compound-hidden` Kimi N=3 with hidden candidate verifier | Kimi scored resolved 3/3, pass@1 0/3, loop exercised 3/3, self-corrected 3/3; candidate verifier discarded attempt 1 before apply and attempt 2 passed. |
| 2026-05-21 | GLM timeout at current self-correction budget | GLM at 1800s attempt timeout produced no patches in 7 observed attempts across 3 run IDs; result is a budget/timeout finding, not a model-capability conclusion. |
| 2026-05-22 | GLM provider unavailable via OpenCode due ZAI resource error | Fibonacci calibration at 200k/3600s timed out with tokens=0/no patch; direct `opencode --print-logs` smoke for `zai-coding-plan/glm-5.1` exposed upstream 429 `Insufficient balance or no resource package`. |
| 2026-05-22 | Add Pi provider route for ZAI | `a2ctl run --provider pi/zai/glm-5.1` now routes WorktreeCatalyst through Pi's built-in ZAI provider, reusing Pi auth and parsing Pi JSON usage; commits `ed41471` and `318937c`. |
| 2026-05-22 | `compound-hidden` Pi/ZAI GLM N=3 with hidden candidate verifier | Pi/ZAI GLM scored resolved 3/3, pass@1 0/3, loop exercised 3/3, self-corrected 3/3; all runs fixed `a2_core` on attempt 1 and fixed `a2ctl` on attempt 2; result `/tmp/a2-compound-hidden-pi-zai-glm.jsonl`. |
| 2026-05-22 | Add third compound self-correction fixture | `compound-archive-hidden` combines the visible `a2_core` Fibonacci regression with a hidden `a2_archive` `for_task` ordering regression; smoke-only injection verified both failures. |
| 2026-05-22 | `compound-archive-hidden` Minimax N=3 with hidden candidate verifier | Minimax scored resolved 3/3, pass@1 0/3, loop exercised 3/3, self-corrected 3/3; all runs fixed `a2_core` on attempt 1 and fixed `a2_archive` on attempt 2. |
| 2026-05-22 | `compound-archive-hidden` Kimi N=3 with hidden candidate verifier | Kimi scored resolved 3/3, pass@1 0/3, loop exercised 3/3, self-corrected 3/3; all runs fixed `a2_core` on attempt 1 and fixed `a2_archive` on attempt 2. |
| 2026-05-21 | `compound-membrane-hidden` Minimax N=3 with hidden candidate verifier | Minimax scored resolved 3/3, pass@1 0/3, loop exercised 3/3, self-corrected 3/3; all runs fixed `a2_core` on attempt 1 and fixed `a2_membrane` on attempt 2. |
| 2026-05-21 | `compound-membrane-hidden` Kimi N=3 with hidden candidate verifier | Kimi scored resolved 3/3, pass@1 0/3, loop exercised 3/3, self-corrected 3/3; all runs fixed `a2_core` on attempt 1 and fixed `a2_membrane` on attempt 2. |
| 2026-05-22 | Refresh Cargo.lock after sentinel lockfile check | During Pi/ZAI validation, sentinel initially passed 5/6 with stale `Cargo.lock`; `cargo generate-lockfile --offline` refreshed compatible cached package versions and sentinel passed 6/6; commit `433adc8`. |
| 2026-05-24 | `compound-membrane-hidden` Pi/ZAI GLM N=3 with hidden candidate verifier | Pi/ZAI GLM scored resolved 3/3, pass@1 0/3, loop exercised 3/3, self-corrected 3/3; all runs fixed `a2_core` on attempt 1 and `a2_membrane` on attempt 2; result `/tmp/a2-compound-membrane-pi-zai-glm.jsonl`. |
| 2026-05-24 | `compound-archive-hidden` Pi/ZAI GLM N=3 with hidden candidate verifier | Pi/ZAI GLM scored resolved 3/3, pass@1 0/3, loop exercised 3/3, self-corrected 3/3; all runs fixed `a2_core` on attempt 1 and `a2_archive` on attempt 2; result `/tmp/a2-compound-archive-pi-zai-glm.jsonl`. |
| 2026-05-24 | Add same-crate Sensorium self-correction fixture | `compound-sensorium-same-crate-hidden` injects visible high-risk priority and hidden title truncation regressions in `a2_sensorium/src/ingest.rs`; smoke-only injection verified both failures. |
| 2026-05-24 | `compound-sensorium-same-crate-hidden` Pi/ZAI GLM N=3 with hidden candidate verifier | Pi/ZAI GLM scored resolved 3/3, pass@1 0/3, loop exercised 3/3, self-corrected 3/3; all runs failed attempt 1 with +1/-1 in `a2_sensorium/src/ingest.rs` and passed attempt 2 with +2/-2 in the same file; result `/tmp/a2-sensorium-same-crate-pi-zai-glm.jsonl`. |
| 2026-05-24 | `compound-sensorium-same-crate-hidden` Minimax N=3 with hidden candidate verifier | Minimax scored resolved 3/3, pass@1 0/3, loop exercised 3/3, self-corrected 3/3; result `/tmp/a2-sensorium-same-crate-minimax.jsonl`. |
| 2026-05-24 | `compound-sensorium-same-crate-hidden` Kimi N=3 with hidden candidate verifier | Kimi scored resolved 3/3, pass@1 1/3, loop exercised 2/3, self-corrected 2/3; result `/tmp/a2-sensorium-same-crate-kimi.jsonl`. |
| 2026-05-24 | Add anti-repeat ablation command surface | `a2ctl run --disable-anti-repeat-retry` disables only the anti-repeat retry motif while retaining prior lineage, verifier-derived relevant files, retry acceptance criteria, and candidate-worktree verifiers; `bench/self_correction.py --disable-anti-repeat` records cohort fields and the scorer can print enabled/disabled cohorts. |
| 2026-05-25 | Add in-repo autopilot command | `a2ctl autopilot` accepts explicit tasks through `--task`/`--task-file`, otherwise discovers unchecked checklist work in `todos/` and `docs/plans/` plus code TODO/FIXME scan tasks; it logs run events to `.a2/autopilot/runs/<run-id>/events.jsonl`, supports dry-run planning, and requires explicit `--apply` for workspace mutation. |
| 2026-05-25 | First in-repo autopilot self-iteration | Ran `a2ctl autopilot --provider pi/zai/glm-5.1 --task "Persist richer autopilot run summaries..." --max-iterations 1 --apply`; workcell used 88,384 tokens over 731.9s, applied cleanly, and `verify_and_rebuild` passed. Patch added `run_summary.json` generation and richer `iteration_completed` event fields. |
| 2026-05-26 | Add autopilot stop reasons after over-budget self-iteration | A stop-condition autopilot attempt produced a patch but exceeded the 100k token budget (114,820 tokens) and was discarded. Follow-up implementation added `autopilot_stopped` events and `run_summary.json.stop_reason` for budget exhaustion, provider quota, repeated failure class, and max-iteration stops. |
| 2026-05-26 | Add verified checklist updates | Autopilot now marks checklist-sourced candidates complete only after `apply_ok && verify_ok`; source parsing is limited to `todos/...:<line>` and `docs/plans/...:<line>`, and updates are logged in `checklist_update` events plus `run_summary.json.iterations[].checklist_update`. |
| 2026-05-26 | Add aggregate autopilot run logs | Completed autopilot runs append one compact dashboard record to `.a2/autopilot/run_index.jsonl` and overwrite `.a2/autopilot/latest_run.json` with the latest run record. |
| 2026-05-27 | Add resident autopilot wrapper | `a2ctl autopilot-resident` repeatedly invokes `a2ctl autopilot`, forwards normal autopilot options, logs resident events under `.a2/autopilot/resident/<resident-id>/events.jsonl`, and stores per-run stdout/stderr files. |
| 2026-05-28 | First anti-repeat ablation cohort | `compound-hidden` with Minimax completed N=3 enabled and N=3 disabled anti-repeat cohorts. Both cohorts scored resolved/self-corrected 3/3 with pass@1 0/3. Result: `/tmp/a2-anti-repeat-ablation-compound-hidden-minimax-20260528T122327Z.jsonl`. |
| 2026-05-28 | Same-crate anti-repeat ablation cohort | `compound-sensorium-same-crate-hidden` with Minimax completed N=3 enabled and N=3 disabled anti-repeat cohorts. Both cohorts scored resolved/self-corrected 3/3 with pass@1 0/3; all runs resolved on attempt 2. Result: `/tmp/a2-anti-repeat-ablation-sensorium-minimax-20260528T221811Z.jsonl`. |
| 2026-05-29 | Add same-crate RAF self-correction fixture | `compound-raf-same-crate-hidden` injects visible single-node RAF connectivity and hidden empty-graph repair coverage regressions in `crates/a2_raf/src/graph.rs`; smoke-only injection verified both failures. Result: `/tmp/a2-raf-fixture-smoke.jsonl`. |
| 2026-05-29 | `compound-raf-same-crate-hidden` Minimax N=3 | Minimax resolved 3/3 runs; pass@1 was 1/3; loop exercised 2/3; self-corrected 2/3. One run resolved on attempt 1; two resolved on attempt 2 after prior lineage. Result: `/tmp/a2-raf-same-crate-minimax-20260529T212431Z.jsonl`. |
| 2026-05-30 | `compound-raf-same-crate-hidden` Kimi N=3 | Kimi resolved 3/3 runs; pass@1 was 3/3; loop exercised 0/3; self-corrected 0/3. Result: `/tmp/a2-raf-same-crate-kimi-20260530T071018Z.jsonl`. |
| 2026-05-30 | `compound-raf-same-crate-hidden` Pi/ZAI GLM N=3 | Pi/ZAI GLM resolved 3/3 runs; pass@1 was 3/3; loop exercised 0/3; self-corrected 0/3. One run had empty captured patch stats despite verifier success. Result: `/tmp/a2-raf-same-crate-pi-zai-glm-20260530T072430Z.jsonl`. |
| 2026-05-30 | Capture committed worktree changes | WorktreeCatalyst now records the pre-agent base commit and diffs the staged candidate worktree against that base, so agent commits are included in patch capture. Regression test added. See `docs/solutions/logic-errors/worktree-agent-commits-hidden-from-diff-20260530.md`. |
| 2026-05-30 | Post-base-diff-fix Pi/ZAI RAF N=3 | Pi/ZAI GLM resolved 3/3 runs; pass@1 2/3; loop exercised 1/3; self-corrected 1/3. Two verifier-success attempts had populated patch stats; one pass@1 attempt still had empty patch stats and no reconciliation. Result: `/tmp/a2-raf-same-crate-pi-zai-glm-post-diff-fix-20260530T073729Z.jsonl`. |
| 2026-05-30 | Align provider subprocess PWD with candidate worktree | WorktreeCatalyst now sets `PWD` to the candidate worktree for Claude, Codex, Gemini, OpenCode, and Pi subprocesses in addition to `current_dir`, reducing risk that provider tools resolve paths against the source task workspace. |
| 2026-05-30 | Post-PWD Pi/ZAI RAF N=3 | Pi/ZAI GLM resolved 3/3 runs; pass@1 2/3; loop exercised 1/3; self-corrected 1/3. All verifier-success attempts had `touched_file_count=1` and populated +2/-2 patch stats; empty verifier-success patch stats were 0/3 runs. Result: `/tmp/a2-raf-same-crate-pi-zai-glm-post-pwd-20260530T075028Z.jsonl`. |
| 2026-05-30 | Add same-crate Eval self-correction fixture | `compound-eval-same-crate-hidden` injects visible failing-test scoring and hidden token-budget scoring regressions in `crates/a2_eval/src/seed.rs`; smoke-only injection verified both failures. Result: `/tmp/a2-eval-fixture-smoke.jsonl`. |
| 2026-05-30 | `compound-eval-same-crate-hidden` Minimax N=3 | Minimax resolved 0/3 runs; pass@1 was 0/3; loop exercised 3/3; self-corrected 0/3. Each run exhausted three attempts and still failed both verifier tests. Result: `/tmp/a2-eval-same-crate-minimax-20260530T080351Z.jsonl`. |
