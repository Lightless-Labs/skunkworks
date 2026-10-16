# Plan: A² as a harness-evolution loop (autoresearch for coding-agent harnesses)

**Created:** 2026-10-15
**Reviewed:** 2026-10-15, by Codex `gpt-6-luna` (xhigh) and an independent Claude review, plus advisor review. Revision notes are in §11.
**Status:** Active
**Deadline:** a first promoted variant, or a written analysis, by 2026-12-15
**Inputs:**
- `docs/benchmark-results/external-value/e1/decision.md`, including its addenda;
- `docs/post-mortems/2026-10-15-a2-first-cycle.md`;
- `bench/e1/`.

## 1. Goal and done condition

A researcher agent proposes changes to a coding-agent harness policy, called the **genome**. A frozen **evaluator** scores each change on external held-out tasks, and the loop keeps or discards it.

The goal is met when one of these holds:
- **(a)** at least 20 experiments have been logged by the unattended loop, **and** at least one variant has been promoted under §7. Promotion means beating the E1 a2-full configuration on a sealed confirmation set, under `protocol-v2.toml`, which is committed before that set's first agent run.
- **(b)** if nothing is promoted by 2026-12-15, a written analysis exists of what the loop learned, plus the next genome to try.

| autoresearch | here |
|---|---|
| `prepare.py`, read-only | the frozen evaluator: `bench/e1/` plus `bench/evolve/` |
| `train.py`, edited by the agent | `genome/policy.py`, `genome/prompts/`, `genome/genome.toml` |
| `program.md`, written by a human | `bench/evolve/program.md`, frozen |
| fixed budget, `val_bpb` | E1's per-task ceilings, resolve rate against the champion |
| `results.tsv` | `docs/benchmark-results/evolve/log.jsonl` |

## 2. Architecture: the driver owns the episode, and the genome is a policy

```
loop.py (frozen)
 ├─ researcher: codex exec, own CODEX_HOME, Seatbelt allow-list; writes only ~/.a2-e1/genomes/<exp>/
 ├─ evaluate.py: genome × fixed task list → rows (shards, resume, infra retry as in E1)
 │    └─ episode.py, one task, a loop of at most 8 steps:
 │         state JSON ──stdin──▶ python3 policy.py (Seatbelt: no network, no writes,
 │                                reads only genome dir, Python runtime, and a pristine base snapshot)
 │         action JSON ◀─stdout─ {"agent": {...}} | {"submit": {...}}
 │         agent  → driver clones base, applies base_diff, commits it as the base, then runs
 │                  `a2ctl run --attempts 1 --lineage-db none` exactly as E1's stateless arm
 │                  (E1 agent sandbox, egress proxy, --deny-read private AND sealed);
 │                  observation = final message, diff, structured visible-verify result, tokens, wall time
 │         submit → grade once, with the hardened grader (§2.3)
 └─ decide (§5) → log → scoped commit
```

### 2.1 Actions, and why the budget matches by construction

`agent {base: "fresh" | <candidate index>, prompt, effort}` runs one `a2ctl run` call from the base, or from an earlier candidate's diff. The ceilings are E1 a2-full's:
- **at most 3 agent calls** per episode;
- **1500 s per call** (a2ctl `--timeout`);
- the same non-binding 2 M token flag;
- the same model, Codex `gpt-6-luna`;
- **effort ∈ {low, medium, high}**, never above E1's high.

The genome therefore never has more compute than the baseline, so any win comes from how it uses the calls. That is the harness question.

`submit {candidate}` names one earlier candidate's diff. Any failure (no submit, a policy crash, a policy step over 60 s of CPU, an invalid action) submits the last candidate, or the empty diff if there is none.

The genome controls:
- prompt construction;
- feedback formatting;
- whether to continue from a candidate or start fresh;
- planning and review calls;
- best-of-n with verifier selection;
- effort per call;
- which candidate to submit.

Context selection works through read access to the pristine base snapshot, which is agent-visible code anyway.

### 2.2 What the policy sees

- `problem_statement`;
- an **episode nonce**, never the task's `opaque_id`;
- the step history.

Observations are structured:
- the agent's final message, truncated by the driver to a fixed size;
- the diff and diff stat;
- visible-verify results as `{visible_test_index: pass | fail | error, message}`, with junit failure messages truncated;
- tokens, wall time and calls remaining.

Visible test ids are given by index and name. Hidden ids never appear.

### 2.3 Evaluator hardening: blockers, closed before any provider run, each with an exploit regression test

1. **Sandboxed pytest.** `/verify` and grading run under Seatbelt:
   - no network;
   - reads allowed only for the system and Python runtime, the task venv and the temp tree;
   - writes allowed only for the temp tree and a temp junit directory;
   - every other path under `~/.a2-e1` denied.

   This closes reads of gold, manifests, other tasks, logs and rows by code under test, and closes writes to them.
2. **Hidden tests stripped from the verify tree.** The visible-verify tree applies `test_patch` and then uses the AST to remove every test function or class whose node ids are all hidden. A task whose hidden ids cannot be separated (shared parametrized functions) uses visible-verify with those tests removed from reporting, and is flagged in the manifest. Final grading uses the full patch.
3. **No shadowing, no config.** A candidate diff is rejected, as `grade_rejected`, which counts as unresolved and not as an infrastructure failure, if it adds or modifies any of:
   - a module at a `sys.path` root named after a stdlib or installed distribution (`pytest`, `_pytest`, `py`, `sitecustomize`, `usercustomize`, …);
   - a `*.pth` file;
   - `pytest.ini`, `tox.ini`, `setup.cfg`, `pyproject.toml`, or `conftest.py` (`conftest.py` is also reverted).

   Pytest runs with `PYTEST_DISABLE_PLUGIN_AUTOLOAD=1`, `-c <frozen ini>` and an explicit `--rootdir`.
4. **Report sanity checks.**
   - Every required id must appear exactly once in junit.
   - A frozen pre-test conftest, injected from outside the tree, asserts that `pytest.__file__` lies inside the venv.
   - The diff audit flags any non-test code that references `_pytest`, `pytest`, `junit`, `TestReport`, `pytest_runtest`, or patches `sys.modules`. A flagged diff counts as `grade_rejected`.
5. **Integrity of the evaluator.** Each experiment row records `evaluator_sha256`, and the loop refuses to run if it differs from `bench/evolve/evaluator.lock`.

### 2.4 Sandboxes and Codex homes are allow-lists, one per role

| Role | Reads | Writes | Network | `CODEX_HOME` |
|---|---|---|---|---|
| policy | genome dir, Python runtime, base snapshot | none | none | — |
| dev agent | workspace, runtime | workspace, tmp | proxy only | `codex-home-agent` |
| confirmation agent | same as dev agent | same as dev agent | proxy only | `codex-home-confirm` |
| researcher | genome tree, `program.md`, log, digests | candidate dir | proxy only | `codex-home-researcher` |

All roles deny reads under `~/.a2-e1/private` and `~/.a2-e1/sealed`. Memories are disabled explicitly in config, not just by a comment. Session transcripts are purged after each episode.

### 2.5 Genome

- **G0** is stateless retry, re-expressed as a policy: up to 3 fresh `agent` calls at effort high, each prompt carrying the previous verify failures, submitting the first visibly passing candidate or else the last one.
- **Scope rule** (`program.md`): no task- or repo-specific content. It is enforced mechanically by an n-gram check that rejects a genome sharing an 8-word sequence with any dev problem statement or digest.
- `program.md` states the noise floor (§4) and the smallest detectable effect. It asks for **structural** hypotheses, not prompt polish:
  - verifier-selected best-of-n;
  - plan-then-execute;
  - test-writing first;
  - review-and-repair;
  - continue vs restart;
  - effort allocation.

## 3. Task pools

| Pool | Content | Use |
|---|---|---|
| **dev** | The 44 gated ansible tasks, all seen in E1 | Screening and the noise floor. The researcher sees digests keyed by a hash that changes nightly: problem statement, final agent message, structured verify result, diff stat, resolved y/n. Never gold or hidden ids. |
| **sealed** | openlibrary and qutebrowser, the tasks with \|F2P\| ≥ 2 (62 and 63) | Confirmation only. Stored under `~/.a2-e1/sealed/` and denied to every role. |

**Feasibility, probed 2026-10-15** from 2 base commits per repo, with no task contents read:
- **openlibrary** needs the `vendor/infogami` submodule at its pinned commit. It needs psycopg2 2.9.6, which builds here because `pg_config` is present, and web.py from git.
- **qutebrowser** pins PyQt5 5.12–5.13, which has no arm64 wheels. PyQt5 5.15.11 plus PyQtWebEngine 5.15.7 install and run offscreen, so the unpinned fallback is constrained to `PyQt5<5.16`.
- Validity is decided per task by the gold/empty gate.

## 4. Noise floor and cost

Costs are E1 **means** per task, not medians:

| Arm | tokens | wall time |
|---|---|---|
| raw | 0.42 M | 299 s |
| stateless retry | 0.95 M | 743 s |
| a2-full | 0.93 M | 589 s |

1. **Noise floor.** Run G0 twice on all 44 dev tasks, then a third time on tasks that were not 0/2 or 2/2. That is about 105 episodes, roughly 100 M tokens and about 6 h on 4 shards. Record the per-task p̂ₜ, and measure the ChatGPT-plan throttling behaviour along the way.
2. **Fixed screen set S**, declared once after the noise floor and never rotated:
   - every contested task (0 < p̂ < 1);
   - 3 always-solved sentinels, so that regressions count;
   - a fixed hard subset of 4 never-solved tasks.

   Expected size: about 18–20.
3. **Throughput.**
   - A screen is about 19 episodes, roughly 18 M tokens and about 1 h on 4 shards, including the 25-min tail.
   - The nightly champion control is one more screen.
   - Expect **5–7 experiments per night**, so 20 experiments take about 4 nights and roughly 500 M tokens.
   - If throttling appears, reduce the per-night target rather than shrinking S.
4. **Early abort.** A screen stops when even resolving every remaining task could not reach the stage-A bar.

## 5. Keep/discard rule (pre-stated; changes only by dated addendum)

- **Current-champion baseline.** p̂ₜ comes **only from fresh runs of the current champion**: its noise-floor runs, its stage-B runs, and a **nightly concurrent control run on S**. On a keep, the old p̂ is discarded.
- **Stage A (screen).** The candidate runs once on S. It proceeds if Δ_A = Σ(rₜ − p̂ₜ) ≥ q₀.₇₀, the 70th percentile of the null distribution. The null is drawn by Monte Carlo: candidate runs from Bernoulli(p̂ₜ), with p̂ resampled from the champion runs to carry its estimation error. Stage A only screens; its draw is never reused.
- **Stage B.** Two fresh candidate runs on S. Δ_B is the mean over those two runs. The candidate is kept if:
  - the Monte Carlo one-sided p ≤ 0.05 against the same null; **and**
  - one run on all 44 dev tasks shows no net regression beyond the null's 10th percentile.

  The no-regression clause stops a candidate from trading easy tasks for contested ones.
- **Metrics.** Resolved is the decision metric. Partial credit (`required_passed / required`) is logged as a diagnostic.
- **Multiplicity.** Across about 20 experiments, a per-experiment false keep below 5% means at most about one spurious keep. The sealed confirmation (§7) is the only promotion claim, and it is a single test.
- **Calibration before use.** The stage-A and stage-B thresholds are computed from the noise-floor data and committed, together with simulated false-keep and power numbers, in `docs/benchmark-results/evolve/noise-floor.md`. This happens **before** the first candidate is evaluated.

## 6. Experiment log

`docs/benchmark-results/evolve/log.jsonl` holds one row per experiment:
- `exp_id` and `parent`;
- `hypothesis`;
- `genome_diff_sha256`, plus the diff at `evolve-archive/<exp_id>.patch`;
- `evaluator_sha256`;
- `rows_sha256`, with the per-task rows kept private;
- `delta_A`, `delta_B`, `p_B`, `full44_delta`, `tokens` and `wall_secs`;
- `decision` (keep / discard / invalid) and `reason`.

After each experiment the loop makes one scoped commit: `log.jsonl`, the archive patch, and `genome/` if the candidate was kept. Push is best-effort: on failure the loop continues, and the next session pushes.

## 7. Confirmation (one test, pre-registered)

`bench/external-value/protocol-v2.toml` is committed **before the sealed gate runs.** It fixes:
- the selection rule and seed. It fixes a rule, not a list, so gate yield cannot force an edit.
- 5 sealed tasks reserved for raw calibration, under a pre-stated floor/ceiling rule as in E1;
- arms:
  - **primary:** the champion genome vs E1 a2-full, run exactly as in E1 through `a2ctl` with identical ceilings and effort high;
  - **secondary:** the champion vs G0;
- per-task arm order seed, the infrastructure rules from E1, and separate `CODEX_HOME`s.

The sample is **every gated sealed task up to 100**, stratified by repo. There is **one confirmation run**, scheduled 2026-12-01 to 12-10 with the champion at that time. The test is an exact one-sided McNemar; **promote iff p ≤ 0.05.**

Power at a 4% baseline-only win rate:

| n | true gain | power |
|---|---|---|
| 80 | +10 pp | 0.61 |
| 80 | +15 pp | 0.87 |
| 100 | +10 pp | 0.72 |
| 100 | +15 pp | 0.93 |
| 48 | +10 pp | 0.35 |

At n = 48 the confirmation is too weak, which is why the plan uses one large test instead of two small ones. A failed confirmation is reported with this power table, never as "no effect".

## 8. Researcher

Codex `gpt-6-luna` at xhigh, in its own `CODEX_HOME` with skills off, a "direct task" preamble and the Seatbelt allow-list from §2.4.

Inputs:
- `program.md`;
- the champion genome;
- the full log, including discarded diffs and their deltas;
- digests of the champion's latest dev episodes.

Output: a candidate directory plus a one-line hypothesis.

A candidate is marked **invalid** before any task spend if it:
- is unchanged;
- fails the n-gram check;
- fails a policy self-test, which replays two recorded states and must return valid actions within the CPU limit;
- fails a 1-task smoke run.

The first loop experiment is G0 at effort medium vs high, moved here from the noise floor.

## 9. Work order

| # | Work | Exit |
|---|---|---|
| 1 | This plan, reviewed and revised | done (this revision) |
| 2 | Grader hardening, §2.3 items 1–4: sandboxed pytest, AST stripping, rejection rules, report checks. Exploit regression tests: shadowed `pytest`, `sitecustomize`, an `addopts` plugin, a junit forger, and code under test reading `manifest.json` or `gold.patch` or printing hidden test source. | All exploits fail closed; E1 gold/empty gate re-run still valid on the 44 dev tasks |
| 3 | `episode.py` plus the policy sandbox, G0, the per-role `CODEX_HOME`s and the sealed deny. A 1-task smoke run. | Graded row |
| 4 | `evaluate.py` (shards, resume, infra retry), the log, and `evaluator.lock` | 3-task dry run |
| 5 | Noise floor; S declared; thresholds plus a simulated false-keep rate committed | `noise-floor.md` |
| 6 | `loop.py`, the researcher and `program.md`. Two attended experiments, then unattended nights. | ≥ 20 logged |
| 7 | `protocol-v2.toml`, then the sealed gate (openlibrary and qutebrowser profiles), calibration, and confirmation (12-01 to 12-10). Gating spends no tokens and can run alongside steps 5–6. | Promotion, or the analysis in §1(b) |

**Cut line.** If step 3 does not produce a graded row within two working sessions, write a blocker note and fall back to a declarative v0 genome (`genome.toml` plus prompts, interpreted by a frozen runner).

## 10. Risks

| Risk | Mitigation |
|---|---|
| ChatGPT-plan throttling at about 130 M tokens per night | Measured during the noise floor; lower the nightly experiment count; `provider_error` back-off |
| Too noisy to tell variants apart | Thresholds come from simulation; `program.md` steers toward structural changes. Two nights without a keep or any informative discard trigger a blocker note and a design change, for example a larger S or more runs per stage. |
| Overfitting to ansible | Nonces, the n-gram check, the no-regression clause, and sealed repos that differ from dev |
| Grader exploits the plan has not foreseen | §2.3 checks, an LLM audit of the champion's resolved diffs before confirmation, and the sealed run uses the same hardened grader |
| Disk (27 GiB free) | Prepare and evict per task; at most 3 candidate workspaces per episode |

## 11. Revision notes (2026-10-15)

| Finding | Source | Change |
|---|---|---|
| Unsandboxed grading, shadow modules and config plugins, writes to evaluator state | both | §2.3 |
| Oracle paths and budgets (handles, races, overshoot) | both | Removed: the driver owns the episode (§2), on the advisor's suggestion |
| Hidden test bodies reachable from verify | Claude, Codex | AST stripping and structured results (§2.2, §2.3) |
| `opaque_id` enables lookup tables | Claude | Nonces, nightly-rehashed digests, n-gram check |
| Keep rule (selected draws reused, σ with 2 degrees of freedom, p̂ selection bias, stale-p̂ ratchet, never-solved tasks only adding, easy-task trade-offs) | both | §5 rewritten: fresh runs, Monte Carlo null, nightly control, sentinels, fixed S, full-44 no-regression |
| Confirmation underpowered, multiplicity | both | One test on up to 100 tasks, with a power table (§7) |
| Budget mismatch with a2-full | both | E1 ceilings applied by construction (§2.1) |
| Deny-list sandboxes, shared `CODEX_HOME`, sealed readable by agents | Claude | §2.4 |
| Costs from medians | Codex, Claude | Means, 5–7 experiments per night (§4) |
| Effort comparison was a non-test | Claude | Moved to the first loop experiment (§8) |
| Primary comparator should be G0 | Claude | **Not adopted.** The goal fixes a2-full as primary; G0 is secondary. |
| Hidden screening slice, separate from researcher-visible dev | Codex | **Deferred.** Splitting the ~18 contested tasks would leave the screen too noisy. Sealed repos differ from dev, so confirmation is the generalization check. |

**Addendum:** 2026-10-16 — noise floor done; S uses 12 hard tasks (before any candidate).

G0 is close to deterministic: 25 tasks were never solved, 14 always solved and only 4 were contested. With the plan's 4 hard tasks, simulated power was 0.26 at a logit shift of +1.0. With 12 it is 0.45, at the same false-keep rate of about 2% and |S| = 19. S and the thresholds are fixed in `docs/benchmark-results/evolve/noise-floor.md`: stage A Δ ≥ 2.0; stage B Δ ≥ 4.0 (p ≤ 0.05) plus full-pool Δ ≥ −1.0.

The sealed pool is openlibrary only (`docs/blockers/2026-10-16-qutebrowser-native-gate.md`). With about 41 tasks, power is 0.29 at +10 pp and 0.53 at +15 pp.

**Addendum:** 2026-10-16 — confirmation runner (`bench/confirm/`) and pre-confirmation checklist.

The confirmation code lives outside `bench/evolve/` so that the running loop's lock does not change. Before the first sealed agent run, all of the following must be done:
1. `bench/evolve/episode.py` honours a `cfg.deny_read` override in `_run_agent_call`, so that confirmation agents are also denied `~/.a2-e1/private`.
2. `bench/confirm/` joins `lock.FROZEN`; re-lock.
3. Record the champion's commit and genome sha256 in the protocol-v2 addendum.
4. Run the raw calibration (`--calibration`), then the run itself.
