# A²D Handoff Document

**Last updated:** 2026-10-15 (session 144: the lift-first roadmap is **Completed (falsified)** after the closing gate; E13 recorded)
**Update this document:** before context compaction, at session end, or when significant state changes. Keep it short: current truth only. History lives in git, `docs/experiments/LOG.md` and `docs/handoff-archive/`.

## Headline (read this first)

- **Owner's framing:** A²D is *autoresearch on agentic coding harnesses*, and you have the reins. The rules are in `docs/PROTOCOL.md`.
- **The lift-first roadmap is Completed (falsified)** (`docs/plans/2026-10-14-lift-first-roadmap.md`; report `docs/reports/2026-10-15-lift-first-outcome.md`). Three outcomes, kept separate:
  1. **Seed: RULED OUT** on Confirmation-1 under the registered analysis (E07 + E07b: +1.4 pp [−5.4, +8.8]; two-look upper bound +9.8).
  2. **Improved harness: none promoted.**
     - Within-task self-signal configurations failed their screens.
     - The parity base, H0 v6 (D's prompts plus a `main()` clause on the metabolism carrier), was +2.9 pp on train but −12.5 pp on validation (exploratory). Its parity on fresh families is not established.
  3. **Self-modification (P4): failed its development screen, unconfirmed, and not ruled out.**
     - Self-written lessons: E12 H\* = i1, 68.8% vs D 70.0%. That is the pre-registered **no-go**.
     - The uninformed control tied, so the "learning from failures" attribution is withdrawn.
     - Lessons beat C:h0 (+11 pp, exploratory) but never beat D.
- **Confirmation-2** (144 tasks, 36 families, `cb7cb496…`) is **unspent and unopened** apart from the disclosed `spreadsheet-eval` exclusion. It is reserved for one registered claim in a future regime.

## Open
1. **E13 is recorded.** D + `main()` clause vs D: −0.7 pp [−7.1, +5.7], so the clause is not the cause. D's own never-green varies 1–4 per train run, so the carrier never-green excess does not hold on train; the report is corrected.
2. **No new roadmap is active.** The report's § Recommended pivot is the proposed next step, pending an owner decision:
   1. check the carrier at zero model calls: byte-diff C:h0 vs D repair prompts on train, and mine the E11 C-arm runs for never-green;
   2. calibrate measurement (about 700 tuples per decision for ±5 pp; A/A and uninformed controls);
   3. one "D + X vs D" hypothesis in a stated regime;
   4. an owner decision on the instrument as a product.

## The instrument (`a2d lift`, `crates/a2d-lift/`)

```bash
cargo build -p a2d
./target/debug/a2d lift validate <suite> --jobs 4
./target/debug/a2d lift run --experiment <id> [--unregistered | --registered-at <sha> --expect-suite-hash <sha>] \
  --suite <dir> --model codex/gpt-6-luna --effort low --arms D,C:a,C:b --harness a=<json> --harness b=<json> \
  [--d-note n=<file> (arm D:n)] [--claim "C:a>C:b,C:a>D"] --k 5 --primary D --seed <n> --jobs 4 \
  --max-invocations <n> --max-wall-clock-secs <s> --out <dir> [--defer-scoring]
./target/debug/a2d lift manifest|score|pool|compare|verify-ledger ...
./target/debug/a2d distill --from <run>=<suite> --harness <in> --out <out> --log <prov> --name <n> \
  [--arm C:x] [--families <json>] [--mode informed|uninformed] [--max-dossiers 40]
```

- **Arms:** A (one-shot), B (best-of-k), D (repair loop), C / `C:<name>` (A²D metabolism with a harness), `D:<name>` (D with a note).
- **Oracle:** the candidate is the crate root, and the trusted tests are an earlier-declared `#[path]` module. Sources are deleted before the run, and a clean exit is required.
- **Classifier:** a whole-family bootstrap with a boundary tolerance of 1e-9. `--claim` = all listed comparisons Supported; IntegrityReview and Invalid dominate.
- **Arm-C harness keys** (`crates/a2d-cli/src/lift_arm_c.rs`):
  - `drop_enzymes`, `drop_coder_catalysts`;
  - `coder_prompt` (must keep `main()`), `coder_note`, `lessons`;
  - `route_previous_code`, `stop_at_public_green`, `dedupe_input`;
  - `prompt_format` (json, plain or repair), `escalation`.
- **Harnesses:** `harnesses/h0-v6.json` (parity base), `s4b-lessons.json` (researcher lessons), `p4-{i1,i2,u1,u2}.json` (search outputs).
- **Analysis scripts** (`docs/experiments/analysis/`): `p4_search.py` (the ratchet orchestrator, resumable), `c2_power.py`, `c2_joint_power.py`, `t0_stage.py`, `p4_0_probe.py`.

## Data
- Visible: `tasks/calibration-v2` (24), `tasks/dev-v2` (48), `tasks/c1-train` (68), and the combined `tasks/train-all` (140) and `tasks/train-clean` (72), both made of symlinks.
- Metric-only: `~/.a2d-holdout/c1-validation` (80). 2 of 4 looks used (S4, E12). Read only `summary.json` aggregates.
- Claim set: `~/.a2d-holdout/confirmation-2` (144). Never opened; unspent.

## Provider state
- Working lanes: Codex (`gpt-6-luna` coder, `gpt-6.1-sol` reviewer and author) and Claude subagents. The other subscriptions were stopped on purpose. The A² session shares the Codex account.
- Codex gotchas:
  - run reviewers and authors in copies-only temp dirs;
  - close stdin;
  - give the sandbox the toolchain's `rustc` path directly;
  - git commits from inside Codex's sandbox fail (index.lock), so commit outside it;
  - **never `ps -o command` while holdout jobs run** (argv carries prompts).

## Tests

`cargo test` is all green (24 result lines). Provenance tests in `a2d-cli` fail when there are **untracked files under `crates/`**, so `git add` first. The `a2d-lift` tests take about 30–90 s.
