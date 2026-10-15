# A²D Current Protocol (consolidated 2026-10-15)

This file gathers the rules **currently in force**. History and rationale live in the roadmap Addenda (`docs/plans/2026-10-14-lift-first-roadmap.md`) and in `docs/experiments/LOG.md`. When they conflict, the latest dated Addendum wins, and this file is updated with it.

## Measurement
- **Arms.**
  - A: one-shot.
  - B: best-of-k, selected by public tests.
  - D: repair loop on public-test output, stopping at public-green.
  - C: the A²D harness under test.
  - All arms use one coder lane: `codex/gpt-6-luna`, effort low, no tools.
- **Budget:** an equal invocation cap per task, **k = 5**. Realised invocations, tokens and provider time are reported beside every claim, and claims say "at an equal invocation cap".
- **Submission rule:** most public tests passed, then compiled, then the **latest** attempt.
- **Classes**, on the paired whole-family cluster bootstrap:
  - SUPPORTED: the 95% lower bound is above 0 **and** the point lift is at least 10 pp;
  - RULED OUT: the 95% upper bound is below 10 pp;
  - otherwise INCONCLUSIVE.
  - Guards: at least 10 families and at least one discordant pair. If more than 5% of tuples are excluded, the run is Invalid.
- **Claims:** SUPPORTED requires C to beat **both B and D**. RULED OUT is decided by the primary comparison (C vs D) alone.
- **Power targets** (simulate the implemented classifier): P(SUPPORTED | true 15 pp) ≥ 0.8 and P(RULED OUT | true 0) ≥ 0.8. P(SUPPORTED) at a true 10 pp is about 0.5 by construction.
- **Integrity:** a hidden test name in any prompt, or any coder tool call, gives IntegrityReview.

## Data
- **Confirmation-2** (`$A2D_HOLDOUT_DIR/confirmation-2`, extended blind) is the only claim set. It is never opened, and it is scored once by one registered run, with at most one pre-registered replicate extension.
- **Development data:**
  - `tasks/calibration-v2`, `tasks/dev-v2`, `tasks/c1-train`: fully visible.
  - `$A2D_HOLDOUT_DIR/c1-validation`: metric only, read through `summary.json`; per-task and per-family statistics are never computed.
- **No command lines while holdout jobs run:** Codex `exec` carries the task prompt in argv. Use `pgrep -f '<pattern>'` (pids only), never `ps -o command` or `pgrep -fl` (Confirmation-2 deviation, 2026-10-15).
- **Hidden-test boundary:** test *content* never reaches an arm or enzyme. In P4 only, train-family aggregate pass/fail labels may reach the proposer.

## Runs
- **Development lane:** `a2d lift run --unregistered`, plus a one-row LOG pre-registration (id, mechanism, kill threshold, budget) written before the run. The ledger is the record.
- **Confirmation lane:**
  1. registration commit with `--registered-at` and `--expect-suite-hash`;
  2. `--defer-scoring`;
  3. `a2d lift manifest`, committed;
  4. `a2d lift score --expect-head`.
- **Harness:** `--harness <json>`, validated, hashed and recorded. Every harness step's output must have a declared consumer. A `coder_prompt` must keep the seed's `main()` clause, because the metabolism's sandbox compiles the file as a program (E08). Check a new harness's first feedback prompts early in its first run.
- **Shared account:** the A² session shares the Codex account. This is a standing condition, mitigated by seeded tuple and arm order. This session never runs holdout authoring alongside a registered run.
- **Budget envelope** (remaining ≈ 15k): P3 ≤ 3k, P4 ≤ 5k, Confirmation-2 ≈ 5k, reserve ≈ 2k.

## Process
- Phase gates are reviewed by Codex `gpt-6.1-sol` plus a fresh Claude subagent, in empty temp dirs. Record who was consulted.
- Commit atomically with a `[P<n>]` tag, and push.
- HANDOFF stays at 250 lines or fewer. Compound learnings in `docs/solutions/`.
