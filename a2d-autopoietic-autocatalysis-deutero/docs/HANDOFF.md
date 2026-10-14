# A²D Handoff Document

**Last updated:** 2026-10-14 (session 143: P0 done; P1 nearly done; calibration-v2 accepted; pilot fixed comparator D; Confirmation-1 at the cap being authored blind)
**Update this document:** before context compaction, at session end, or when significant state changes. Keep it short: current truth only. History lives in git, `docs/experiments/LOG.md` and `docs/handoff-archive/`.

## Headline (read this first)

- **Standing goal:** `docs/plans/2026-10-14-lift-first-roadmap.md`, Appendix. Measure whether A²D (arm C) beats its own coder model used directly at matched invocations: A one-shot, B best-of-k, D repair loop. Then test whether self-modification widens the margin. **Deadline:** P2 confirmation result by 2026-11-30.
- **Phase: P1, exit pending.** Exit needs Confirmation-1 and Confirmation-2 validated with hashes logged (in progress), the P1 gate review (running), and power planning (done, E05).
- **Results** (`docs/experiments/LOG.md`):
  - E01: Luna one-shot passes 2/3 legacy dev tasks.
  - E02: first live four-arm run.
  - E03: calibration v1 saturated, A 79%. That triggered T7 redesign 1/2.
  - E04: calibration v2 headroom passed, A 62.5%, B 58.3%.
  - **E05, four-arm pilot on v2:** A 58%, B 50%, D 62.5%, C 67%. C vs D +4 pp, CI [−12.5, +25], so noise. **The P2 primary comparator is D.**
  - Power plan: pd 0.21 needs about 162 independent pairs for 80% power at 10 pp, and replicates add little. **Confirmation-1 = 37 families × 4 = 148 tasks** (the cap).
- **Arm C's realised seed schedule:** coder → evolver → architect → tester → coder at k=5 (2 code attempts). **P2 uses k=5.**
- **The public tests discriminate weakly.** Best-of-5 does no better than one-shot. The rates of public-green-but-hidden-fail submissions are the evidence.
- **Coder lane:** `codex/gpt-6-luna`, effort low, run as a no-tools generator (about 20–25 s per call when the account isn't contended). **Don't run holdout authoring (Codex Sol, 6 parallel jobs) at the same time as live runs**: Luna calls slowed to about 65 s (E05 deviation).

## Holdouts (never open their contents)

- `$A2D_HOLDOUT_DIR` = `~/.a2d-holdout`, outside the repo.
  - `authoring/<set>/<family>/` holds Codex's working dirs. **Never read `codex.out`, `codex.err`, `REPORT.md` or any files in them.**
  - `confirmation-1/` and `confirmation-2/` hold the assembled sets.
- **Assemble blind:** `python3 -I $SCRATCH/assemble-holdout.py <set>` copies only the contract files and prints task ids. **Validate blind:** `a2d lift validate ~/.a2d-holdout/<set>` prints verdicts only.
- Family lists: Confirmation-1 has 12 base families plus 25 extension families; Confirmation-2 has 12 (extend it before P4). Each list is in the LOG and roadmap context, as names only.
- State: the first 12 Confirmation-1 families (48 tasks) are validated, 48/48 (hash `c4ac2cde…`, superseded once the extension lands). The round-2 driver (`$SCRATCH/author-holdouts-2.sh`, log `author-holdouts-2.log`) authors the extension and then Confirmation-2. When it finishes, re-assemble, re-validate, and record the final hashes in the LOG.

## Next moves (updated 2026-10-14 late; P1 gate triage done)

The triage is complete and recorded in the roadmap Addendum "P1 gate":
- tie-break is latest;
- classify over complete tuples up to 5% excluded;
- claim = primary C-vs-D **and** secondary C-vs-B SUPPORTED, while RULED OUT is decided by C vs D alone;
- API-surface validation;
- manifest, `score --expect-head`, freeze through scoring;
- power targets restated.

**E06 is done.** C/D replicate agreement was 0.875, so **Confirmation-1 runs r = 1**. Across the pilots there is no sign of lift: under the latest-tie rule C passed 17 and 14, D 15 and 16.

0. **State (2026-10-14, late):**
   - **Confirmation-1 is final: 148 tasks, hash `7bd1b669…`** (LOG "Holdouts — Confirmation-1 final"). The independent-solve audit passed 148/148, so no adjudication was needed.
   - **Confirmation-2 authoring is running** (log `$SCRATCH/author-holdouts-c2.log`). When it finishes:
     - assemble and validate it;
     - repair any rejects blind with `$SCRATCH/FIX.md`, `api_check.sh` and `FIX-VERDICTS.txt` in the family dir;
     - audit it like Confirmation-1;
     - log its hash.
   - Then: P1 exit Addendum, then E07, whose draft is in `$SCRATCH/E07-prereg-draft.md`. Fill in the C1 hash and the registration commit.
1. **Holdout authoring is running** (driver `author-holdouts-2.sh`, log `$SCRATCH/author-holdouts-2.log`): the rest of the Confirmation-1 extension, then Confirmation-2. When it finishes:
   - `python3 -I $SCRATCH/assemble-holdout.py confirmation-1`;
   - `a2d lift validate ~/.a2d-holdout/confirmation-1 --jobs 4` (verdict lines only).
2. **Independent-solve audit of Confirmation-1, which blocks P1 exit.** The account must have nothing else running.
   - Run:
     ```
     a2d lift run --experiment AUDIT-C1 --unregistered --suite ~/.a2d-holdout/confirmation-1 \
       --model codex/gpt-6.1-sol --effort medium --arms D --k 3 --jobs 4 --record-hidden-output \
       --max-invocations 500 --max-wall-clock-secs 21600 --out ~/.a2d-holdout/audit/c1-run1
     ```
   - Then `python3 -I $SCRATCH/stage-audit.py confirmation-1 ~/.a2d-holdout/audit/c1-run1 $SCRATCH/ADJUDICATE.md`. It prints counts and family names.
   - Then one Codex Sol job per flagged family: `codex exec "Read ADJUDICATE.md ..."` in `~/.a2d-holdout/authoring/confirmation-1/<family>`. Read only `grep '^ADJUDICATED' codex.out`.
   - Then re-assemble, re-validate, and record the final hash in the LOG.
3. **P1 exit:** a dated roadmap Addendum (Confirmation-1 hash, Confirmation-2 status, audit result) and Status → P2.
4. **E07, the P2 confirmation.**
   - Pre-register: Confirmation-1 (148 tasks), A, B, D, C, k=5, r=1, `--primary D`, seed fixed, `--defer-scoring`, `--out ~/.a2d-holdout/runs/E07`. The global cap is the planned maximum (148 × 16 = 2368); the wall clock is 2× the expected time (about 6 h). Authoring must be paused.
   - After generation: `a2d lift manifest ~/.a2d-holdout/runs/E07 > runs/<date>-e07/manifest.json`, then commit and push.
   - Then `a2d lift score --out ~/.a2d-holdout/runs/E07 --suite ~/.a2d-holdout/confirmation-1 --expect-head <ledger_head>`.
   - Copy `summary.json` and the ledger, which is content-free (checked), into `runs/`.
5. P3 (plan: `docs/plans/2026-10-14-p3-ablation.md`, to draft) on development data. calibration-v2 can serve as development data now.

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
