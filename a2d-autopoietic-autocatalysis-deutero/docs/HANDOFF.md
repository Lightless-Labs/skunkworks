# A²D Handoff Document

**Last updated:** 2026-10-14 (session 143: P0 done; P1 nearly done; calibration-v2 accepted; pilot fixed comparator D; Confirmation-1 at the cap being authored blind)
**Update this document:** before context compaction, at session end, or when significant state changes. Keep it short: current truth only. History lives in git, `docs/experiments/LOG.md` and `docs/handoff-archive/`.

## Headline (read this first)

- **Owner's framing (2026-10-15):** A²D is *autoresearch on agentic coding harnesses*. A coding agent iterates and experiments on a self-improving harness and process, with human insights. You have the reins. The current rules live in **`docs/PROTOCOL.md`**.
- **P2 is DONE: the seed lift is RULED OUT.**
  - Pooled E07 + E07b, 296 tuples, 37 families: C vs D +1.4 pp, CI [−5.4, +8.8]. The two-look adjusted upper bound is +9.8. C spends 3.7× D's tokens.
  - Robustness checks: domain clustering, leave-one-family-out, and only 2/296 self-test artifacts (all in the LOG).
  - Narrow claim: the frozen seed harness at an equal invocation cap, with Luna-low.
- **P3 is the outer research loop** (`docs/plans/2026-10-15-p3-research-loop.md`). Development data:
  - visible: `tasks/calibration-v2`, `tasks/dev-v2` (48 tasks), `tasks/c1-train` (68; Confirmation-1 re-designated, split in `docs/experiments/c1-split.json`);
  - metric-only: `~/.a2d-holdout/c1-validation` (80 tasks), never opened.
- **Step 0 findings (LOG S0):**
  - **The seed coder never sees its previous code.** In 15/24 tasks its second call got no feedback at all.
  - The architect and tester are inert, proven by unit test.
  - Selection headroom: within B's 5 candidates, any-pass is 86% against D's pick at 74% (train).
  - The constraint: 92% of D's failures are public-green, so only signals beyond the public tests can help.
- **S1 (blind spec tests, offline falsification) is running.** It is pre-registered with kill thresholds: validity ≥ 0.85, AUC ≥ 0.65, recall ≥ 0.45.
- **Confirmation-2 is being extended blind** (25 families; log `$SCRATCH/author-c2ext.log`). It is the only claim set left.
- Coder lane: `codex/gpt-6-luna`, low effort, no tools. The A² session shares the account; this is a standing condition.

## Next moves
1. Read S1's result (`runs/2026-10-15-s1-spec-tests/evaluation.json` and the stdout summary) and record it in the LOG.
   - If it survives: build the harness interpreter (`code` with the previous file, `spec_tests`, `review`, `stop`, `select`, plus a reachability check) and run the first live train experiment (development lane, one-row pre-registration).
   - If it dies: make clause review-and-fix primary.
2. Finish Confirmation-2: assemble with `$SCRATCH/assemble-holdout.py confirmation-2`, validate, run the independent-solve audit, and record the hash.
3. Validation looks are capped at 4. The promotion gate is +12 pp over a concurrent D. Budget: P3 ≤ 3k invocations.

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

`cargo test` is all green. Five `a2d-cli` fitness-evidence provenance tests fail whenever there are **untracked files under `crates/`**, so commit before running the full suite. The `a2d-lift` tests take about 20 s, because the oracle compiles real code.

## Legacy (unchanged, deprioritised)

- The metabolism, evolver, architect, escalation rungs and observer are implemented, but their value is unproven. The observer never fires: CLI providers emit a constant trace, and the lift lane keeps it constant until P5 validates the observer offline.
- Live `challenge` and related commands now use `Challenge::selection_benchmark()`, with no hidden acceptance; hidden tests are scored only via `score-artifact` or `a2d lift`.
- The SWE-Bench Pro chain is parked (`docs/plans/swe-bench-pro-blind-evaluator-adapter.md`).
- Pre-reset history: `docs/handoff-archive/2026-07-10-session-141-HANDOFF.md`. Audit and prior art: `docs/research/2026-10-14-direction-audit.md` and `docs/research/2026-10-14-dgm-rqgm-relevance.md`.
