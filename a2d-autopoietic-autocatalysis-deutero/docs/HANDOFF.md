# A²D Handoff Document

**Last updated:** 2026-10-14 (session 143: P0 complete, gate reviewed, E01–E02 recorded, P1 suite authoring started)
**Update this document:** before context compaction, at session end, or when significant state changes. Keep it short: current truth only. History lives in git, `docs/experiments/LOG.md` and `docs/handoff-archive/`.

## Headline (read this first)

- **Standing goal:** `docs/plans/2026-10-14-lift-first-roadmap.md`, Appendix. Measure whether A²D (arm C) beats its own coder model used directly at matched invocations: A one-shot, B best-of-k, D repair loop. Then test whether self-modification widens the margin. The plan completes only when it is marked Completed with confirmation results in `docs/experiments/LOG.md`. **Deadline:** P2 seed result by 2026-11-30.
- **Phase: P1** (calibrate a non-saturated suite). Plan: `docs/plans/2026-10-14-p1-calibration-suite.md`. P0 exited 2026-10-14; see the roadmap's P0-gate Addendum.
- **Results so far (development only, no findings):**
  - E01: Luna one-shot passes 2/3 legacy tasks.
  - E02: all four arms pass 2/3; integrity checks clean; arm C's realised schedule is coder → evolver → architect → tester → coder at k=5.
  - **P2 must use k ≡ 1 (mod 4).** At k=4, C gets one code attempt.
- **Coder lane:** `codex/gpt-6-luna`, effort low, run as a **no-tools generator** (`CODEX_DISABLED_FEATURES` in `crates/a2d-providers/src/cli.rs`). About 25–45 s and 11k tokens per call. Reviews use Codex `gpt-6.1-sol` plus a fresh-context Claude subagent, in empty temp dirs.

## The controller: `a2d lift` (`crates/a2d-lift/`)

```bash
cargo build -p a2d
./target/debug/a2d lift hash tasks/dev
./target/debug/a2d lift validate <suite> --jobs 4          # reference + mutants oracle check (P1 suites)
./target/debug/a2d lift run --experiment E0N --suite <dir> --expect-suite-hash <sha> --registered-at <sha> \
  --model codex/gpt-6-luna --effort low --arms A,B,D,C --k 5 --primary B --seed 1 --jobs 3 \
  --max-invocations <n> --max-wall-clock-secs <s> --out runs/<date>-<id> [--record-hidden-output]
./target/debug/a2d lift verify-ledger runs/<...>/ledger.jsonl
```

- **Tasks:** `<suite>/<task>/task.json`, `public_tests.rs` and `hidden_tests.rs`, plus `reference.rs` and `mutants/*.rs` for validation. Arms see only a `PublicTask`.
- **Two phases.** Every arm generates and submits; then each submission is scored once by the hidden oracle. The oracle compiles the candidate as the crate root and the tests as an earlier-declared `#[path]` module with explicit std macro imports. It deletes the sources before running, and requires a clean exit with every expected test passing. See `docs/solutions/best-practices/oracle-isolation-for-llm-candidates-2026-10-14.md`.
- **Budget meter.** Per-arm budget (A=1, B/C/D=k), a global cap, a wall clock, and tokens, provider time and coder calls per tuple. A wall-clock refusal voids the tuple. Prompts, responses and Codex transcripts go under `runs/<id>/invocations/`.
- **Summary.**
  - Run status: complete/aborted, void tuples, and hidden-name scan hits.
  - Per arm: rate, tokens, public-green-but-hidden-fail and suspicious constructs.
  - Comparisons: paired cluster-bootstrap CI, family sign-flip p, descriptive McNemar, and a class guarded by `--min-families` (default 10) and by zero discordance.
  - `--primary` marks the single primary comparison.
- **Registration freeze:** `crates/`, `Cargo.toml` and `Cargo.lock` must be unchanged since `--registered-at`. The suite hash must match. `--unregistered` is for development only.
- **Arm C** (`crates/a2d-cli/src/lift_arm_c.rs`):
  - seed germline per task;
  - one metered provider for every enzyme;
  - public-tests-only benchmark;
  - no project root, so patches are rejected;
  - cooldown off;
  - returns all code artifacts, and the controller picks one with the same public rule as B and D (most public passes, then compiled, then earliest).
- **Protected from automated patches:** `crates/a2d-lift/`, `lift_arm_c.rs`, `LOG.md`, `runs/` (`self_sandbox::PROTECTED_DIRS`; CONSTITUTION Invariant 4).

## In flight / next moves

1. **P1 calibration suite.** Six families × 4 graded tasks (`stream-b64`, `dep-resolve`, `txn-kv`, `rate-sched`, `semver`, `lru-repair`) are being authored by Codex Sol in `$SCRATCH/p1-author/<family>/out/`, per `SPEC.md` there. Then:
   - copy them to `tasks/calibration/`;
   - run `a2d lift validate`, and feed rejections back to Codex;
   - hand-check that the public tests cover every major API item.
2. **E03, the calibration pilot:** A + B at k = 4 (or 5) on `tasks/calibration`. Headroom gate: A in 20–70%, B ≤ 85%, per family and pooled. If saturated, it's a T7 redesign (max 2): harder levels, `gpt-5.6-luna`, or lower effort.
3. **Power planning** from pilot discordance (in families), then author Confirmation-1 and Confirmation-2 into `$A2D_HOLDOUT_DIR` (outside the repo, Codex-authored, validated blind), and commit only the hashes.
4. **Before P2 confirmation:** add deferred scoring (generate, commit submission hashes, then score), and pre-register k ≡ 1 (mod 4) with `--primary` fixed from the pilot.

Stop doing: SWE-bench/redaction/env-scrub scaffolding (T3), constant-artifact evidence, controller work not needed by the next run.

## Provider state (2026-10-14)

- Working lanes: Codex (`gpt-6-luna` coder, `gpt-6.1-sol` reviewer and author) and Claude subagents. The Kimi, Z.ai, MiniMax and Gemini subscriptions were stopped on purpose, so don't plan around them. `gpt-6.1-luna` is not offered on this account.
- Codex gotchas:
  - `-s read-only` never blocks reads;
  - user config auto-approves escalations, so run reviewers and authors in empty temp dirs, never with the repo as cwd;
  - close stdin (`< /dev/null`);
  - inside Codex's sandbox the rustup proxy hangs, so give it the toolchain's `rustc` path directly.

## Tests

`cargo test` is all green (about 520 tests). Five `a2d-cli` fitness-evidence provenance tests fail whenever there are **untracked files under `crates/`**, so commit before running the full suite. The `a2d-lift` tests take about 20 s, because the oracle compiles real code.

## Legacy (unchanged, deprioritised)

- The metabolism, evolver, architect, escalation rungs and observer are implemented, but their value is unproven. The observer never fires: CLI providers emit a constant trace, and the lift lane keeps it constant until P5 validates the observer offline.
- Live `challenge` and related commands now use `Challenge::selection_benchmark()`, with no hidden acceptance; hidden tests are scored only via `score-artifact` or `a2d lift`.
- The SWE-Bench Pro chain is parked (`docs/plans/swe-bench-pro-blind-evaluator-adapter.md`).
- Pre-reset history: `docs/handoff-archive/2026-07-10-session-141-HANDOFF.md`. Audit and prior art: `docs/research/2026-10-14-direction-audit.md` and `docs/research/2026-10-14-dgm-rqgm-relevance.md`.
