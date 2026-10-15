# A²D Handoff Document

**Last updated:** 2026-10-15 (session 144: P2 RULED OUT; P3 mechanisms S1–S3 killed; H0 built, fixed and live-checked (E08r); T0 taxonomy; S4 oracle-lessons ceiling running; Confirmation-2 finalising)
**Update this document:** before context compaction, at session end, or when significant state changes. Keep it short: current truth only. History lives in git, `docs/experiments/LOG.md` and `docs/handoff-archive/`.

## Headline (read this first)

- **Owner's framing:** A²D is *autoresearch on agentic coding harnesses*. A coding agent iterates and experiments on a self-improving harness and process, with human insights. You have the reins. The rules live in **`docs/PROTOCOL.md`**. Goal and completion: `docs/plans/2026-10-14-lift-first-roadmap.md` (deadline 2026-11-30).
- **P2 is DONE: the seed lift is RULED OUT.** Pooled E07 + E07b (296 tuples, 37 families): C vs D +1.4 pp, CI [−5.4, +8.8], at 3.7× D's tokens.
- **P3 (outer loop, `docs/plans/2026-10-15-p3-research-loop.md`).**
  - Mechanisms S1, S1b, S2 and S3 were all killed offline by pre-registered rules:
    - blind spec tests (AUC 0.63);
    - consensus-filtered spec tests (AUC 0.64);
    - clause review and fix (conversion rate 0.19);
    - agreement selection (0.73 vs D 0.74).
  - Compound doc: `docs/solutions/architectural-insights/self-signals-dont-beat-repair-loop-2026-10-15.md`.
  - **H0** (`harnesses/h0.json`) = the seed minus the architect and tester, with the coder shown its previous attempt and a cleaned prompt.
    - The first run, E08, was **aborted**: the cleaned prompt dropped the seed's `main()` clause, and the metabolism sandbox compiles the file as a program, which caused spurious E0601 errors. Fixed and guarded in `validate_harness`.
    - **E08r (train): C(H0 v2) 77.9% vs D 75.0%** (+2.9 pp, CI [−11.8, +19.1]) at 4.25× invocations. The sanity check is met.
  - **H0 for P4 = H0 v2 + `stop_at_public_green`** (new harness key).
  - **T0 failure taxonomy** (32 public-green but hidden-fail D picks on train, Sol): the top 3 categories cover 62%, and the lessons recur. The P4 lever is therefore a **`lessons` slot**.
  - **S4 (running):** oracle-lessons ceiling on c1-validation, validation look 1 of 4. It is H0 v3 plus 8 researcher-distilled lessons, with D alongside.
    - Rule: ≥ +5 pp means P4 builds lessons that A²D writes itself; below that, the lever is deprioritised.
    - It is a ceiling, **not promotable**. Whether a researcher-lessons arm may enter Confirmation-2 is a question for the gate.
- **Confirmation-2** (the only claim set): 36 families × 4 = **144 tasks**.
  - `spreadsheet-eval` was excluded after an accidental exposure (see the LOG deviation). The rule now: never print process command lines while holdout jobs run.
  - The extension audit (Sol) passed 97/100. The 3 `calendar-recurrence` failures were adjudicated (3 underdetermined tests, 3 tasks changed), and the re-audit passed 3/3.
  - **Full re-validation is running** (`$SCRATCH/c2-final-validate.txt`). Next, record the final hash and family list in the LOG.
- Coder lane: `codex/gpt-6-luna`, low effort, no tools. The A² session shares the account; this is a standing condition.

## Next moves
1. **S4 result:** read `~/.a2d-holdout/runs/S4/summary.json` (aggregates only), copy it to `runs/2026-10-15-s4-oracle-lessons/`, apply the rule, and record it in the LOG.
2. **Confirmation-2:** once validation passes, `a2d lift hash ~/.a2d-holdout/confirmation-2`, then record the hash, 144 tasks and the family list in the LOG (Holdouts — Confirmation-2).
3. **P3 exit addendum** (roadmap and P3 plan):
   - P3 is "not promoted". Record the validation looks used and freeze H0 v3 as P4's baseline.
   - T0 justifies the lessons *slot*. Its contents in P4 must be written by A²D (Luna, as architect) from train pass/fail labels, never from test content.
   - Then the P3 → P4 gate review: Codex Sol plus a fresh Claude subagent, both in empty temp dirs.
4. **P4 plan revision** (`docs/plans/2026-10-15-p4-autopoietic-lift.md` is stale: it still says Confirmation-2 has 48 tasks, and its budget exceeds 5k):
   - `a2d lift distill`: the architect reads failure dossiers (spec, candidate and hidden pass/fail label only) and appends ≤ 8 task-agnostic lessons to the harness. Mechanical test: a hidden-test sentinel never reaches the architect prompt.
   - Cross-fit within train (lessons from one half, acceptance on the other).
   - ≥ 2 searches, plus an uninformed-lessons control (generic advice) at equal budget.
   - Validation look for H*.
   - Final registered Confirmation-2 run: D, B, H0, H*.
   - Size everything within P4 ≤ 5k invocations.

## The controller: `a2d lift` (`crates/a2d-lift/`)

```bash
cargo build -p a2d
./target/debug/a2d lift validate <suite> --jobs 4
./target/debug/a2d lift run --experiment <id> [--unregistered | --registered-at <sha> --expect-suite-hash <sha>] \
  --suite <dir> --model codex/gpt-6-luna --effort low --arms D,C --k 5 --primary D --seed <n> --jobs 4 \
  --harness harnesses/<h>.json --max-invocations <n> --max-wall-clock-secs <s> --out <dir> [--defer-scoring]
./target/debug/a2d lift manifest|score|pool|compare|verify-ledger ...
```

- **Arms:** A (one-shot), B (best-of-k, public selection), D (repair loop, stops at public-green), C (A²D metabolism, `crates/a2d-cli/src/lift_arm_c.rs`). All run at an equal invocation cap k.
- **Oracle:** the candidate is the crate root, and the trusted tests are an earlier-declared `#[path]` module. Sources are deleted before the run, and a clean exit is required.
- **Harness-as-data for C** (`--harness`, validated and hashed). Keys:
  - `name`;
  - `drop_enzymes` (RAF closure enforced);
  - `coder_note` (appended to the task input);
  - `coder_prompt` (it must keep the `main()` clause);
  - `route_previous_code`;
  - `stop_at_public_green`.

  Unknown keys are rejected.
- **Summary:** per-arm rate, tokens and public-green-hidden-fail. Comparisons use a paired cluster-bootstrap CI, the family sign-flip p, and a class guarded by `--min-families`.
- **Protected from automated patches:** `crates/a2d-lift/`, `lift_arm_c.rs`, `LOG.md`, `runs/`.

## Data
- Visible development data: `tasks/calibration-v2` (24), `tasks/dev-v2` (48) and `tasks/c1-train` (68; split in `docs/experiments/c1-split.json`).
- Metric-only: `~/.a2d-holdout/c1-validation` (80 tasks, 20 families). Read `summary.json` aggregates only, and never open its prompts or tasks.
- Claim set: `~/.a2d-holdout/confirmation-2` (144). Never opened; scored once.
- Development artefacts: `runs/2026-10-15-s0-candidate-pool/` (1,263 hidden-scored train candidates), `runs/2026-10-15-t0-taxonomy/`, and analysis scripts in `docs/experiments/analysis/`.

## Budget (PROTOCOL envelope, about 15k at the P2 gate)
- P3 ≤ 3k. Used so far: about 200 offline, E08 159, E08r 420, T0 32 Sol, plus S4 (≤ 500).
- P4 ≤ 5k. Confirmation-2 run ≈ 5k. Reserve ≈ 2k.

## Provider state
- Working lanes: Codex (`gpt-6-luna` coder, `gpt-6.1-sol` reviewer and author) and Claude subagents. The Kimi, Z.ai, MiniMax and Gemini subscriptions were stopped on purpose.
- Codex gotchas:
  - `-s read-only` never blocks reads, so run reviewers and authors in empty temp dirs;
  - close stdin;
  - inside the sandbox, give it the toolchain's `rustc` path directly;
  - **`codex exec` argv carries the prompt**, so never `ps -o command` while holdout jobs run.

## Tests

`cargo test` is all green. Provenance tests in `a2d-cli` fail whenever there are **untracked files under `crates/`**, so commit first. The `a2d-lift` tests take about 20 s.

## Legacy (deprioritised)

- The metabolism, evolver, architect, escalation rungs and observer exist, but their value is unproven. The SWE-Bench Pro chain is parked.
- Pre-reset history: `docs/handoff-archive/`. Audit: `docs/research/2026-10-14-direction-audit.md`.
