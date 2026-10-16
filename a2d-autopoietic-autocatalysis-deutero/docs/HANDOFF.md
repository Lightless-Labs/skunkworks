# A²D Handoff Document

**Last updated:** 2026-10-16 (session 145: **first cycle-2 keep**. E002b, 4 Luna samples + a Sol judge, beat the plain repair loop: +7.1 pp all-pass on the confirmation replicate and +10.7 at the full stage. The random-pick control gives −1.4, so the judge is the active ingredient. `nano/harness.py` is now the E002 judge harness.)
**Update this document:** before context compaction, at session end, or when significant state changes. Keep it short: current truth only. History lives in git, `docs/experiments/LOG.md` and `docs/handoff-archive/`.

## Headline (read this first)

- **Owner's framing:** A²D is *autoresearch on agentic coding harnesses*, and you have the reins. Never pause or ask for direction. A failed variant is a data point, not a verdict.
- **Active goal: cycle 2,** the session `/goal` of 2026-10-15. Working plan: `docs/plans/2026-10-15-cycle2-nanoharness.md` (its addendum holds the design-review dispositions and definitions).
  - Done when a frozen harness is SUPPORTED vs both D and B on confirmation-2, or when the budget (≤ 3k Luna/day, ≤ 25k total) or the deadline (2026-11-30) is reached.
- **Cycle 1 is closed** (falsified roadmap). Report: `docs/reports/2026-10-15-lift-first-outcome.md`.
- **Incumbent = E002** ("Luna generation plus Sol selection", `nano/harness.py`; v0 = D is at `nano/history/before-E002b.py`).
  - 4 Luna samples, then one Sol judge call that picks among the distinct public-green files from the spec alone.
  - Exploratory dev-set result vs v0 (= D): screen +11.8, full +10.7 (SE 2.9), confirm +7.1 (SE 3.5).
  - **Not cost-matched:** about 4.0 Luna + 0.9 Sol calls and 6.4M tokens per 140 tasks, vs about 1.3 Luna and 1.4M for D.
  - Sol is from the same vendor as Luna, so this is not a cross-family claim.
- **Keep rule (eval v2.2, `nano/eval.py`):** all-pass is primary and fraction is a guard.
  - Screen (half): Δ ≥ thr/2.
  - Full: Δ > max(thr, 1.645·SE), with Δfrac not significantly negative.
  - Fresh confirmation replicate: Δ > thr/2, then promote.
  - **`nano/aa.json` is stale** (calibrated on v0); `eval.py run` refuses until the A/A is re-run for the new incumbent.
- **Results log:** `nano/results.tsv` (every experiment and stage, including discards). Narrative and pre-registrations: `docs/experiments/LOG.md`, Cycle 2.

## In flight / next
1. **Offline failure analysis of E002** (no Luna). Pool: `runs/2026-10-16-c2-e002-pool/`, which scores every coder sample of the E002 and E003 runs on public and hidden tests. Question: where does E002 still lose tasks (judge mistakes, every sample wrong, no green file)? That decides the next lever.
2. **2026-10-17 (UTC), after the daily reset:**
   - (a) the A/A for the new incumbent: 2 arms (X:a, X:b), about 1.1k Luna + 0.25k Sol;
   - (b) the first dev-holdout look: incumbent vs D, r = 2, gate = all-pass ≥ +10 pp with cluster CI low > 0, about 1.1k Luna. Read aggregates only (`summary.json`).
   - Both are pre-registered in the LOG.
3. **Next candidates against E002** (seeds 3–5 and 8 in `nano/seeds.md`, plus Codex Sol proposals):
   - a judge with a clause checklist;
   - judge-guided repair;
   - a clause-check first-attempt prompt;
   - diverse sampling.
   - Choose by the failure analysis.
4. **Proposer comparison** (A²D architect vs an uninformed proposer) after ≥ 3 keeps.
- **Budget:**
  - Cycle-2 Luna so far is about 3.2k: 275 on 10-15 and 2,903 on 10-16, so **10-16 is at the cap**.
  - Sol: 306 on 10-16.
  - The A² session shares the Codex account.

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
- Dev-holdout: `~/.a2d-holdout/dev-holdout`, 100 tasks in 25 new families, hash `9114bae3…`. Validated and audited 100/100.
  - Metric-only (aggregates only).
  - Looks: after each confirmed keep (at most 4) plus a final one. Gate to confirmation: incumbent vs D all-pass ≥ +10 pp, with CI low > 0.
- Claim set: `~/.a2d-holdout/confirmation-2` (144). Never opened; unspent. Valid only in the current regime.

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
