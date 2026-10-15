# A²D Handoff Document

**Last updated:** 2026-10-14 (session 143: P0 done; P1 nearly done; calibration-v2 accepted; pilot fixed comparator D; Confirmation-1 at the cap being authored blind)
**Update this document:** before context compaction, at session end, or when significant state changes. Keep it short: current truth only. History lives in git, `docs/experiments/LOG.md` and `docs/handoff-archive/`.

## Headline (read this first)

- **Standing goal:** `docs/plans/2026-10-14-lift-first-roadmap.md`, Appendix. **Phase: P2**, with the extension running. P0 and P1 are complete. Deadline 2026-11-30, and the P2 result already exists.
- **E07 (the P2 confirmation, Confirmation-1, 148 tasks in 37 families, scored once): INCONCLUSIVE.**
  - Primary C vs D: **+2.7 pp, 95% CI [−4.7, +11.5]**. Secondary C vs B: +4.1 pp, CI [−4.7, +12.8].
  - Pass rates: A 58%, B 65%, D 66%, C 69%.
  - **Cost: C spends 3.8× D's invocations and 3.7× its tokens for +2.7 pp.**
  - The run was clean: no voids, errors, tool calls or hidden-name hits.
- **E07b is the single pre-registered extension:** a second replicate pass, seed 8, frozen at `c4ec034a`.
  - It is pooled with E07 by `a2d lift pool`, and **the pooled class is final**.
  - If the pooled result is INCONCLUSIVE, that is NO-GO for adding complexity. If it is RULED OUT, the seed configuration is falsified, which completes once P4 also fails to show lift.
  - **No edits to `lift_arm_c.rs`, metabolism, `main.rs` or providers until E07b is scored**, because pooling needs a byte-identical treatment.
- **Coder lane:** `codex/gpt-6-luna`, effort low, no-tools. The sibling A² session shares the Codex account; this is recorded as a deviation.
- Pilots, for context: E05 and E06 put C ≈ D (C 17 and 14, D 15 and 16 under the latest-tie rule). Public tests discriminate weakly: the public-green but hidden-fail rate is 25–33% for B and D.

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

0. **State (2026-10-15):**
   - E07 is scored and recorded.
   - **E07b is generating** (out `~/.a2d-holdout/runs/E07b`, stdout `~/.a2d-holdout/runs/E07b.stdout`).
   - When its ledger has `generation_complete`:
     1. `a2d lift manifest ~/.a2d-holdout/runs/E07b > runs/2026-10-15-e07b-confirmation-1-r2/manifest.json`; commit and push.
     2. `a2d lift score --out ~/.a2d-holdout/runs/E07b --suite ~/.a2d-holdout/confirmation-1 --expect-head <head> --jobs 4`.
     3. Copy the summary and ledger into `runs/`.
     4. `a2d lift pool --out runs/2026-10-15-e07b-confirmation-1-r2/pooled-summary.json runs/2026-10-15-e07-confirmation-1/ledger.jsonl runs/2026-10-15-e07b-confirmation-1-r2/ledger.jsonl`.
     5. Record the pooled class in the LOG (final) and write the P2-gate Addendum, with reviews and diagnostics.
     6. Then P3 (`docs/plans/2026-10-14-p3-ablation.md`).
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
