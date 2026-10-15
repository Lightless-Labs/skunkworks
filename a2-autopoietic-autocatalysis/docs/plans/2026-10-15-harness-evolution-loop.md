# Plan: A² as a harness-evolution loop (autoresearch for coding-agent harnesses)

**Created:** 2026-10-15
**Status:** Draft, pending multi-model review
**Deadline:** a first promoted variant, or a written analysis, by 2026-12-15
**Inputs:**
- `docs/benchmark-results/external-value/e1/decision.md`, including its addendum;
- `docs/post-mortems/2026-10-15-a2-first-cycle.md` (§6 findings, §7 design rules);
- the E1 apparatus in `bench/e1/`.

## 1. Goal and done condition

A² becomes a loop in which a researcher agent proposes changes to a coding-agent harness, called the **genome**. A frozen **evaluator** scores each change on held-out external tasks, and the loop keeps or discards it.

The goal is met when one of these holds:
- **(a)** at least 20 experiments have been logged by the loop running unattended, **and** at least one variant has been promoted under §7. Promotion means it beat the E1 a2-full configuration on a sealed confirmation set, under a protocol committed before that set's first agent run.
- **(b)** if no variant is promoted by 2026-12-15, a written analysis exists of what the loop learned, plus the next genome to try.

This mirrors autoresearch:

| autoresearch | here |
|---|---|
| `prepare.py`, read-only to the agent | the frozen evaluator: `bench/e1/` plus `bench/evolve/` |
| `train.py`, which the agent edits | the genome: `genome/` |
| `program.md`, written by a human | `bench/evolve/program.md`, frozen |
| fixed 5-minute budget, `val_bpb` | a fixed per-task budget, paired resolve rate against the champion |
| `results.tsv` | the experiment log |

## 2. Architecture

```
loop.py (frozen, unsandboxed orchestrator)
 ├─ researcher: codex exec under Seatbelt; writes only ~/.a2-e1/genomes/<exp>/
 │     reads: champion genome, program.md, experiment log, dev digests
 ├─ evaluate.py: genome × task list → rows   (shards; resumable)
 │     └─ episode.py, one task:
 │          oracle (localhost HTTP, unsandboxed, frozen): /agent /verify /workspace /submit /budget
 │          harness: python3 genome/harness.py under Seatbelt
 │                   (network: oracle port only; reads denied under ~/.a2-e1/private and sealed/)
 │          /agent → codex exec under the E1 agent profile (egress proxy, deny private)
 │          /submit → grade once, hidden + visible, with bench/e1 grading
 └─ decide: paired comparison vs champion → keep/discard → log → commit
```

### 2.1 Frozen evaluator (the researcher has no write access)

- **Reused from E1:** `bench/e1/{common,grading,tasks,verify,egress_proxy}.py`, the Codex home isolation, and the native grading rules.
- **New in `bench/evolve/`:**
  - `oracle.py`
  - `episode.py`
  - `evaluate.py`
  - `loop.py`
  - `stats.py`
  - `confirm.py`
  - `sandbox.py`, the Python Seatbelt profile builder. It carries the same rules as the E1 profile, plus a localhost-only profile for the harness.
  - `program.md`
- **Integrity:** every experiment row records `evaluator_sha256`, a hash over the frozen file list. The loop refuses to run if that hash differs from the value committed in `bench/evolve/evaluator.lock`.

### 2.2 Oracle API (the genome's only privileged channel)

All metering happens here, outside the harness sandbox.

| Endpoint | Effect |
|---|---|
| `POST /workspace/new` | A fresh APFS clone of the history-free base. Returns its path. At most 4 per episode. |
| `POST /agent {workspace, prompt, effort}` | Runs `codex exec gpt-6-luna` in that workspace under the agent profile. Returns the final message, a JSONL event summary, the diff and token usage. At most 2 concurrent. |
| `POST /verify {workspace}` | Runs the visible verifier, with the same text as `bench/e1/verify.py` output. At most 6 per episode. |
| `GET /budget` | Tokens, wall time and calls remaining. |
| `POST /submit {workspace}` | Final answer. Grades once, then the episode ends. With no submit, the last workspace touched is graded. |

The leak guard applies to every response, using the whole-id matcher. It is unit-tested on the recorded E1 verifier outputs before any run (post-mortem §7.6).

### 2.3 Fixed per-task budget (identical for every genome and for the baseline)

- **Tokens:** 2.0 M (input + output, from Codex usage events). The oracle refuses `/agent` once the budget is exhausted.
- **Wall time:** 45 min per episode, after which the harness is killed and the last workspace is graded.
- **Model:** fixed at Codex `gpt-6-luna`. Reasoning effort is a genome choice.

### 2.4 Genome (`genome/`, tracked in git; the current champion lives on `main`)

- `harness.py`: `solve(task: dict, oracle: Oracle) -> None`, where `task` is `{problem_statement, opaque_id}`. It can read the repository in its workspace and run local commands, all sandboxed.
- `prompts/*.md` and `genome.toml`.
- A small frozen client, `bench/evolve/oracle_client.py`, is importable but not editable.
- **Seed genome G0:** stateless retry with visible feedback, re-implemented through the oracle. It is the strongest simple loop in E1.
- **Scope rule (in `program.md`):** no task-specific content (repo-specific hints count as task-specific), and no attempt to read evaluator state. The sealed set is the backstop for overfitting.

## 3. Task pools

| Pool | Tasks | Use |
|---|---|---|
| **dev** | The 44 gated ansible tasks: 5 calibration, 36 sample, 3 reserve. All have been seen in E1. | Screening and noise-floor measurement. The researcher sees digests: problem statement, final agent message, visible verifier output, diff stat, resolved y/n. It never sees gold patches or hidden test ids. |
| **sealed** | openlibrary (62 tasks with \|F2P\| ≥ 2) and qutebrowser (63), gated with the E1 gold/empty validity rule. | Confirmation only (§7). Stored under `~/.a2-e1/sealed/`. Unreadable by the researcher and harness sandboxes, and never touched by `loop.py`. |

Using different repos for sealed and dev confirms that an improvement generalizes rather than fitting ansible. If gating yields fewer than 40 sealed tasks, add Go repos (flipt, vuls), provided a native Go toolchain gates cleanly.

## 4. Cost and noise (step 4 of the goal)

E1 costs per (task, arm), as medians:

| E1 arm | wall time | tokens |
|---|---|---|
| raw | 206 s | 0.32 M |
| stateless retry | 603 s | 0.78 M |
| a2-full | 444 s | 0.66 M |

Of the 36 E1 tasks, 23 were never solved by any arm and 4 were solved by every arm.

1. **Noise floor.** Run G0 three times on all 44 dev tasks: 132 episodes, about 100 M tokens, about 5.5 h on 4 shards. The output is a per-task solve probability p̂ₜ for the champion, and the run-to-run standard deviation σ of its total.
2. **Contested screen set S.** Dev tasks with 0 < p̂ₜ < 1, plus 4 never-solved tasks that rotate each night, so that cracking hard tasks still counts. The expected size is about 12–16 tasks.
3. **Cheaper episodes.** Measure G0 at `effort=medium` against `high` on S during the noise-floor run. If medium is within noise, the seed genome uses medium. The effort choice belongs to the genome, so this is a seed decision, not an evaluator change.
4. **Shards.** 4 shards, chosen for 6 CPUs and 11 GiB RAM. Verification is CPU-bound and the agent calls are network-bound. If no OOM appears, try 5.

Target: one screen of 12–16 tasks in about 45 min, so roughly 10 experiments per night.

## 5. Keep/discard rule (pre-stated; tunable only by dated addendum)

- **Screen (stage A).** The candidate runs once on S. Its score is Δ = Σₜ (rₜ − p̂ₜ), where rₜ is 1 if task t resolved and 0 otherwise. Stage B runs if Δ ≥ +1.5.
- **Stage B.** A second candidate run on S, plus the 4 rotating hard tasks. The candidate is **kept**, becoming the new champion, if the mean Δ over the two runs is at least max(+1.5, 2σ/√2). Otherwise it is discarded.
- **Champion re-runs.** A new champion is re-run once on S to refresh p̂ₜ (a running mean). The noise estimate therefore improves over time.
- **Discards are logged in full.** Their diffs stay in the archive, and the researcher sees them in the log.

## 6. Experiment log

`docs/benchmark-results/evolve/log.jsonl` holds one row per experiment:
- `exp_id` and `parent`;
- `genome_diff_sha256` and the diff path in `evolve-archive/<exp_id>.patch`;
- `hypothesis`, the researcher's one-line statement;
- `evaluator_sha256`;
- per-task rows, stored privately, with their `rows_sha256` in the log row;
- `screen_delta`, `stageB_delta`, `tokens` and `wall_secs`;
- `decision` (keep / discard / invalid) and `reason`.

The loop commits the log row and the archive patch, plus `genome/` if the candidate was kept, as one scoped commit per experiment. It pushes on a best-effort basis: if a push fails, it continues, and the next session pushes.

## 7. Confirmation on the sealed set (promotion)

`bench/external-value/protocol-v2.toml` is committed **before the sealed gate runs.** It fixes:
- the repos and the selection seed;
- the sample size: up to 48 tasks, with 4 reserves;
- the arms: the champion genome against the E1 a2-full configuration (`a2ctl run` exactly as in E1, same budget class);
- the per-task arm order seed;
- infrastructure rules as in E1;
- statistics: an exact one-sided McNemar test.

**Promote iff** diff ≥ +10 pp **and** p ≤ 0.05. There are at most **two** confirmation runs before 2026-12-15 (Bonferroni across two runs at overall α = 0.10). A failed confirmation is logged, and the loop continues on dev. The sealed set is never used for screening.

## 8. Researcher

Codex `gpt-6-luna` at `effort=xhigh`, using the same isolated `CODEX_HOME` and egress proxy as E1. It runs under Seatbelt with:
- write access only to the candidate directory;
- reads denied for `~/.a2-e1/private` and `~/.a2-e1/sealed`.

It works under an explicit "no skills, direct task" instruction. Inputs:
- `program.md`;
- the champion genome;
- the last N log rows, with discarded diffs;
- dev digests for the champion's latest episodes.

Output: one candidate genome directory plus a hypothesis line.

A candidate is marked `invalid` and costs nothing further if it:
- fails a 1-task smoke run;
- imports anything outside the standard library and `oracle_client`;
- does not change the genome.

## 9. Work order

| # | Work | Exit |
|---|---|---|
| 1 | This plan, multi-model reviewed (Codex plus an independent Claude review), with revisions recorded | `Reviewed:` header |
| 2 | `sandbox.py` profiles plus boundary tests: the harness profile reaches only the oracle port, and private and sealed reads fail | Tests green, boundary proven from inside the sandbox |
| 3 | `oracle.py`, `episode.py` and the seed genome G0. A 1-task smoke run on a dev task. | Graded row produced |
| 4 | `evaluate.py` (shards, resume, infra retry as in E1) and the experiment log | 3-task dry run |
| 5 | Noise floor: G0 × 44 × 3, plus the effort comparison. Fix S, σ and the seed effort. | `noise-floor.md` committed |
| 6 | `protocol-v2.toml` committed, then the sealed gate (openlibrary and qutebrowser) | ≥ 40 sealed tasks, or a blocker note plus the Go pool |
| 7 | `loop.py` and the researcher. Two attended experiments, then unattended overnight runs. | ≥ 20 logged experiments |
| 8 | Confirmation run(s) under protocol-v2 | Promotion, or analysis per §1(b) |

Steps 2–4 and step 6 are independent and can run in parallel. Delegate implementation to Codex, and review each piece before it lands.

## 10. Risks

| Risk | Mitigation |
|---|---|
| ChatGPT-plan rate limits at about 100 M tokens per night | Track `provider_error` rows, add back-off in `evaluate.py`, shrink S |
| Overfitting to ansible dev | Sealed repos differ; the scope rule in `program.md`; confirmation is capped at two runs |
| Too noisy to tell variants apart | Stage B and champion re-runs; if σ swamps every plausible Δ, change S's composition. A blocker note comes after two nights without movement. |
| Researcher stagnation (small prompt tweaks) | `program.md` asks for structural hypotheses, e.g. best-of-n with verifier selection, planning passes, test-writing, context selection, and requires reading the discard history |
| Disk (27 GiB free) | Per-task prepare and evict as in E1; at most 4 workspaces per episode |
| Sandbox gap in the new Python profiles | Boundary tests run from inside each profile before any provider call (step 2) |
