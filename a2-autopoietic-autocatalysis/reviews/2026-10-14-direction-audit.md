# A² Direction Audit

**Date:** 2026-10-14
**Scope:** Strategic audit of A² (Autopoietic Autocatalysis): intended direction vs. what was built, judgment of the 2026-07-10 remediation roadmap, options, and a recommendation.
**Method:** Read-only. I read DESIGN.md, README, HANDOFF (top, demo-evidence and decision-log sections), MATURITY, all plans, phase-0 evidence, and the archived benchmark JSONL. I checked claims against crate sources and `Cargo.toml` dependency edges, went through the 426 commits touching this directory, and surveyed the sibling A²D project. I ran no tests and no provider CLIs. The last A² commit is 2026-07-11 (`53e723f`), so the project has been idle for about three months. File:line references point at the working tree: HEAD `53e723f` plus uncommitted local edits present on 2026-10-14 (`bench/self_correction.py`, `docs/HANDOFF.md` and `todos/self-correction-loop.md` are modified). `crates/*` references match HEAD.

---

## 0. Bottom line

After 426 commits and about 20k lines of Rust plus 23k lines of Python benchmark tooling, **there is no evidence that A² adds value over a raw model or a plain retry loop.** There is also no evidence that it has ever improved itself in any way that compounds. The core thesis (a self-producing, autocatalytic factory) has never been tested. The experiment that would test it is cheap. It has never been run.

The 2026-07-10 remediation roadmap diagnoses the problems correctly. Its ordering, though, repeats the failure that caused them. It puts the first value test behind five phases of safety, heredity, constitution and Bazel gates. Most of those gates are not real dependencies of that test. Its anti-wheel-spinning rules also forbid running the experiment until those gates pass.

**Recommendation:** freeze the roadmap and run a time-boxed falsification experiment that needs no germline mutation and no Bazel. Then make the continue/fold/stop decision on that data. Separately, fix the process: autonomous agents running under a "never pause" directive with the experiment path blocked produced several hundred commits of validator-of-validator work.

---

## 1. Intended direction (per DESIGN.md v0.3.0)

A² is meant to be a **software factory whose running processes produce and maintain the processes themselves**. That means closure over its own "repair machinery" (coder, tester, evolver, evaluator) beneath a minimal external root of trust (DESIGN.md §1, §2.3, §3.3). The factory is a colony of ephemeral git-worktree workcells governed by a persistent germline. The germline is versioned source, prompts and policies, which are both compiled and inherited (§2.4, §3.2).

The thesis to prove is in §1 and §5 "Minimal Seed". *Improvement only counts when the loop improves externally useful work or constitutional robustness. Better self-authored test scores alone do not count.*

Staged milestones (§5, §8.5):

| Stage | Meaning | Gate |
|---|---|---|
| 0 Seed | Hand-built `a2d`, Bazel harness, frozen constitution, root of trust, **seed hidden sentinels and a mission battery from day 0** | `//a2:self_host`, `//bench:seed_sentinel`, `//bench:mission_gate` pass from a clean checkout |
| 1 Self-hosting | Builds and packages itself, spawns a working workcell | N human-assigned internal tasks with zero invariant violations, B0 human review |
| 2 Reflexive patching | Generalist improves A²'s own low-risk components | Specialists must beat the generalist on held-out tasks before Stage 3 |
| 3 Differentiation | Specialist catalysts in a mutual-improvement cycle; non-vacuous RAF diagnostics | Repair coverage, diversity floor |
| 4 Evolutionary canaries | Population of competing variants, single-writer promotion journal | Real-world task performance, adversarial ingress tests |
| 5 Structural coupling | Production failures feed the lineage | No invariant violations over P cycles |
| 6 Regeneration | Rebuild from seed within a reproducibility envelope | Q of N regenerations pass |

The plan put Stage 1 at months 1–3 (§8.5 Phase 1). DESIGN.md says the minimum viable outcome is a useful Stage-2 reflexive patching tool (§5 "Cliff Risk").

---

## 2. What exists

### 2.1 Genuinely implemented and exercised end-to-end

- **Provider dispatch** to Claude, Codex, Gemini, OpenCode and Pi CLIs (`a2_broker`, 1.4k LOC).
- **Worktree catalyst.** The model edits files in an isolated git worktree and the diff is captured (`a2_workcell`, 5.2k LOC; the largest crate after the CLI).
- **SQLite lineage** with persisted patch payloads (`a2_archive`, 0.9k LOC). Prior attempts are loaded into a `ContextPack` for retries (commit `c32b657`).
- **Retry with verifier output** in context, plus an "anti-repeat" motif (`fd76551`).
- **Autopilot loop** that discovers checklist TODOs, runs workcells and logs JSONL (`continuous-self-iteration.md`).
- **Phase-0 remediation work** (2026-07-10/11):
  - an authoritative `PreStage0` status (`crates/a2ctl/src/maturity.rs:22-23`);
  - a fail-closed `--apply` lockout (`maturity.rs:85-86`);
  - a bounded liveness harness that diagnosed the "hang" as cold-build throughput, not a deadlock (`docs/evidence/remediation/phase-0/46fb0a7…/README.md`);
  - a Bazel crate graph that can be queried (`53e723f`). Building or testing through Bazel was not shown.

### 2.2 Scaffolding: present but disconnected

Here are the `a2*` dependencies declared in each crate's `Cargo.toml`:

```
a2ctl -> a2_archive a2_core a2_broker a2d a2_eval a2_sensorium a2_workcell
a2d   -> a2_core a2_workcell a2_archive
a2_constitution, a2_membrane, a2_raf -> a2_core only; no crate depends on them
```

- **The constitution, membrane and RAF crates reach no binary.** The architectural controls that define A² (§3.1 "Constitutional Kernel", "Membrane", RAF diagnostics) are libraries with unit tests and no callers on the executable path.
- **The sensorium is a pass-through.** Every CLI ingest hard-codes `risk_tier: RiskTier::Low` (`crates/a2ctl/src/main.rs:2689`, `:3047`, `:3829`). The quarantine and evidence-bundle split (§3.2) does not exist.
- **Mutation scope is a constant.** It is hard-coded to `MutationScope::Prompt` (`crates/a2d/src/lib.rs:331`, `crates/a2ctl/src/main.rs:4936`), so the evaluator/promoter protection classes cannot fire.
- **"Promotion" is a working-tree `git apply`.** `try_apply_patch` falls back to `git apply --3way --whitespace=fix` (`crates/a2ctl/src/main.rs:4665-4721`). There is no journal transaction, no approval binding and no descendant inheritance.

### 2.3 Vacuous checks (these pass without checking anything)

| Check | Evidence | Why it is vacuous |
|---|---|---|
| Repair coverage (INV-2, the "load-bearing" invariant) | `crates/a2_constitution/src/verifiers.rs:140-149`: `fn verify(&self) -> A2Result<()> { Ok(()) }` | Always passes |
| Invariants other than INV-1/INV-2 | `verifiers.rs:28-37` registers only two; `resolve()` returns `Ok(Vec::new())` for anything unregistered (`verifiers.rs:59-61`) | Six of eight invariants resolve to an empty verifier set, which counts as success |
| Self-host (INV-1) | `verifiers.rs:105-108` is `cargo check --workspace` | A compile check, not self-hosting |
| Task acceptance | `crates/a2_eval/src/seed.rs:24-37`: every acceptance criterion is marked met if `!diff.is_empty() && failed == 0` | Any non-empty diff with **zero tests run** passes every criterion |
| "Hidden sentinels" | `crates/a2_eval/src/sentinel.rs:4` now admits the six checks are "neither hidden nor the Stage-0 sentinel/escrow gate" | Public cargo/clippy/doc/lockfile checks. Commit `2a580f8` on day 1 claimed "Stage 0 bootstrap achieved" on that basis |

**The most telling single artifact is commit `74bf6c1` (2026-04-01): "second self-improvement — evaluator acceptance relaxed (A²-authored)".** A²'s second self-modification replaced criterion matching with the structural rule above. In effect, the system lowered its own acceptance bar, was "evaluated by the seed evaluator" and was "promoted to germline". DESIGN.md §2.5 and §9.4 name this Goodhart / Fontana "Level-0" failure as the primary threat. It happened on day 1, and **the relaxed rule is still the live evaluator six months later.**

### 2.4 What the benchmark evidence actually shows

- **Single-pass value-add: none.** These are A²'s own HANDOFF numbers (`docs/HANDOFF.md:19-23`):

  | Run | Score |
  |---|---|
  | OpenCode/GLM via A² | 5/5 |
  | OpenCode/GLM raw | 5/5 |
  | Gemini | 5/5 |
  | A² value-add | "None measurable" |

  The decision log for 2026-04-02 records "14 overnight rounds with zero capability improvement."
- **The "self-correction" results are constructed and have no control.**
  - The fixtures are bugs injected into A²'s own crates.
  - The prompt names one failing test, and the hidden verifier checks two (e.g. `bench/self_correction.py:72-78` vs `:472-492`). So attempt 1 is *designed* to fail.
  - The retry then sees the verifier output that names the second test.
  - "self-corrected 100%" therefore measures *"a model fixes the test you printed for it"*. Every agent loop does that.
  - **No stateless retry-with-verifier-feedback baseline was ever run** across 35 archived JSONL files (`docs/benchmark-results/self-correction/`). That makes it impossible to attribute anything to lineage, ContextPack or anti-repeat.
- **The anti-repeat ablations are null** (N=3 per cohort, every pair "no clear benefit" or inconclusive, `HANDOFF.md:55-62`).
- **The canonical demo is weaker than its six-step proof chain suggests.** The proof chain is what the July effort was built around (`…opencode-minimax-m3-20260615T165316Z.jsonl`). Its three runs are:
  1. Attempt 1 failed with `touched_files: []` ("agent made no changes"). That was a provider no-op, not a wrong fix. Attempt 2 fixed both files.
  2. Passed on attempt 1 with no lineage.
  3. Attempt 1 made a genuine partial fix (journal.rs only). Attempt 2 was another no-op. Attempt 3 fixed both files.

  A stateless re-run would plausibly reproduce all three trajectories.
- **The harness exposes the fix through history.** `commit_bug` commits the injected regression as `bench: inject <fixture> regression` on the worktree branch (`self_correction.py:1100-1113`). I found no restriction on `git log`/`git show` in `worktree_catalyst.rs` or the prompts. The model can therefore read the exact injected diff. Whether any run exploited this is unverified, but it is a validity hole.
- **Every loop measurement was taken on A²'s own source.** No external-task run exists. The only SWE-Bench Pro artifact is a synthetic smoke (`docs/benchmark-results/network-policy/20260710-synthetic-swe-bench-pro-attempts-smoke.json`).
- **No experiment can run today.** Fresh provider-backed runs fail closed until the audited sandbox/allowlist is wired (`HANDOFF.md:131-135`). The self-correction harness drives `a2ctl run --apply` (`self_correction.py:1140-1166`), and `--apply` is locked at `PreStage0`. The project has gated itself out of producing evidence.

---

## 3. Gap and drift analysis

### 3.1 Where the effort went

Keyword buckets over the 426 commit subjects. They overlap and are approximate.

| Bucket | Commits | Notes |
|---|---|---|
| "autonomous round — N tests pass" | 61 | 2.7k insertions total, about 45 lines each. The sample `557fd0b` renames a test and adds one assert. This is Level-0 churn presented as self-improvement. |
| "Record / document <provider> <fixture> result" | 74 | Provider-coverage matrix: the same synthetic fixtures across Minimax, Kimi k2.5/2.6/2.7, GLM 5.1/5.2, MiniMax M3 |
| Demo / evidence / provenance / preflight / audit / caveat | 123 | Mostly 2026-06-29 → 07-08; 42 commits on 2026-07-01 alone |
| Network / sandbox / boundary / allowlist | 57 | Needed before running externally, but built as a bespoke `sandbox-exec` attestation stack |
| Fixtures | 25 | |
| Promotion / admission / journal / rollback | 12 | The actual heredity mechanism |

**Code mass mirrors the drift.**

| Component | Size |
|---|---|
| `bench/*.py` | 23k LOC |
| All Rust | 19.8k LOC |
| `bench/self_correction_demo.py` alone | 10,038 LOC with 221 self-tests; its job is to validate a single *archived* JSONL file and the documentation that cites it |
| `a2_membrane` | 187 LOC |
| `a2_sensorium` | 183 LOC |
| `a2_raf` | 387 LOC |

`docs/HANDOFF.md` lines 65–115 are single paragraphs of up to 12k characters. Each records one more guard on the evidence contract (comment-stripping in sandbox profiles, bool-as-int rejection, "evilapi.openai.com" substring spoofing, …).

### 3.2 Timeline of drift

1. **Apr 1–2.** Eleven crates were scaffolded in a day, "Stage 0" was claimed, and the evaluator was relaxed by A² itself. Then roughly 45 trivial autonomous rounds (61 in total through Apr 4).
2. **Apr 5–6.** A raw-vs-A² baseline showed no difference ("baseline proves bench tests the wrong thing", `d1d29c2`). This was the right moment to pivot to a value question. Instead, budgets and timeouts were raised.
3. **Apr 24 – Jun 30.** A self-correction harness was built on A²'s own code. Fixtures were then hardened whenever a model passed on attempt 1 (e.g. the `compound-archive-index-hidden` addendum, `self-correction-benchmark.md`, 2026-06-17). That is moving the target to keep the loop "exercised", not measuring value. A provider-coverage sweep followed.
4. **Jun 29 – Jul 8.** The demo evidence contract and sandbox attestation. This is evidence about evidence; it produced zero new measurements.
5. **Jul 10–11.** An audit led to the roadmap, Phase 0, and then the project went idle.

### 3.3 Wheel-spinning patterns

- **Validator recursion.** When the next real step is blocked (sandbox not implemented, provider quota, paid-run confirmation), work moves to hardening the validator of the existing artifact, then the validator of the documentation of the validator (`verify-demo-docs`, "documented test-count guard", the `--update` count updater).
- **Fixture treadmill.** Fixtures were made harder until the loop "exercised" again. Without a control arm, that cannot produce attributable evidence.
- **Provider matrix as progress.** Each new model release produced a new row. Rows are inventory, not capability.
- **Process root cause.** HANDOFF "What NOT To Do" opens with "Don't ask for direction. The project name is the instruction." Memory `feedback_no_pausing.md` reinforces this. Combined with a blocked experiment path, the directive made agents fill sessions with safe, local, always-green hardening work. The roadmap's rule 6 ("two attempts without movement require a blocker report") is the correct antidote but has no teeth while the no-pausing directive stands.

### 3.4 Does the core thesis have any supporting evidence?

**No.** Each part of the thesis, against what has been observed:

| Thesis component | Observed evidence |
|---|---|
| A² beats a raw model | No (5/5 vs 5/5) |
| A²'s loop beats a plain verifier-feedback retry | Never measured |
| Persistent lineage helps | Never isolated; anti-repeat ablation null |
| Self-modification compounds (generation N+1 beats N on held-out work) | Never measured; the only self-modification of the evaluator weakened it |
| Catalytic closure / repair coverage | `Ok(())` |
| Differentiation | Not attempted |

That is not evidence *against* the thesis. It is the absence of any test of it, after six months.

### 3.5 Sibling A²D: same evidential point, duplicated plumbing

A²D (`../a2d-autopoietic-autocatalysis-deutero`, about 45.9k LOC Rust across `a2d-cli`/`a2d-core`/`a2d-providers`) has the same destination (memory `project_a2d.md`) and **no path dependency on any A² crate**. Provider adapters, sandboxed worktree runners, lineage, retry loops, Senior-SWE-Bench / SWE-Bench-Pro harnesses and contamination guards have been written twice.

A²D's own HANDOFF says:

- "Evolved topology value is unknown";
- "Compounding self-modification is unproven";
- seed-vs-evolved results are noisy (83/67, 67/50, 83/50).

Its one real-repo pass (`runs/20260709-senior-swe-bench-live-self-iteration-…/validation-summary.json`) was a provided local evaluator, not the official one. The sequence was: DeepSeek attempt 0 failed patch preflight, then Kimi k2.6 passed 3/3 first try. That is provider switching, not learning. Its July log is the same hardening pattern: percent-, base64- and HTML-entity GitHub-reference detectors, env scrubbing, evidence schemas.

**Consolidation is justified on cost, not on evidence.** Neither project is ahead on the thesis.

---

## 4. Judgment of the 2026-07-10 remediation roadmap

**What it gets right.**

- It is an honest diagnosis: PreStage0, the relabelled sentinels, "a working-tree `git apply` is not a promotion", the vacuous repair coverage, and the zero-test evaluator rule.
- Phase 6's pre-registered design is good: power analysis, a stateless-retry arm, a lineage-disabled arm, matched budgets, and GO / NO GO / INCONCLUSIVE outcomes.
- The anti-wheel-spinning rules 3, 6, 8 and 9 are exactly right.

**What is wrong: ordering conflates sequencing with dependency.**

1. **The Phase-6 value campaign is candidate-only.** It compares raw, stateless retry, lineage-disabled A² and full A² on sandboxed external tasks, and it never admits anything to the germline. Its true prerequisites are:
   - (a) a sound process-tree network boundary (Phase 2.8–2.9);
   - (b) authoritative verification bound to the candidate (Phase 2.1–2.5, for the external tasks only);
   - (c) a clean held-out task set.

   Phases 1 (atomic heredity), 3 (constitution/membrane/sensorium), 4 (the thirteen-label Bazel Stage-0 suite, escrow, mission battery) and 5 (an admitted-descendant trace) are **not dependencies of that test**. They are dependencies of *resident self-modification*, which only matters if the value test passes.
2. **The roadmap forbids its own decisive experiment.** Several rules combine to rule out any paid provider run until the Phase-4/5 gates are done:
   - Phase-0 stop/go: "Do not run paid providers… while… Bazel target parity is absent";
   - anti-wheel-spinning rule 1: "No paid provider run while an earlier deterministic gate is red";
   - rule 2: no fixtures before the sealed campaign.

   The cheapest falsifier is scheduled last.
3. **The sandbox requirement is over-specified.** The bespoke macOS `sandbox-exec` attestation stack (profile-line hashing, comment stripping, endpoint spoof checks) is a research project of its own. A Linux container or VM with default-deny egress and an HTTPS allowlist proxy gives a defensible boundary for an *experiment*, and it is shared by every arm, so contamination risk is symmetric. Treat the bespoke stack as a later hardening task.
4. **Bazel is on the critical path for no experimental reason.** The monorepo `CLAUDE.md` mandates Bazel, so keep it as a convention and keep the queryable graph. Nothing in the value question depends on it, though. Make Bazel parity non-blocking.
5. **Rough cost.**
   - Phases 0–5 cover about 40 distinct work items, including a mission battery, a mutation corpus, escrow bundles, an independent CI, and atomic admission with crash recovery.
   - At the historical pace (about 2 substantive mechanisms per month among the hardening), that is likely 4–8 months before the first value datum.
   - The evidence so far gives no reason to expect a positive result, so the expected value of building the full safety stack first is low.
6. **What is missing.**
   - **The heredity test itself.** Phase 6 measures the *retry loop*. The autopoietic thesis needs a second, cheap experiment: let A² author changes to its own prompts and policies (human-reviewed, B0), then measure generation N+1 vs N on a *fixed external* held-out battery. That experiment can be run manually with branch checkouts. It needs neither atomic admission nor resident autonomy.
   - **Any project-level kill criterion.** There is no rule for stopping or pivoting the project; every failure routes back to an earlier phase.

**What I would cut, reorder or add:**

- **Cut from the critical path:** Phase 3 (constitution/membrane/sensorium integration), Phase 4 (Bazel Stage-0 suite, escrow, mission battery), and the bespoke `sandbox-exec` attestation. Keep them as a backlog that the experiments either unlock or retire.
- **Reorder:** container sandbox, then a minimal verifier registry for external tasks, then the Phase-6 campaign (slimmed), then the heredity experiment. Phase 1 and the rest come only on GO.
- **Add:** a stateless-retry baseline to the existing harness right away. Remove history leakage (squash the injected bug into the base commit, or run with `--no-tags --depth 1` / an orphan commit). Fix the zero-test acceptance rule. Add project-level kill criteria (§5).

---

## 5. Options

### Option A — Continue the roadmap as written
- **Cost:** about 4–8 months of agent and human effort before any value datum. Provider spend is deferred.
- **Risk:** high. The pattern of building gates while blocked recurs. The project may never reach Phase 6, and if Phase 6 is NO GO the safety stack was built for nothing.
- **What it would prove:** that A² can safely admit and roll back its own changes. That has value only conditional on a GO.
- **Kill criterion:** if Phase 5 is not reached by 2027-02-28, or Phase 6 is NO GO / INCONCLUSIVE, stop.

### Option B — Value-test first (slim roadmap)
- **Cost:** about 3–5 weeks. Provider budget is capped (for example, a fixed number of tasks × arms × attempts on cheap Pi/OpenCode models).
- **Work:**
  1. container sandbox shared by all arms;
  2. a stateless-retry arm and history-leak fix in the harness;
  3. a pre-registered held-out external set of 30–60 tasks. The primary set is Senior SWE-Bench / SWE-Bench Pro, using the sanitized-provenance tooling already in the repo (`a030380`, `682e8c0`, `5af7602`); A²D targets the same set. SWE-bench Verified/Lite is only a fallback: it is in 2026 models' training data, which inflates raw pass rates and leaves the loop little room to help;
  4. four arms: raw, stateless retry with verifier feedback, A² with lineage disabled, full A²;
  5. then experiment 2, heredity (generation N vs N+1 on the same battery, B0 manual admission via branches).
- **Risk:** low to medium. The result may well be null. That is the point: it is cheap.
- **What it would prove:** whether A²'s loop machinery (lineage, ContextPack, anti-repeat) adds anything beyond the retry pattern everyone already uses, and whether self-authored changes compound.
- **Kill criteria (pre-register these):**
  - **Power, stated honestly.** With 30–60 paired tasks, the minimum detectable effect is roughly 15–20 percentage points (pp) of resolved rate. The experiment is deliberately sized to detect only a *large* effect. If A²'s loop is not dramatically better than stateless retry on a small sample, it does not justify months of safety stack. A small true effect would not justify the project anyway.
  - **E1:** full A² must beat stateless retry by ≥15 pp resolved on paired tasks (exact McNemar or paired bootstrap, one-sided α=0.10). Otherwise it is NO GO on the loop. **INCONCLUSIVE counts as NO GO** for continuing as a research runtime. This matches the roadmap's own anti-wheel-spinning rule 10, which keeps autonomy disabled on INCONCLUSIVE.
  - **E2:** after 3 admitted self-changes, generation N+3 must beat N by ≥15 pp on the same paired battery. Otherwise, INCONCLUSIVE included, it is NO GO on heredity.
  - **Calendar box:** the decision is due by **2026-11-30**, with no extensions for added gates.

### Option C — Consolidate with A²D
- **Cost:** about 2–4 weeks of migration, or zero if A² simply stops and A²D inherits the experiment design.
- **Work:** choose one runtime. A²'s worktree catalyst, lineage and pre-registered campaign design are its best assets. A²D's fitness-evidence and metabolism scheduler are its best assets. Retire the duplicate provider adapters, sandboxes and SWE-bench harnesses.
- **Risk:** medium. It halves maintenance but rescues nothing evidentially, because A²D's thesis is equally untested. It also risks becoming a porting project that defers the value test again.
- **What it would prove:** nothing by itself. It is a cost decision.
- **Kill criterion:** if the consolidated project has not run Option B's E1 by 2026-12-31, stop both.

### Option D — Reframe A² as a narrow tool
- **Cost:** about 2–3 weeks to package.
- **Work:** drop the autopoiesis framing. Ship A² as a **sandboxed, verifier-gated, lineage-recording coding-agent eval harness**: multi-provider, worktree-isolated, pre-registered paired-cohort campaigns. That is what the code actually is, and it is genuinely useful to the wider ecosystem (A²D, Middens/Third Thoughts data collection, Foundry).
- **Risk:** low. It gives up the research thesis.
- **What it would prove:** utility as infrastructure.
- **Kill criterion:** if no sibling project consumes it within 60 days, archive it.

### Option E — Stop and write it up
- **Cost:** about 1 week.
- **Work:** archive A² with a post-mortem: DESIGN.md as an essay, plus the day-1 Goodhart episode, the validator-recursion pattern and the no-pausing failure mode as findings (they suit Sawdust/Third Thoughts). Redirect the effort to A²D or elsewhere.
- **Risk:** you lose a thesis that might be right but was never tested.
- **What it would prove:** nothing about the thesis, but it frees the budget.

---

## 6. Recommendation

**Option B, run with consolidation in mind (B then C or E).** Freeze the remediation roadmap at Phase 0 (done) and mark Phases 1–8 as *conditional on GO*. Run two pre-registered, time-boxed experiments. Then make an explicit continue / fold-into-A²D / reframe-as-tool / stop decision on the data. Do not migrate A²D first; the experiment is cheaper than the migration and tells you which runtime is worth keeping.

### First actions

1. **Fix process before code.**
   - Amend HANDOFF "What NOT To Do" and the `feedback_no_pausing` memory for A²: *when the experiment path is blocked, write a one-page blocker report and stop; do not add guards to existing artifacts.*
   - Add a hard rule: no new commits to `bench/self_correction_demo.py`, sandbox attestation or the HANDOFF verification paragraphs until E1 reports.
   - The owner decides this; agents should not self-authorize it.
2. **Repair the harness's validity holes** (about 2–3 days, deterministic, no provider spend):
   - (a) remove history leakage by putting the injected bug into the base commit, with no visible "inject" commit;
   - (b) make zero executed tests a failure in `SeedEvaluator` (`seed.rs:24-37`);
   - (c) add a `--arm stateless-retry` mode: the same verifier stdout fed back, lineage and ContextPack disabled, a fresh TaskId;
   - (d) add `--arm raw` (single attempt).
3. **Stand up a container sandbox** (about 3–5 days): a Linux container or VM with default-deny egress and an allowlist proxy for model endpoints, shared by every arm. Record the image digest and proxy config as the boundary artifact. Unblock candidate-only provider runs under it. `--apply` stays locked.
4. **Pre-register and run E1** (about 1–2 weeks):
   - 30–60 held-out external tasks (Senior SWE-Bench / SWE-Bench Pro as primary, SWE-bench Verified only as a fallback with a contamination-ceiling caveat), with the export digest fixed;
   - pre-registered minimum effect of 15 pp with the paired test from §5 Option B; INCONCLUSIVE counts as NO GO;
   - 4 arms, matched token and wall-clock budgets, 3 attempts per arm, randomized order, cheap providers (Pi/ZAI GLM, Kimi, MiniMax);
   - primary metric: resolved rate, full A² vs stateless retry;
   - write the protocol to `bench/external-value/protocol-v1.toml` *before* any provider call.
5. **If E1 is GO or borderline, run E2 (heredity)** (about 1 week):
   - allow 3 B0-reviewed A²-authored changes to A²'s prompts, policies or retry strategy, admitted manually on a branch;
   - re-run the same battery (or a fresh held-out split) at generations N, N+1 and N+3.

### Evidence that triggers a re-decision

| Result | Decision |
|---|---|
| **E1 NO GO or INCONCLUSIVE** (full A² does not clear +15 pp over stateless retry at the pre-registered test) | Stop A² as a research runtime. Choose D (package as eval harness) or E (archive), and move the experiment protocol to A²D as its entry gate. |
| **E1 GO, E2 NO GO** | A² is a better retry loop, not a self-producing system. Choose D or keep it as a Stage-2 "reflexive patching tool" (DESIGN.md's own minimum viable outcome). Do not resume Phases 3/4/8. |
| **E1 GO and E2 GO** | Resume the roadmap in dependency order: Phase 1 heredity, then 2, then a slimmed 4. Run Phase 3 and RAF work only against admitted trajectories. Then consolidate A²D into A² (or the reverse), keeping the runtime that produced the GO. |
| **Not run by 2026-11-30** | Treat as NO GO. The project's binding constraint is the inability to run the experiment, not the absence of gates. |
