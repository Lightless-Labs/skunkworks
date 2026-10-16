# A²D Handoff Document

**Last updated:** 2026-10-16 23:25 UTC (session 145: **first cycle-2 keep**; C2-FA1/FA2 failure analyses; C2-E2 cross-vendor judge 66/68; 6 candidates registered (E005–E010); look 1 queued for 00:05 UTC. E002b, 4 Luna samples + a Sol judge, beat the plain repair loop: +7.1 pp all-pass on the confirmation replicate and +10.7 at the full stage. The random-pick control gives −1.4, so the judge is the active ingredient. `nano/harness.py` is now the E002 judge harness.)
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
1. **The queue for 2026-10-17 (UTC)** is a background runner (`$SCRATCH/queue-runner.sh $SCRATCH/queue-2026-10-17.txt 202610170005`). It runs these lines in order:
   - (a) **dev-holdout look 1**: `python3 -I nano/eval.py look --n 1 --registered-at 8e07801e588e8d48334fa267dd2dd7109ad86f57 --seed 6161 --jobs 8`. It prints only gate PASS/FAIL; the full report stays in `~/.a2d-holdout/runs/c2-look1/report.json`.
   - (b) E005, E007, E006, E008, E009, E010, each via `python3 -I nano/eval.py run --id <id> --candidate nano/candidates/<file> --proposer <claude|sol> --jobs 6 --stop-after full --hypothesis "<as in LOG>"`.
   - Every stage self-defers when today's Luna count plus its worst case would exceed 3k (`python3 -I nano/eval.py budget` shows the count).
   - **If the runner died:** re-run the same lines by hand. Cached stages are reused, and the experiment manifests keep them immutable.
   - **Do not touch `crates/`, `Cargo.toml` or `Cargo.lock` (not even uncommitted edits) until look 1 has finished.** They are frozen against its registration (`8e07801e`), and any change voids it. Since C2-ISO, harnesses run under a seatbelt sandbox.
2. **If look 1 PASSES:**
   - freeze E002;
   - get one Codex Sol review and one fresh-Claude review;
   - pre-register confirmation-2: arms D, B and X:frozen at r = 2 (one replicate per day, pooled with `a2d lift pool`), `--claim` vs D and B.
   - **If it FAILS:** continue the search.
3. **Proposer comparison** (A²D architect = informed, with failure dossiers carrying labels only, vs uninformed, at equal budget): draft the protocol, freeze it, and get a review. The goal triggers it after ≥ 3 keeps; a budget-reserve trigger is to be added as an addendum.
- **Claude-quota work, 2026-10-16 evening** (the owner reset the weekly Claude quota; it resets Sunday 12:00). All offline, through subagents:
  - **C2-E2:** a Claude Opus judge matches Sol on E002's decisive tuples, 66/68 vs 68/68 (random 36.3). The selection lever is not vendor-specific.
  - **C2-FA2:** failure taxonomy. 29/34 misreadings are shared; boundary 9, semantics 9, performance 8 (the timeouts).
  - **Proposer round c1:** 3 independent Opus proposers. E009 (failure checklist) and E010 (two readings per call) are registered and smoked, and c1a-1 was declined because its probes cannot run under `public_test`.
  - **No live Claude lane:** the `claude` CLI returns nothing inside the session sandbox, and running it outside the sandbox was refused by the permission classifier. The owner was told what it would take to enable it.
- **State of the search:**
  - Incumbent E002; thr_allpass 0.0357 (eval v2.3; A/A from its own replicates).
  - C2-FA1: the judge is at its ceiling (0 misses in 249), and the failures are misreadings shared across samples. So the levers are generation-side.
  - Registered: E005 (diverse angles, claude), E006 (Sol spec notes, claude), E007 (Sol-directed repair, sol), E008 (4th-sample synthesis, sol).
  - Sol's r1-1 (Luna at high effort) was declined as out of scope.
- **Budget:**
  - Cycle-2 Luna: about 3.2k (275 on 10-15; 2,903 + about 15 smoke calls on 10-16).
  - Sol: 407 on 10-16.
  - About 21.8k Luna left of 25k; deadline 2026-11-30.

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
