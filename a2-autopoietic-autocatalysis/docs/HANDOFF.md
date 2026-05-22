# A² Handoff — Read This First

**Last updated:** 2026-05-22
**Update this file:** before context compaction, at session end, or when significant state changes.

## What Is This

A² (Autopoietic Autocatalysis) is an autonomous software factory that modifies its own source code. It uses AI model CLIs (Claude, Codex, Gemini, OpenCode) as "food set" models that edit code in git worktrees, then the system verifies, scores, and optionally applies the patches to its own germline.

## Current Numbers (as of 2026-05-10)

| Metric | Value |
|--------|-------|
| Tests | 81 Rust + 4 self-correction Python tests |
| Sentinels | 6/6 PASS |
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
cargo test                                    # expect 74 pass
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
| pi/zai | zai/glm-5.1 | Available | Added 2026-05-22. Uses Pi's built-in ZAI provider and existing `~/.pi/agent/auth.json` `zai` API key. Fibonacci calibration passed attempt 1 with token accounting (`/tmp/a2-pi-zai-fibonacci-json-usage.jsonl`); `compound-hidden` N=3 resolved/self-corrected 3/3 via Pi/ZAI before JSON usage parsing (`/tmp/a2-compound-hidden-pi-zai-glm.jsonl`). |
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

ContextPack is wired and self-correction harnesses exist. The current gap is no longer "build a loop benchmark"; it is "make the loop recover." Minimax and Kimi now have N=3 self-correction success on both current compound fixtures after hidden candidate-worktree verifier wiring. Remaining validation is provider availability for GLM and broader loop-shaped fixtures.

### Current loop status (2026-05-22)

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
- The 2026-05-20 Minimax/Kimi reruns happened after candidate verifier code existed but before the self-correction harness passed verifier commands, so those reruns exercised post-apply verification/retry context rather than candidate-worktree verifier scoring.
- `compound-hidden` with Kimi on 2026-05-20 after anti-repeat + task-verifier code changes resolved 3/3 runs; pass@1 was 0/3; loop exercised 3/3; self-corrected 3/3. Results: `/tmp/a2-compound-after-task-verifier-kimi.jsonl`.
- `compound-hidden` with Minimax on 2026-05-20 after anti-repeat + task-verifier code changes resolved 3/3 runs; pass@1 was 0/3; loop exercised 3/3; self-corrected 3/3. In all three runs attempt 1 touched only `a2_core/src/lib.rs`; attempt 2 touched both `a2_core/src/lib.rs` and `a2ctl/src/main.rs` and verified clean. Results: `/tmp/a2-compound-after-task-verifier-minimax.jsonl`.
- `compound-hidden` with Minimax on 2026-05-16 resolved 3/3 runs; pass@1 was 0/3; loop exercised 3/3; self-corrected 3/3. In all three runs attempt 1 touched only `a2_core/src/lib.rs`; attempt 2 touched both `a2_core/src/lib.rs` and `a2ctl/src/main.rs` and verified clean.
- `compound-hidden` with Kimi on 2026-05-18 resolved 3/3 runs; pass@1 was 0/3; loop exercised 3/3; self-corrected 3/3. In all three runs attempt 1 touched only `a2_core/src/lib.rs`; attempt 2 touched both `a2_core/src/lib.rs` and `a2ctl/src/main.rs` and verified clean.

**Not yet validated:**
- Pi/ZAI GLM recovery beyond `compound-hidden` after hidden candidate-worktree verifier wiring. 2026-05-22 Pi/ZAI `compound-hidden` N=3 resolved/self-corrected 3/3; `compound-membrane-hidden` and `compound-archive-hidden` are not yet run with Pi/ZAI.
- Loop recovery beyond Minimax/Kimi and the three current compound fixtures after candidate-worktree task verifier execution.
- Cross-provider/fixture benchmark impact of anti-repeat retry strategy beyond the current self-correction fixtures.

**Structural solution direction:** Minimax/Kimi now have N=3 loop recovery on all three current compound fixtures with hidden candidate-worktree verifier wiring. Pi/ZAI GLM has N=3 loop recovery on `compound-hidden`; remaining validation is Pi/ZAI on the other compound fixtures and adding broader loop-shaped fixtures. Dedicated todos live in `todos/`.

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
- [x] **Run task-specific verifier in candidate worktrees before promotion scoring.** Completed 2026-05-20. `TaskContract.verification_commands` carries shell verifier commands; `WorktreeCatalyst` runs them in the candidate worktree, maps outcomes into `TestResults` plus structured `ExternalVerification`, and `run_workcell` persists those verifier records into lineage before promotion. `a2ctl bench` wires TOML `[verify]` commands, and JSONL run input accepts optional `verification_commands`. See `todos/worktree-task-verifier.md`.
- [ ] **Run `compound-hidden` N≥3 per available non-Claude provider after each structural change.** Current factual result after hidden candidate-worktree verifier wiring: Minimax N=3 and Kimi N=3 on `compound-hidden` on 2026-05-21 both scored resolved 3/3, pass@1 0/3, loop exercised 3/3, self-corrected 3/3. Results: `/tmp/a2-compound-with-hidden-worktree-verifier-minimax.jsonl` and `/tmp/a2-compound-with-hidden-worktree-verifier-kimi.jsonl`. Minimax N=3 and Kimi N=3 on `compound-membrane-hidden` on 2026-05-21 both also scored resolved 3/3, pass@1 0/3, loop exercised 3/3, self-corrected 3/3. Results: `/tmp/a2-compound-membrane-with-hidden-worktree-verifier-minimax.jsonl` and `/tmp/a2-compound-membrane-with-hidden-worktree-verifier-kimi.jsonl`. Minimax/Kimi N=3 on `compound-archive-hidden` on 2026-05-22 also scored resolved/self-corrected 3/3. Pi/ZAI GLM N=3 on `compound-hidden` on 2026-05-22 scored resolved 3/3, pass@1 0/3, loop exercised 3/3, self-corrected 3/3; results: `/tmp/a2-compound-hidden-pi-zai-glm.jsonl`. Prior OpenCode GLM route timed out while ZAI balance was unavailable; prefer `pi/zai/glm-5.1`. Prior structured retry-context results: Minimax N=3 on 2026-05-16 and Kimi N=3 on 2026-05-18 both scored resolved 3/3, pass@1 0/3, loop exercised 3/3, self-corrected 3/3.
- [x] **Add a second compound fixture after one self-correction success.** Completed 2026-05-18. `bench/self_correction.py` now includes `compound-membrane-hidden`, which combines the visible `a2_core` Fibonacci regression with a hidden `a2_membrane` deny-overrides-allow regression. Smoke-only injection verified both failures. After hidden candidate-worktree verifier wiring, Minimax N=3 and Kimi N=3 on 2026-05-21 both scored resolved 3/3, pass@1 0/3, loop exercised 3/3, self-corrected 3/3.

### Loop-shaped benchmarks

1. **Self-correction benchmark** *(implemented 2026-04-28 as `bench/self_correction.py` + `bench/self_correction_score.py`)*: isolated git worktree, pinned task ID, core run-path lineage reconciliation, JSONL results. Fixtures: `fibonacci` (too easy: Minimax N=3 pass@1 3/3, loop 0/3; Pi/ZAI GLM passed attempt 1 on 2026-05-22), `compound-hidden` (harder: Minimax/Kimi runs before structured retry context failed attempts 1-3; after structured verifier records + retry acceptance criteria + verifier-derived relevant files, Minimax N=3 on 2026-05-16 and Kimi N=3 on 2026-05-18 resolved on attempt 2 in all runs; after hidden candidate-worktree verifier wiring, Minimax N=3 and Kimi N=3 on 2026-05-21 resolved on attempt 2 in all runs; Pi/ZAI GLM N=3 on 2026-05-22 resolved on attempt 2 in all runs), `compound-membrane-hidden` (added 2026-05-18; visible `a2_core` Fibonacci regression plus hidden `a2_membrane` deny-overrides-allow regression; Minimax N=3 on 2026-05-18 resolved on attempt 2 in all runs; after hidden candidate-worktree verifier wiring, Minimax N=3 and Kimi N=3 on 2026-05-21 resolved on attempt 2 in all runs), `compound-archive-hidden` (added 2026-05-22; visible `a2_core` Fibonacci regression plus hidden `a2_archive` lineage ordering regression; smoke-only injection verified both failures; after hidden candidate-worktree verifier wiring, Minimax N=3 and Kimi N=3 on 2026-05-22 resolved on attempt 2 in all runs).
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
| 2026-05-22 | Add Pi provider route for ZAI | `a2ctl run --provider pi/zai/glm-5.1` now routes WorktreeCatalyst through Pi's built-in ZAI provider, reusing Pi auth and parsing Pi JSON usage. |
| 2026-05-22 | `compound-hidden` Pi/ZAI GLM N=3 with hidden candidate verifier | Pi/ZAI GLM scored resolved 3/3, pass@1 0/3, loop exercised 3/3, self-corrected 3/3; all runs fixed `a2_core` on attempt 1 and fixed `a2ctl` on attempt 2. |
| 2026-05-22 | Add third compound self-correction fixture | `compound-archive-hidden` combines the visible `a2_core` Fibonacci regression with a hidden `a2_archive` `for_task` ordering regression; smoke-only injection verified both failures. |
| 2026-05-22 | `compound-archive-hidden` Minimax N=3 with hidden candidate verifier | Minimax scored resolved 3/3, pass@1 0/3, loop exercised 3/3, self-corrected 3/3; all runs fixed `a2_core` on attempt 1 and fixed `a2_archive` on attempt 2. |
| 2026-05-22 | `compound-archive-hidden` Kimi N=3 with hidden candidate verifier | Kimi scored resolved 3/3, pass@1 0/3, loop exercised 3/3, self-corrected 3/3; all runs fixed `a2_core` on attempt 1 and fixed `a2_archive` on attempt 2. |
| 2026-05-21 | `compound-membrane-hidden` Minimax N=3 with hidden candidate verifier | Minimax scored resolved 3/3, pass@1 0/3, loop exercised 3/3, self-corrected 3/3; all runs fixed `a2_core` on attempt 1 and fixed `a2_membrane` on attempt 2. |
| 2026-05-21 | `compound-membrane-hidden` Kimi N=3 with hidden candidate verifier | Kimi scored resolved 3/3, pass@1 0/3, loop exercised 3/3, self-corrected 3/3; all runs fixed `a2_core` on attempt 1 and fixed `a2_membrane` on attempt 2. |
