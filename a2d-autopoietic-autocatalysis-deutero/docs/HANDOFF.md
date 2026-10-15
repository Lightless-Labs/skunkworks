# A²D Handoff Document

**Last updated:** 2026-10-15 (session 144: gate passed; multi-harness controller merged; E10 (G0, H0 v5) FAILED never-green 5 vs 2; H0 v6 = parity with D; E10b running)
**Update this document:** before context compaction, at session end, or when significant state changes. Keep it short: current truth only. History lives in git, `docs/experiments/LOG.md` and `docs/handoff-archive/`.

## Headline (read this first)

- **Owner's framing:** A²D is *autoresearch on agentic coding harnesses*, and you have the reins. The rules are in **`docs/PROTOCOL.md`**; the goal and completion criteria are in `docs/plans/2026-10-14-lift-first-roadmap.md` (P4 decision by 2026-11-30).
- **P2: the seed lift is RULED OUT** (C vs D +1.4 pp, CI [−5.4, +8.8]).
- **P3 is completed as "not promoted"** (roadmap Addendum, P3 exit).
  - S1–S4 failed their rules. Within-task self-signals do not beat the repair loop.
  - **S4b survived its development screen.** Four researcher lessons on H0 v4 gave +11.1 pp over H0 v4 (net +8 of 72; CI [+2.8, +19.4]) and +5.6 pp over D, on families T0 never saw. This authorises P4 spend only.
  - **Plumbing defects dominated the early readings:**
    - the missing `main()` clause;
    - input rendered three times;
    - JSON-escaped repair inputs;
    - an evolver that rewrites the coder's prompt mid-task.

    All are fixed and unit-tested; see `docs/solutions/best-practices/inspect-first-prompts-of-a-new-harness-2026-10-15.md`.
- **The P3 → P4 gate passed.** Reviewers: Codex Sol and a fresh Claude subagent. Every item has a disposition in `docs/plans/2026-10-15-p4-autopoietic-lift.md` (v3).
- **P4 base: H0 v6** (`harnesses/h0-v6.json`, after E10 failed). H0 v6 is parity with D: D's `CODER_SYSTEM` plus the `main()` clause, `prompt_format: repair`, and `escalation: false`. It replaced H0 v5, an evolver-free coder enzyme with:
  - `drop_coder_catalysts: [enzyme_defs]`;
  - `plain_inputs`;
  - `dedupe_input`;
  - previous-attempt routing;
  - `stop_at_public_green`.
- **P4 = lessons that A²D writes itself** (`a2d distill`, Luna architect, per-task train labels only, starting from empty lessons).
  - Searches: I1 and I2 informed; U1 and U2 an uninformed control (paired seeds 101 and 202).
  - Ratchet: the incumbent is re-run fresh each generation, and a candidate is accepted at ≥ 4 net tasks.
  - Then one validation look. Go/no-go: C(H\*) − D ≥ +10 pp.
  - Then a multi-harness Confirmation-2 run at r = 2. Completion needs C(H\*) SUPPORTED vs C(H0 v5), D and B.
- **Confirmation-2:** 144 tasks, 36 families, hash `cb7cb496…`, final (LOG). The power shortfall is recorded.

## In flight
1. **E10b (G0 retry, H0 v6):** D vs C on `tasks/train-all`, seed 22, out `runs/2026-10-15-e10b-g0-h0v6`, stdout `$SCRATCH/e10b.out`.
   - Pass needs all of: never-green C − D ≤ 2; rate within ±5 pp; zero evolver and consultation calls.
   - **If it fails, P4 stops** and the gap is recorded as a finding (LOG E10b).
   - The early check has passed: first-prompt bodies are identical to D's.
2. **If E10b passes, launch E11:** `python3 -I docs/experiments/analysis/p4_search.py --search I1 --mode informed --seed 101 --base harnesses/h0-v6.json --suite tasks/train-all --g0-run runs/2026-10-15-e10b-g0-h0v6 --out runs/2026-10-1x-p4-I1`. Then U1 (uninformed, seed 101), I2 and U2 (seed 202).
   - Run them in parallel with `--jobs 3`.
   - The multi-harness controller (`--harness name=path`, `C:<name>`, `--d-note`, `--claim`) is merged.

## Next moves after E11
- Validation look: C(H\*_I1), C(H\*_I2), C(H\*_U1), C(H\*_U2), C(H0 v5) and D on c1-validation (aggregates only).
  - H\* = the better informed search.
  - The control consequence and the go/no-go are pre-registered in the plan.
- If go:
  - joint power simulation (three comparisons);
  - register the Confirmation-2 run: D, B, C(H0 v5), C(H\*), C(R) (S4b lessons on H0 v5, descriptive), and D + H\*-lessons (descriptive); r = 2.
- If no-go: the roadmap completes as falsified with the precise wording (plan § Validation).

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
- **Harness-as-data for C** (`--harness`, validated and hashed). Keys (see `lift_arm_c.rs`), plus `lessons`, `dedupe_input`, `plain_inputs` and `drop_coder_catalysts`:
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
- P3: about 1.45k used (closed).
- P4 ≤ 5k (plan about 4.0k: G0 about 400, searches about 2.9k, validation about 700). Confirmation-2 about 3.5k. Reserve ≈ 2k.

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
