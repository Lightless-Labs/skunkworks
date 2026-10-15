# A²D Handoff Document

**Last updated:** 2026-10-15 (session 144: **cycle 2 under way**. C2-E1: a stronger judge picks correct files where Luna's own signals fail. Nanoharness and eval v2.1 built; the A/A + parity run and dev-holdout validation are in flight.)
**Update this document:** before context compaction, at session end, or when significant state changes. Keep it short: current truth only. History lives in git, `docs/experiments/LOG.md` and `docs/handoff-archive/`.

## Headline (read this first)

- **Owner's framing:** A²D is *autoresearch on agentic coding harnesses*, and you have the reins. Never pause or ask for direction. A failed variant is a data point, not a verdict.
- **Active goal: cycle 2,** the session `/goal` of 2026-10-15. Working plan: `docs/plans/2026-10-15-cycle2-nanoharness.md` (its addendum holds the design-review dispositions and definitions).
- **Cycle 1 is closed:** the lift-first roadmap is Completed (falsified); report `docs/reports/2026-10-15-lift-first-outcome.md`. It closed one plan, not the project.
- **C2-E1 (offline, train): the information source exists.**
  - On the 39 tasks where Luna's public-green candidates disagree on hidden tests, a judge reading only the spec picked a passing file 37/39 times (Claude, Sonnet) and 39/39 times (Sol; audited, no leakage).
  - Equal-budget pick rate: 0.83–0.85 vs D 0.74.
  - Rule met, so cross-model review is the first lever. The live judge is Sol ("Luna generation plus Sol selection"), because the Claude CLI is unauthenticated from subprocesses.
- **Instrument for cycle 2:**
  - `--external name=path` runs a Python harness as arm `X:<name>` (JSON lines; metered model calls; public tests through D's oracle; hidden tests unreachable);
  - `--lane name=codex/<model>[:effort]` adds judge lanes on the shared budget;
  - `nano/harness.py` v0 equals D (fixture parity test);
  - `nano/eval.py` v2.1 provides immutable experiments, completeness checks, an A/A threshold, screen → full → confirmation replicate, and a static isolation guard.

## In flight
1. **C2-AA** (`python3 -I nano/eval.py aa`): a 3-arm run D, X:a, X:b on `tasks/train-all`, out `runs/c2-aa-200daaa763c4/`, stdout `$SCRATCH/c2-aa.out`. It writes `nano/aa.json` with the threshold and parity results. Parity rule: X − D within ±5 pp all-pass, and byte-identical first prompts.
2. **Dev-holdout:** 25 new blind families (100 tasks) are authored and assembled in `~/.a2d-holdout/dev-holdout`. Validation is running (`$SCRATCH/devholdout-validate.txt`; read verdicts only). Next come an independent-solve audit (Sol, D, k = 3), adjudication if needed, and a hash in the LOG.

## Next
- After C2-AA, run E002 (sample 4 + Sol judge), E003 (random-green control) and E004 (repair to green, then judge), all pre-registered in the LOG:
  - `python3 -I nano/eval.py run --id E002 --candidate nano/candidates/E002_judge.py --hypothesis "..." --jobs 8`.
  - Avoid overlapping CPU-heavy validation with evaluations.
- Seed list for proposers: `nano/seeds.md`. Results: `nano/results.tsv`.
- Budget: ≤ 3k Luna invocations a day and ≤ 25k in total; deadline 2026-11-30. Spend is tracked in `results.tsv` and the LOG.

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
