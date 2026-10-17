# A²D Handoff Document

**Last updated:** 2026-10-17 05:55 UTC (session 145: C2-WEB: web search was open but unused, and confirmation-2 now has a count-only web audit; E007 screen passed; E005 provisional keep (+5.0 pp, confirmation deferred); **look 1 PASSED; confirmation-2 registered** at `78727f29`. E002, 4 Luna samples plus a Sol judge, is frozen as `runs/c2-confirm2/frozen.py`. Replicate 1 runs at 00:05 UTC 10-18 and replicate 2 at 00:05 UTC 10-19; then manifests, scoring and pooling. The oracle is now OS-sandboxed (C2-ISO2), with parity 420/420.)
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
- **Keep rule (eval v2.3, `nano/eval.py`):** all-pass is primary and fraction is a guard.
  - Screen (half): Δ ≥ thr/2.
  - Full: Δ > max(thr, 1.645·SE), with Δfrac not significantly negative.
  - Fresh confirmation replicate: Δ > thr/2, then promote.
  - `nano/aa.json` is E002's A/A, taken from its own two full replicates: thr_allpass 0.0357, thr_frac 0.0174.
- **Results log:** `nano/results.tsv` (every experiment and stage, including discards). Narrative and pre-registrations: `docs/experiments/LOG.md`, Cycle 2.
- **Report draft:** `docs/reports/cycle2-nanoharness-report.md`. The settled sections are written; fill in the **[pending]** ones at close.

## In flight / next
1. **Confirmation-2 (C2-CONFIRM2):** registered at `78727f29e53743dc5d989c62e1de23ab1a81e6c6`. The spec is `runs/c2-confirm2/registration.json`; the LOG entry holds the review dispositions and the power exception.
   - **Queue** (background runner `$SCRATCH/queue-runner.sh $SCRATCH/queue-2026-10-17.txt`, re-read line by line):
     - E005 (done: provisional keep), then E007 (screen on 10-17; its rest defers at the cap);
     - at ≥ 00:05 UTC 10-18, `python3 -I nano/eval.py confirm --rep 1 --registered-at 78727f29… --jobs 8`, then E007 resume, then E006;
     - at ≥ 00:05 UTC 10-19, `confirm --rep 2 …` the same way, then E006 resume, then E008, E009 and E010;
     - at ≥ 00:05 UTC 10-20, 10-21 and 10-22: resumes for E006–E010, then `eval "$(python3 -I nano/confirm_best.py E005 … E010)"` (once on 10-20, twice on the later days). `confirm_best` enforces the pre-registered maximum of 2 confirmations.
     - **From line 6 on, every line is `$SCRATCH/qrun.sh <tag> $SCRATCH/qcmds/L<nn>-<tag>.zsh`.** The full output goes to `$SCRATCH/qlogs/<UTC>-<tag>.log`, and the runner output shows one `"decision": "qrun <tag> rc=<n> … last=<line>"` per line. Any `rc≠0` is a failure to investigate.
     - The runner itself (`queue-runner.sh`) must not be edited while it runs, and lines ≤ the current one must never change. Two copies-only reviews (Sol and a fresh Claude, 10-17) checked this automation; their dispositions are in the LOG.
     - A cap-deferred stage prints "deferred". Re-running a command is free and safe: completed stages are cached, and `log_result` logs each (id, stage, decision) once. So over-provision resume lines rather than re-queue by hand. Append lines to the end only.
   - **End of the batch (pre-registered in the LOG, 10-17):** `confirm_best` mechanically confirms the largest full-stage provisional keep, at most 2 confirmations. This happens even if the 11k phase-B trigger has fired.
     - If one is promoted: `python3 -I nano/eval.py aa-from-replicates --id <id>`, then phase B starts from that incumbent. Otherwise phase B starts from E002.
     - Phase B is started by hand (`docs/experiments/analysis/c2_phaseb.py`).
   - **After both replicates have generated:**
     1. `python3 -I nano/eval.py confirm-manifest`;
     2. commit and push `runs/c2-confirm2/manifest-r*.json`;
     3. `python3 -I nano/eval.py confirm-score` (jobs 6). It writes `pooled-summary.json` and `decision.json`. Only the pooled claim decides, **subject to C2-WEB:**
     4. `runs/c2-confirm2/web-audit-r{1,2}.json` (queued after each confirm line; counts only) must show 0 web-search events. Any search means the outcome is IntegrityReview. This amendment was registered in the LOG before any call. Commit both audit files with the manifests.
   - **Freeze window, through `decision.json`:** do not touch `crates/`, `Cargo.toml`, `Cargo.lock` or the `CONFIRM2…main()` section of `nano/eval.py`. The controller and the runner both refuse to run if they change.
     - Rebuilding the binary from the same source is fine: the binary sha is logged, not enforced, because debug builds are not byte-reproducible.
     - The harness-supplied-tests op idea waits until this window closes. So does the **provider web-search fix (C2-WEB):** add `-c web_search="disabled"` and count `web search:` lines in `codex_tool_calls`, with a test.
     - codex-cli 0.162 ignores `--disable web_search_request`, so web search is ON in every `a2d lift` call until then. There were 0 uses in about 15k recorded transcripts. `eval.py` stops a dev experiment on any search, and phase B invalidates the round.
     - **The fix is ready** on branch `c2-web-provider` (4b80fe67, pushed): provider tests 20/20, a2d-lift 66/66, and a live 1-task smoke with no deprecation notice and a pass.
     - **Merge it after the batch's confirmation step and before phase B round 1,** not right after `decision.json`. Merging mid-batch would change the provider between an experiment's stages (Claude review). Then rebuild and log it.
   - **If a replicate's generation fails or aborts:** the runner refuses to rerun it by design. The registered policy allows one relaunch with the same seed:
     1. move `~/.a2d-holdout/runs/c2-confirm2-r<n>` (and its `.log`) to `…-failed1`;
     2. move `runs/c2-confirm2/attempt-r<n>.json` to `attempt-r<n>-failed1.json`;
     3. log it in the LOG, then re-run the queue line.
     - A second failure means confirmation-2 is "not run": no claim, and the set is spent.
   - **While holdout or confirmation jobs run:** never print process command lines (`pgrep -f` pids only).
2. **Search continues on dev** from incumbent E002 (thr_allpass 0.0357). Look 1's numbers stay in `~/.a2d-holdout/runs/c2-look1/report.json` until the final report; proposers learn only PASS.
   - For each new run, read its first prompts and DIAG lines, then log it in the LOG and `results.tsv`.
   - **E005 (diverse angles): provisional keep at full, +5.0 pp (SE 2.8), 123 vs 116/140.** Its confirmation waits for the sequencing rule: once E006–E010 have reached full, the largest Δ is confirmed first.
     - Per-angle: no per-sample cost, small decorrelation, +5 tasks from generation.
     - The in-run E002 arm drew low (116 vs 123 and 117 on 10-16), so expect a close confirmation. See the LOG.
   - **E007 (Sol-directed repair): screen passed,** 65/68 vs 62/68 (+4.4 pp). Its rest stage was cap-deferred and resumes on 10-18. Its first prompts were read and are as designed.
   - **Phase-B tooling:** live-smoked end to end (`runs/c2-pc-smoke2`, after the C2-WEB fix): valid proposals, the round runs and scores. The driver is `$SCRATCH/phaseb_smoke.py <root> <family> <dir>`. The real `runs/c2-pc` is not created yet: the first `propose` freezes the incumbent.
3. **Proposer comparison (phase B)** triggers at ≥ 3 keeps or 11k Luna left. The protocol is frozen (plan addendum 2026-10-16) and the tooling is ready (`docs/experiments/analysis/c2_phaseb.py`).
- **C2-ISO2 (merged 32e72a53):** the oracle's compile and test run now execute under sandbox-exec.
  - Both steps: no network and no file access under `$HOME` or the temp roots.
  - Run step: no process spawning, Mach lookups or Apple Events.
  - The environment is cleared, and the harness sandbox gets the same broker closure.
  - Parity: 420/420 identical against the old binary. References: 140/140, 100/100 and 144/144.
- **Claude-quota work** (the quota resets Sunday 12:00): C2-E2 (Claude judge 66/68 vs Sol 68/68), C2-FA2 failure taxonomy, proposer round c1 (E009, E010), and the reviews for C2-ISO2 and C2-CONFIRM2. There is no live Claude lane: the CLI is blocked inside the session sandbox.
- **Budget:**
  - Cycle-2 Luna: 275 (10-15), about 2.9k (10-16), and 1,060 (10-17 so far) plus about 22 dry-run calls outside `runs/`.
  - About 20.7k Luna left of 25k. Confirmation-2 costs about 1.6k Luna and 125 Sol per replicate.

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
