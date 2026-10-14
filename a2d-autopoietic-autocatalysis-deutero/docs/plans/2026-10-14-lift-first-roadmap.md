# Plan: Lift-First Roadmap

**Created:** 2026-10-14
**Status:** Active — phase P0
**Reviewed:** 2026-10-14 (independent review, Codex gpt-6.1-sol xhigh). The other non-Claude lanes were unavailable; see Blockers. Changes adopted: repair-loop comparator, three-way result classification, pilot/development/confirmation split, frozen controller, B-headroom calibration, harness-first P4, process metrics as diagnostics only.
**Addendum:** 2026-10-14 — coder lane: the owner stopped the Kimi, Z.ai, MiniMax and Gemini subscriptions on purpose and suggested Luna via Codex or Pi. `gpt-6-luna` works through Codex with A²D's provider flags. `gpt-6.1-luna` is not offered on this ChatGPT account (Codex model list fetched 2026-10-14 has 6.1 only as Sol). Pi's `openai-codex` provider is not authenticated. P0/P1 updated accordingly.
**Supersedes as primary goal:** `swe-bench-pro-blind-evaluator-adapter.md` (parked), the Senior SWE-Bench retry chain (parked)
**Grounding:** `docs/research/2026-10-14-direction-audit.md`, `docs/research/2026-10-14-dgm-rqgm-relevance.md`
**Decision deadline:** P2 confirmation result recorded by **2026-11-30**. Missing it is a non-result, not a falsification: record "not measured by deadline", stop adding seed-cycle complexity, and continue per Completion.

## Objective

Find out whether A²D produces working software more reliably than simple uses of the same model at the same cost, and whether its self-modification increases that margin. Establish either answer cleanly.

Supported, ruled-out and inconclusive findings all complete this plan. Building infrastructure does not.

## Metric

All arms use the same coder model and the same resource budget. Every arm's final submission is scored once by hidden tests. Public tests are the only signal any arm may use while working.

| Arm | Definition |
|---|---|
| **A — one-shot** | pass@1. |
| **B — best-of-k** | k independent attempts; the submission is chosen by public tests, with frozen tie-breaking. |
| **D — repair loop** | up to k attempts: coder, public tests, failures fed back. Submit the best by public tests. |
| **C — A²D** | at most k provider invocations per task across all enzymes. Every enzyme uses the coder model at first (no portfolio). |

- **Primary comparison:** C vs the stronger of B and D. Which one is stronger is fixed from pilot results before confirmation.
- **Lift:** C's hidden-pass rate minus the comparator's, paired over tasks.
- **Budget:** matched invocations. Also report input, output and reasoning tokens, cost where known, test compute, wall-clock, and retries/fallbacks. A timeout or missing artifact counts as a failure. Infrastructure exceptions are predefined in the pre-registration.
- **Result classes:**
  - **SUPPORTED** — the 95% lower bound on lift is above 0 and the point estimate is at least 10 pp.
  - **RULED OUT** — the 95% upper bound is below 10 pp.
  - **INCONCLUSIVE** — anything else. At most one pre-registered sample extension is allowed. If the result is still inconclusive after that, treat it as NO-GO for adding complexity.
- **Statistics:** paired cluster bootstrap over task families, with replicates kept paired. Power-plan the confirmation n from pilot discordance, targeting 80% power at 10 pp and a cap of 150 tasks. Report exact McNemar alongside. Twenty to thirty tasks is a pilot, not a confirmation.

## Task sets

- **Calibration/pilot:** used for headroom checks and power planning. Not reused.
- **Development:** free use for building and debugging. Results from it are never claimed as findings.
- **Confirmation-1:** scored once, at the end of P2.
- **Confirmation-2:** fresh task families reserved for the final P4 claim, scored once.
- Hold out whole task families, not just individual tasks.
- Confirmation tasks and their hidden tests:
  - are authored or validated by a non-Claude provider;
  - live outside the repo (under `$A2D_HOLDOUT_DIR`), with only content hashes committed;
  - are never opened by the developing agent.
- Submission artifact hashes are committed before confirmation scoring.

## Controller (integrity)

`a2d lift` is the only path that runs arms and scores them. It writes an append-only JSONL ledger of registrations, invocations, submissions (artifact hashes) and scores.

- The pre-registration records the git hash of the controller and oracle files.
- Any change to those files between registration and run voids the registration. Check this mechanically with `git diff <reg-sha>..HEAD -- <controller paths>`.
- Self-modifying agents (evolver, architect, P4 harness search) may not edit the controller, the oracles, stopping rules or the ledger. Extend `CONSTITUTION.md` protected paths accordingly.

## Development loop (process)

Each unit of work is one **experiment**:

1. **Pre-register** in `docs/experiments/LOG.md` before any live run, using the template.
2. **Smallest change** that lets it run. Write the tests first; `cargo test` must be green.
3. **Run** through the controller. Artifacts go under `runs/<date>-<experiment-id>/`. Never cite `/tmp`.
4. **Record** the result class, the confidence interval and the artifact path. Negative and inconclusive results are results.
5. **Decide** by the metric. Overwrite HANDOFF (≤250 lines). Compound non-obvious learnings into `docs/solutions/`.

Commit messages carry the tag, e.g. `feat(a2d): [P0] split public and hidden oracles`.

### Tripwires

- **T1 — results cadence:** no more than 2 consecutive sessions, or 10 commits, without a new LOG result (pilot results count; "it compiled" does not). On a hit, the next action is a run.
- **T2 — blockers:** when blocked on external input (credentials, quota, access), log it under Blockers and switch to a track that is not blocked. Never build scaffolding around a blocker.
- **T3 — proportional safety:** new redaction, scrubbing or sandbox hardening needs a triggering incident in a run artifact. Exemption: oracle and controller isolation required by this plan.
- **T4 — evidence must exercise the change:** a constant-artifact replay certifies nothing. Plumbing is gated by `cargo test`; outcome claims by the controller.
- **T5 — budget:** every live run has pre-registered invocation and wall-clock caps; abort at 2× and record why. The global cap for this plan is 20,000 provider invocations, re-assessed at each phase gate.
- **T6 — goal phrasing:** goals are stated as outcomes, never as constraints.
- **T7 — bounded search:** at most 5 P4 hypotheses in total, and at most 2 P1 suite redesigns. There is no reset-on-success counter.

### Model diversity

Phase gates and key designs (oracle split, suite, P4 search space) get independent review from at least two non-Claude providers when available. When fewer are available, record which ones were and proceed; never wait.

Reviewers run in an empty temporary directory with copies of the inputs, never with `-C <repo>`. `codex exec -s read-only` is not enforced on this machine (see `docs/solutions/` sandbox note). Delegate bounded implementation to Codex. Use Claude subagents for repo navigation and parallel independent tasks.

### Deutero layer (process learning)

At each phase gate, compute process diagnostics from git and the LOG:

- results per session;
- wall-clock and invocations per result;
- share of commits tagged to an experiment;
- tripwire hits.

These are **diagnostics, not gates**: commit counts are editable, and fewer tripwire hits can mean evasion. Process changes are proposed as dated **Addenda** stating the outcome they target (results per unit of cost), reviewed by a non-Claude provider, and revisited at the next gate. Stopping rules, the controller and oracles are outside process self-modification.

## Phases

### P0 — Oracles, controller, first real baseline

1. Split each challenge into `public_tests` and `hidden_tests`. The live metabolism, portfolio ranking and `failure_report` see only public tests. Today they see `scoring_benchmark()` (`crates/a2d-cli/src/main.rs:5377`). Unit tests must prove hidden tests cannot reach any arm.
2. Build `a2d lift`: arms A, B, D and C, plus `--k`, `--model` (must accept a Codex-backed model such as `codex/gpt-6-luna`; the CLI registry has no Codex provider and no coder override today, only `A2D_TESTER_PROVIDER` and `A2D_ARCHITECT_PROVIDER`), the ledger, a summary with paired confidence intervals, and result classification.
3. Add holdout loading from `$A2D_HOLDOUT_DIR` with hash verification.
4. Extend the protected paths to cover the controller and oracles.
5. **Tiny real run:** arm A on about 5 development tasks with a live, available model. Mocks miss extraction and provider failures.

You may move `lift` and the oracle code into a module or crate if `main.rs` build time slows the loop.

**Exit:** the hidden-test isolation tests pass, all four arms run end to end on mocks, and the tiny real run is recorded in the LOG.

### P1 — Calibrate a non-saturated suite

- **Model:** Luna via Codex, `gpt-6-luna`, with reasoning effort pinned in the pre-registration. If `gpt-6.1-luna` appears in Codex's model list, re-register with it rather than switching mid-experiment. If Luna saturates the pilot (B above 85%), drop to `gpt-5.6-luna` or a lower reasoning effort, recorded as a suite or model redesign under T7.
- **Suite:** fresh, bounded, std-only Rust micro-crate task families. Examples: streaming codecs and parsers, dependency resolution, a transactional key-value store, schedulers, and seeded bug-repair variants of reference crates.
- **Oracles:** validate each oracle against an independent reference implementation and against deliberately faulty mutants (an oracle that passes a mutant is rejected).
- **Plumbing and pilot only:** the Polyglot Rust subset has 30 exercises and high contamination risk, so it is not used for confirmation.
- **Optional capped feasibility pilot:** Rust-SWE-bench (arXiv 2602.22764; verify first). SWE-Bench Pro stays parked.
- **Headroom:** on the pilot, A must pass 20–70% of tasks and B must pass no more than 85%, so C has room above B. Calibrate on whole families; never select tasks by keeping individual one-shot failures.

**Exit:**
- Confirmation-1 and Confirmation-2 families are generated and validated outside the repo, and their hashes are logged.
- The non-Claude review of the suite is recorded.
- Power planning is done.

### P2 — Seed lift test

- Run arms A, B, D and C at k = 4 (pre-register any other value with its reason). Verify that every evaluated component actually executes within the budget.
- Run the pilot on development tasks, fix the primary comparator, then score Confirmation-1 once.

**Exit:** a result class is recorded. Continue to P3 regardless of the class.

### P3 — Exploratory ablation and simplification

- Measure each component's marginal effect against arm C on development tasks: evolver, architect, escalation rungs 4–6, `failure_report`, lineage topology vs seed, and portfolio (with equal resource access).
- Specify how any freed budget is reallocated, and check interactions.
- Turning a component off by default needs a recorded estimate. Deleting its code needs an upper confidence bound below meaningful effect, or a significant build-time or complexity cost.

**Exit:** a minimal configuration, recorded as the frozen harness for P4.

### P4 — Autopoietic lift

- Use the development tasks, split into train (the signal for self-modification) and validation (the signal for accepting a generation).
- Compare complete independent evolution runs against the frozen harness, and account for search cost in the budget.
- Hypotheses, in order, bounded by T7:
  1. **Harness self-modification:** the architect edits the coder harness (prompt templates, candidate generation and ranking, attempt history). This is where DGM found its gains.
  2. **Archive vs ratchet:** DGM-style parent selection from the lineage archive, against the current fitness ratchet.
  3. **Co-evolved evaluator:** RQGM-style, with the tester enzyme frozen per epoch and swapped only when it beats the incumbent on a separate labelled anchor set. Only after a fixed-oracle lift exists.

**Exit:** the best evolved harness and the frozen harness are both scored once on Confirmation-2, each against the stronger of B and D.

### P5 — Observer, offline, off the critical path

- Run `observer::observe` on Third Thoughts / Middens traces, using only features available at a prefix of the session. Validate on held-out projects and time periods.
- Pre-register: AUC of at least 0.65 and a net simulated quality/cost benefit of early kill.
- **Pass:** feed the observer real tool-event traces and run kill-and-respawn as a P3-style ablation.
- **Fail:** drop the "mechanical cortex" claim and leave the observer off.

## Completion

- **Success:** an A²D configuration, seed or evolved, is SUPPORTED against max(B, D) on a confirmation set, **and** in P4 the evolved harness is SUPPORTED against the frozen harness on Confirmation-2. Write it up in `examples/runs/` and `docs/solutions/`, then mark this plan **Completed**.
- **Lift without autopoiesis:** the seed or minimal configuration is SUPPORTED, but no P4 hypothesis is SUPPORTED within T7. Report it, then mark this plan **Completed (no autopoietic lift)**.
- **Falsified:** the confirmation results are RULED OUT for the seed or minimal configuration, and no P4 hypothesis is SUPPORTED within T7. Write a negative-result report with the evidence and a recommended pivot (for example, the research-instrument direction from the audit), then mark this plan **Completed (falsified)**. INCONCLUSIVE after the allowed extension is reported as inconclusive, not as falsified.
- **Not measured by deadline:** the P2 confirmation result is missing on 2026-11-30. Record why (build volume, blockers), with a recommendation, then mark this plan **Completed (not measured by deadline)**. A successor plan must explain how it avoids the same miss.

## Blockers

None open.

- **2026-10-14 — provider lanes (resolved by owner decision).** Kimi, Z.ai/GLM, MiniMax and Gemini subscriptions were stopped intentionally, so don't re-probe or plan around them. Perplexity's agent quota is also exhausted. Working lanes: Codex (`gpt-6.1-sol`, `gpt-6-luna`, `gpt-6-sol`, `gpt-6-astra`, `gpt-5.6-*`) and Claude.
- **Coder:** `codex/gpt-6-luna` (see Addendum).
- **Reviews:** the "≥2 non-Claude providers" rule cannot be met with one non-Claude family. Use Codex Sol plus a fresh-context Claude subagent, and record the reduced diversity.
- **Pi:** using Luna via Pi needs the `openai-codex` provider authenticated in `pi`. That's optional, because Codex already serves Luna.

## Appendix — goal prompt

The standing goal that drives this plan. If you change it, add a dated Addendum explaining why.

```text
A²D (skunkworks/a2d-autopoietic-autocatalysis-deutero): establish, with pre-registered controller-scored evidence, whether A²D builds working software more reliably than its own coder model used directly (one-shot, best-of-k, plain repair loop) at the same budget — and whether A²D's self-modification widens that margin.

Done only when docs/plans/2026-10-14-lift-first-roadmap.md is marked Completed in one of its outcomes (success / lift without autopoiesis / falsified / not measured by deadline), each backed by confirmation results in docs/experiments/LOG.md. Infrastructure is never "done" on its own.

Each session:
1. Read docs/HANDOFF.md and the roadmap. Run cargo test. Probe provider lanes (trivial prompt, short timeout).
2. Take the next experiment of the current phase. Pre-register it in LOG.md before any live run.
3. Make the smallest change that lets it run, tests first. Delegate bounded implementation to Codex. Run independent work in parallel subagents.
4. Run it through `a2d lift`, with durable artifacts under runs/. Record the result class with its CI. Negative and inconclusive results count.
5. Decide by the metric. Commit atomically with a [P<n>] tag. Overwrite HANDOFF (≤250 lines). Compound non-obvious learnings.

Rules:
- Never pause for direction. If blocked on something external (quota, credentials, access), log it under the roadmap's Blockers, switch to an unblocked track and keep going. Never build scaffolding around a blocker.
- Tripwires: a new LOG result at least every 2 sessions or 10 commits. No new redaction/sandbox/scrub work without an incident in a run artifact. Evidence must exercise the change. Budget caps on every run. At most 5 P4 hypotheses.
- Hidden tests never reach any arm or enzyme. Confirmation sets never reach you and are scored once.
- Phase gates and key designs get independent review: Codex (Sol) plus a fresh-context Claude subagent, and other model families if they become available. Run reviewers in an empty temp dir, never with the repo as cwd, and record who was consulted.
- Deutero: at each phase gate, compute process diagnostics (results per session, cost per result, tripwire hits), get them reviewed, and amend the roadmap with a dated Addendum. The loop that builds A²D is held to the same standard as A²D.
- Deadline: if the P2 seed result is not recorded by 2026-11-30, record it as not measured by deadline (a non-result, not a falsification) and complete per the roadmap.
```
