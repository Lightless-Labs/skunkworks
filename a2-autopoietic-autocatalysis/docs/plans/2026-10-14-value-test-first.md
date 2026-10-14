# A² Value-Test-First Plan (E1 / E2)

**Created:** 2026-10-14
**Status:** Active — supersedes the execution order of `2026-07-10-a2-remediation-roadmap.md`
**Decision basis:** `reviews/2026-10-14-direction-audit.md` (Option B). The owner delegated the decision on 2026-10-14.
**Decision deadline:** 2026-11-30. No E1 decision record by then = **NO GO**. No extensions.

## Decision

Stop building gates before any value datum exists. Run one cheap, pre-registered external-value experiment (E1). Run the heredity experiment (E2) only if E1 is GO. Remediation Phases 1–8 are frozen and conditional on E1 GO. Phase 0 is frozen with one known gap: `//crates/a2_workcell:a2_workcell_test` is red under Bazel because one test spawns `cargo`.

## E1 — Does the A² loop beat plain retry?

**Arms** (same model, same task, matched token/wall-clock/attempt budgets, randomized order):

1. `raw` — a single attempt.
2. `stateless-retry` — up to k attempts with identical verifier feedback, a fresh context each attempt, no lineage, no ContextPack.
3. `a2-no-lineage` — A² loop with persistent lineage disabled.
4. `a2-full` — the full A² loop.

**Primary metric:** resolved rate, `a2-full` vs `stateless-retry`, on paired tasks. **GO** requires ≥ +15 pp, using an exact McNemar test or paired bootstrap, one-sided α = 0.10. **INCONCLUSIVE = NO GO.**

**Oracle separation (mandatory).** All arms receive feedback only from public/visible checks (pass-to-pass tests, or the agent's own test runs). Hidden fail-to-pass tests score the final artifact once, after the run. Hidden-test output never reaches any arm.

**Boundaries.**
- Agent arms run under the existing `sandbox-exec` `AllowList` profile, which permits model API endpoints only (`cd88992`). Do not rebuild this.
- Evaluation runs in the task's container with `--network none`.
- The task checkout is created without upstream history: a fresh single commit at the base, so neither the fix nor an injected bug is visible in `git log`.

**Task source.**
1. First verify the public SWE-Bench Pro split directly: dataset card, harness repo, image availability and per-image size. Record what was verified.
2. If it does not hold, fall back to the Senior SWE Bench export, using the existing sanitized-provenance tooling (`a030380`, `682e8c0`, `5af7602`).
3. Choose 30–60 tasks and record the export digest and the sample.

**Calibration.** Run about 5 tasks on `raw` first. If raw pass@1 falls outside roughly 20–60%, change the model or task subset, then re-calibrate. Calibration tasks are excluded from the E1 sample.

**Host constraints.**
- About 58 GB of disk is free, so each task is pull → evaluate → evict.
- Resize Colima before use; it is currently 2 CPU / 2 GiB.
- Budget wall-clock for image pulls.

**Pre-registration.** Commit `bench/external-value/protocol-v1.toml` **before the first non-calibration provider call**. It records:
- dataset digest, sample, model, k, budgets and arm order seed;
- metric, test and threshold;
- the infrastructure-failure taxonomy and reserve-task rule;
- the stop conditions.

Never edit it after outcomes are observed. Corrections go in a dated addendum.

**Output:** `docs/benchmark-results/external-value/e1/` holding the per-task rows and a decision record (GO / NO GO / INCONCLUSIVE) bound to the protocol digest and source commit.

## E2 — Do self-authored changes compound? (only if E1 is GO)

1. Make 3 B0-reviewed, A²-authored changes to prompts, policies or retry strategy. Admit them manually, as reviewed commits on `main`; the live `--apply` paths stay locked.
2. Re-run the E1 battery, or a fresh held-out split, at generations N and N+3.
3. **GO** requires ≥ +15 pp, using the same test. INCONCLUSIVE = NO GO.

## Re-decision table (from the audit)

| Result | Decision |
|---|---|
| E1 NO GO / INCONCLUSIVE | Stop A² as a research runtime. Package it as an eval harness (Option D) or archive it with a post-mortem (Option E). |
| E1 GO, E2 NO GO | A² is a better retry loop, not a self-producing system. Take Option D, or keep it as DESIGN.md's Stage-2 reflexive patching tool. Do not resume Phases 3/4/8. |
| E1 GO, E2 GO | Resume the roadmap in dependency order (1 → 2 → slim 4). Run Phase 3 and RAF only against admitted trajectories. |
| No decision by 2026-11-30 | NO GO. |

## Work rules

- **Zero-spend fixes come first:**
  - zero executed tests = failure in `crates/a2_eval/src/seed.rs`;
  - history-free task checkouts;
  - `--arm raw` and `--arm stateless-retry`.
- **Frozen until E1 reports:** no commits to `bench/self_correction_demo.py`, sandbox attestation/validators, or the HANDOFF verification paragraphs.
- **Stalled steps:** two attempts without movement on a blocking step → write a one-page blocker report in `docs/blockers/`, then change the design. Do not make a third tweak.
- **Shared checkout:** follow `AGENTS.md` (work on `main`, no worktrees, no stash, no destructive git, scoped commits, never touch `a2d-*`).
- **Long Cargo runs:** use the bounded liveness harness (`bench/phase0_liveness.py`).

## Addenda

### 2026-10-14 — Native grading (no container runtime on this host)

Local containers are impossible here: the host is a VM without nested virtualization, and every SWE-Bench Pro V2 image is amd64-only. See `docs/blockers/2026-10-14-no-container-runtime.md`. E1 therefore uses a **native-graded SWE-Bench Pro V2 Python subset**: history-free shallow checkouts, `uv` venvs pinned with `--exclude-newer <base commit date>`, and an official-equivalent resolution rule. A task enters the sample only if gold resolves it and the empty patch does not. The plan's "Boundaries" item for evaluation becomes "native grading in a disposable copy of the checkout" instead of a `--network none` container. Agent arms are unchanged.

**Feedback signal.** SWE-Bench Pro exposes no visible test subset. For tasks with ≥ 2 fail-to-pass tests, the fail-to-pass set is split deterministically, by SHA-256 of the test id, into a *visible* half and a *hidden* half. Every retrying arm receives identical feedback from running the visible half plus pass-to-pass. The hidden half is used only for final scoring. The primary metric remains official-rule resolution (all fail-to-pass ∪ pass-to-pass), computed once after the run. Hidden-half resolution is reported as a secondary metric. The split rule is fixed in the protocol before any model call.

**A² path facts found while mapping the loop** (to be fixed before calibration):
- retry feedback is replaced by hashes when `no_external_solution_search` is set, which blinds A²'s retries;
- `a2ctl run` cannot target an external repository;
- the worktree prompt hard-codes A²'s own crate layout;
- `TestResults` counts verifier commands, not tests.

### 2026-10-14 — Arms, subset, and agent read boundary (before the protocol exists)

- **Three arms, not four.** A² loads lineage per task id (`Governor::run_task` → `store.for_task`), and each task runs once per arm, so `a2-no-lineage` and `a2-full` would execute the same program in E1. The arms are `raw`, `stateless-retry` and `a2-full`. Whether persistent lineage adds value across generations is E2's question.
- **Stateless-retry carrier.** The driver loops `a2ctl run --attempts 1 --lineage-db none` with a fresh task id each attempt, and appends the previous attempt's verifier output to the problem statement. No lineage, no ContextPack, and no prior diff carry over. `a2-full` runs `a2ctl run --attempts k` with a fresh lineage db and unredacted verifier feedback. Both start every attempt from the clean base checkout.
- **Subset.** `ansible/ansible` tasks with ≥ 2 fail-to-pass tests (68 of 82 in V2 at dataset commit `2d52cb3d…`). The Python version is chosen from the base-commit year (≤ 2021 → 3.9, 2022 → 3.10, ≥ 2023 → 3.11), and deps are pinned with `uv pip install --exclude-newer <base date + 1 day>`. A task enters only if gold resolves it and the empty patch does not. openlibrary is out of scope (vendored submodules). In the spike, 3/3 ansible tasks passed the gate.
- **Agent read boundary.** The sandbox profile is `(allow default)` apart from network, so the agent could read private task material from disk. Agent launches therefore add `(deny file-read* file-write* (subpath <private-dir>))` for the experiment's private directory, which holds the parquet, gold and test patches, test lists and manifests. Verifier and grader run in A²'s process, outside the agent sandbox. The verifier command the agent sees is only `python3 <committed script> --task <opaque id>`; no private path appears in any prompt or command string.
- **Test-path reverts.** Both the visible-feedback verifier and the final grader revert agent edits to test paths before running, matching the official `test.sh`.

### 2026-10-14 — Boundary fix, install rule, and provider blocker (no model outcomes observed yet)

- **AllowList was never loadable.** Seatbelt rejects hostname filters, so AllowList is now enforced through a loopback egress proxy (`67d2bc3`, `bench/e1/egress_proxy.py`). An end-to-end check showed `api.z.ai` reachable via the proxy, GitHub refused, and direct egress blocked.
- **Install rule.** The first full gate passed 29/68 tasks; 26 failures were date-pinned build-dependency failures for 2019–2021 sdists on arm64. The pre-registered rule is now: a date-pinned install, then the same requirement files unpinned. The mode is recorded per task and the gold/empty gate decides validity. The first gate's results are kept privately for diagnosis and are superseded by the re-run.
- **Calibration is blocked by provider credit** (`docs/blockers/2026-10-14-provider-credit.md`). The protocol stays uncommitted until a provider is funded, since a TBD provider is not a pre-registration.

### 2026-10-14 — Provider: Codex `gpt-6-luna` (owner direction, before calibration)

The owner disabled Z.ai and MiniMax and directed E1 to Codex `gpt-6-luna`, which resolves the provider-credit blocker. Agents run as `codex exec` under A²'s Seatbelt profile; Codex's own sandbox is bypassed because profiles cannot nest. They use an isolated `CODEX_HOME` (`bench/e1/codex-home.config.toml`):
- off: web search, ChatGPT apps/connectors (a probe showed `codex_apps` → `github.fetch` on `api.github.com`, server-side and invisible to the proxy), plugins, remote plugins, browser, skill MCP;
- auth: the operator's `auth.json` symlinked;
- egress: only `chatgpt.com:443` and `auth.openai.com:443` through the proxy.

Probes confirmed `NO_TOOL` / `NO_SEARCH_TOOL` answers, private-dir reads denied, and GitHub refused. Codex's bundled system skills remain; they are identical across arms and network-confined. The smoke run (`smoke-codex-1`, gate-failed task) produced a diff, visible feedback and a grade, with egress to `chatgpt.com` only. Reasoning effort is A²'s fixed `high` for every arm.

### 2026-10-14 — Calibration ceiling: switch to issue-only instructions (before calibration 5)

`raw` calibration on the 5 calibration tasks (private rows under `~/.a2-e1/private/runs/`). These tasks are excluded from the sample, and every run is listed here:

| run | reasoning effort | raw resolved |
|---|---|---|
| `calib-1` | high | 5/5 |
| `calib-2-low` | low | 4/5 |
| `calib-3-minimal` | minimal | provider rejection (`gpt-6-luna` does not support `minimal`); not a data point |
| `calib-4-none` | none | 5/5 |

Reasoning effort is not a usable lever. The official instruction (problem statement + `requirements` + `interface`) gives the agent the exact names and signatures the hidden tests check. Decision:
- E1 uses **issue-only instructions**: the problem statement alone, with `requirements` and `interface` withheld from every arm.
- Reasoning effort is fixed at A²'s default `high` for every arm.

The interface then becomes discoverable only through visible-test feedback, which is the channel the retry arms differ on. Results are reported as an issue-only variant of native-graded SWE-Bench Pro V2, never as SWE-Bench Pro scores. Per-attempt time is not reduced, so time is not confounded with feedback.

**Pre-stated rule for `calib-5-issue-only`** (raw, same 5 tasks):
- 1–3 resolved → lock `protocol-v1.toml` with issue-only + high.
- 4–5 resolved → stop adjusting ansible and record a blocker. A new subset (Go toolchain for flipt, or openlibrary submodules) is a separate decision.
- 0 resolved → run `stateless-retry` on the same 5 first. Lock if it resolves ≥ 2; otherwise this is a subset decision.

Note: `A2_CODEX_REASONING_EFFORT` is also inherited by the verifier's shell (harmless; it only affects Codex).

**Addendum:** 2026-10-14 — `e1-v1` leak-guard crash (harness fix, no protocol change).

All three shards stopped after 22 rows, on the first stateless-retry attempt that received failing verifier output. The cause was the driver's leak guard, which treated the private directory's *path* as a secret. Pytest tracebacks name the task venv and the verifier temp tree, and both live under that path.

What was in the output:
- No hidden test id or gold-patch text appeared in any of the 38 recorded attempt files.
- The a2-full arm had already received the same output through a2ctl.
- Seatbelt denies reads under the private directory either way.

The fix limits the guard markers to gold-patch content and hidden ids (`run.private_markers`). Verifier output is unchanged, and every arm still receives identical feedback. The completed rows are kept, and the shards resume with the same commands.
