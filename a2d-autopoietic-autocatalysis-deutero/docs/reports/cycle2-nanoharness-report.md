# Cycle 2: Autoresearch on a Minimal Coding Harness

**Status: DRAFT (2026-10-17).** Sections marked **[pending]** are filled at close:
- confirmation-2 (`runs/c2-confirm2/decision.json` plus the C2-WEB audits);
- the E005–E010 batch and its confirmation step;
- the proposer comparison (phase B);
- the final dev-holdout look.

Look-1 numbers stay in `~/.a2d-holdout/runs/c2-look1/report.json` until close, per its read discipline.

**Goal:** the session `/goal` of 2026-10-15 (quoted in the plan).
**Plan:** `docs/plans/2026-10-15-cycle2-nanoharness.md`.
**Evidence:** `docs/experiments/LOG.md`, section "Cycle 2" (every entry registered before its run), and `nano/results.tsv` (one row per experiment stage, including discards).
**Outcome:** **[pending]**. Known so far (10-18): confirmation-2 ended in **IntegrityReview** (2 web searches in arm B, replicate 1). The set is spent with no claim, so a SUPPORTED claim in this cycle needs a new blind confirmation set.

## The question

1. Can harness changes make our coder produce working software more reliably than its plain repair loop D, with the same coder model and the same per-task invocation cap?
2. Does A²D's own architect, fed A²D's own observations, propose such changes better than the same model without them?

- **Regime:**
  - Single-file Rust tasks from numbered specifications, with weak public tests and 20+ hidden tests per task.
  - Coder: `codex/gpt-6-luna` at low effort, no tools, k = 5 model calls per task, on any lane.
  - Hidden tests never reach a harness or a proposer.
- **Data:**
  - **dev:** `tasks/train-all`, 140 tasks in 35 families;
  - **dev-holdout:** 100 tasks in 25 families, blind, outside the repo, aggregates only;
  - **confirmation-2:** 144 tasks in 36 families, blind, spent once.
  - All were authored by Codex `gpt-6.1-sol`, validated, and independently solved.

## Setup

- **Nanoharness:** `nano/harness.py` is one editable Python file, run by the controller as an external arm (JSON lines: model, lane, extract, public_test, done).
  - v0 reproduces D: first prompts byte-identical on 280/280 task-arm pairs, all-pass within 1.4 pp (C2-AA).
  - Metabolism, enzymes, lessons and the evolver re-enter only as kept experiments. None has so far.
- **Eval** (`nano/eval.py`): paired runs on the same tasks; screen on half the families, then the full set, then a fresh confirmation replicate; cluster (family) SEs.
  - **v2.1** used the goal's hidden-test fraction.
  - **v2.2** switched the primary metric to all-pass (working software), with fraction as a guard (disclosed deviation; see Deviations).
  - **v2.3** takes a promoted keep's A/A from its own two replicates: thr_all = 3.57 pp for E002, 7.14 pp for v0.
- **Budget:** ≤ 3k Luna calls per day (shared account), ≤ 25k in total, deadline 2026-11-30.
  - Envelope: search ≈ 10.8k, proposer comparison ≈ 6.5k, confirmation-2 ≈ 3.2k, final look ≈ 1.3k.

## Experiment 1: the information source (C2-E1, offline, train)

- **Setup:** on the frozen S0 pool (136 tuples), a judge reads only the spec and picks among Luna's public-green candidates. The equal-budget arm is 4 samples + 1 judge call.
- **Results:**
  - **Claude (Sonnet) judge: 37/39** decisive tuples, pick rate 0.831, against D's 0.743. The rule was ≥ 0.793: **met.**
  - Sol judge: 39/39 (0.846, the ceiling).
- **Consequence:** cross-model review became the first lever. The Claude CLI was unavailable to subprocesses, so the live judge is Sol: a stronger model from the same vendor, not a different family.
- **C2-E2** (offline, E002's 68 decisive tuples): Claude Opus 5.5 got 66/68, Sol 68/68. The selection effect is not vendor-specific.

## Search (dev set, exploratory)

| Exp. | Proposer | Change vs incumbent | Result |
|---|---|---|---|
| E002 / E002b | claude | v0 → 4 Luna samples + 1 Sol judge over distinct public-green files | **KEEP:** screen +11.8, full +10.7 (SE 2.9), fresh confirm +7.1 (SE 3.5) pp all-pass vs v0 = D |
| E003 / E003b | claude | same 4 samples, random green pick (attribution control) | discard (full): −1.4 pp. **The judge is the active ingredient.** |
| E004 | claude | repair to green, then sample + judge | screen +4.4 pp; superseded by E002's promotion |
| E005 | claude | samples 2–4 get different "Approach" angles | provisional keep (full): +5.0 pp (SE 2.8) vs E002; **[pending confirmation]** |
| E006 | claude | Sol spec notes before 3 samples | **[pending]** |
| E007 | sol | 3 drafts → Sol pick + defect list → Luna repair | screen +4.4 pp; **[pending]** |
| E008 | sol | 4th sample becomes a synthesis | **[pending]** |
| E009 | claude-c1 | task-agnostic failure-class checklist | **[pending]** |
| E010 | claude-c1 | two readings per Luna call | **[pending]** |

- **Where E002 still fails (C2-FA1):** the judge never picked a failing file when a passing green existed (0 misses in 249). All-pass equals its own any-pass ceiling (0.857), so **the bottleneck moved from selection to generation:** shared misreadings defeat any selector.
  - The C2-FA2 taxonomy (Claude, train labels only) guided E005, E006, E009 and E010.
- **E005 decomposition:** of its +7 tasks, +5 came from generation (more tasks with a right reading in the pool), +2 from selection noise in the incumbent arm, and 0 from repair. The incumbent arm drew low (116/140, against 123 and 117 in its earlier replicates).
- **Cost:** E002-class arms are **not cost-matched**: about 4.0 Luna + 0.9 Sol calls and 6.4M tokens per 140 tasks, against about 1.3 Luna calls and 1.4M tokens for D. Every claim is "at an equal invocation cap".

## Dev-holdout look 1 (C2-LOOK1)

- **PASS, valid.** The rule: frozen E002 vs D, 100 tasks × 2 replicates, lift ≥ +10 pp with CI low > 0.
- Numbers: **[pending: released at close from `report.json`]**.

## Confirmation-2 (C2-CONFIRM2): frozen E002 vs D and B

- **Registration:** `78727f29` (registration file `runs/c2-confirm2/registration.json`). r = 2 replicates on separate UTC days. SUPPORTED requires the lift ≥ +10 pp with CI low > 0 against **both** D and B.
  - It was reviewed by Codex Sol and a fresh Claude before spending.
  - Power at a true +10 pp is about 0.47, with a recorded exception for P(RULED OUT | 0) = 0.73.
- **C2-WEB amendment (strictly tightening):** any web-search event in any arm makes the outcome IntegrityReview.
- **Result: IntegrityReview, so no claim.** The confirmation-2 set is spent.
  - Replicate 1 (10-18, 1,486 Luna and 132 Sol calls): its web audit counted **2 web-search events in arm B** (720 transcripts), against 0 in D (185) and 0 in X:frozen (713). There were no invocations without a transcript and no other codex version.
  - By the registered amendment, this fixes the outcome whatever `decision.json` says. Replicate 2 still runs, as registered.
  - The scored numbers will be **descriptive only**: **[pending: `decision.json`]**.
  - The cause is not examined: that would mean opening the set. The counts are the whole permitted view.

## Proposer comparison (phase B)

- **Design:** protocol v2, frozen 2026-10-16. A²D's architect role (`gpt-6.1-sol`, high) fed A²D's mechanical observations, against the same role without them.
  - 6 rounds, each a 3-arm run on a seeded quarter of the dev families.
  - Endpoints: confirmed keeps per arm, screen-pass rate, and the pooled paired precision difference. "Inconclusive" is the expected outcome at this budget.
- **Trigger:** when the remaining budget reached 11k, it started after the batch's confirmation step (pre-registered 10-17). The keep count reached by then is reported prominently.
- **Result:** **[pending]**

## Instrument integrity in cycle 2

- **C2-ISO** (10-16): the static harness guard was porous (attribute routes to the filesystem). Harnesses now run under `sandbox-exec`, with no network and no file access under `$HOME` or the temp roots.
  - A mechanical scan of every harness that ever ran found nothing.
- **C2-ISO2** (10-17): the oracle's compile and test steps now also run sandboxed. The run step gets no processes, no Mach lookups and a cleared environment.
  - Parity: 420/420 identical against the old binary.
- **C2-WEB** (10-17): codex-cli 0.162.0 ignores `--disable web_search_request`, so web search was available to every call, and the controller's tool count (exec only) could not see it.
  - **0 searches in about 15.8k recorded transcripts** (every dev run since 10-14, and every holdout run, counts only), so no result recorded before confirmation-2 is affected.
  - Fixed for proposers. Dev runs stop on any search, and confirmation-2 is audited. The provider itself is fixed after the confirmation-2 freeze.
  - **Confirmation-2 is the exception:** 2 searches by arm B in replicate 1, which voids its claim (above).
  - **Lesson:** a known open channel on a spend-once set must be closed and re-registered before the spend, even at the cost of a re-review. An audit run after the spend can only tell you the set is gone.
- **C2-MCP** (10-18): codex also loads a curated Cloudflare plugin (an MCP server) and local skills, despite `--ignore-user-config`.
  - The server needs OAuth and never connected, so the coder's rare `list_mcp_resources` calls returned nothing.
  - Counts: 7 in about 5.7k dev transcripts; 3 of 1,240 in look 1, all in X; 0 in confirmation-2's replicate 1.
  - Look 1's gate still passes with those 3 tuples imputed as fails (a one-bit check).
  - About 12k chars of codex-injected skill and multi-agent context precede every prompt, the same in every arm.
  - No recorded result is affected. The fix is an empty `CODEX_HOME`, a feature allowlist and structured-event detection.

## Claims and scope

- **[pending: final wording after confirmation-2]**
- Wording rules already fixed:
  - "at an equal invocation cap", never "equal cost";
  - Sol is the same vendor as Luna, so not a cross-family claim;
  - measured on hidden-test all-pass for this suite, not general software reliability.

## Deviations and amendments (process diagnostics)

- **v2.2: primary metric fraction → all-pass,** after E002's screen (fraction −, all-pass +11.8 pp).
  - Disclosed. The goal names fraction. The affected experiments were re-registered (E002b, E003b, E004b), and the fresh confirmation replicate is the actual test.
- **v2.3:** the A/A from a keep's own replicates; its noise was calibrated on v0 (±30%).
- **Look 1 re-registered** after C2-ISO changed the controller's frozen paths, before any look data.
- **Confirmation-2 re-registered** (b8a274aa → 78727f29) after its own binding check failed on a path bug, before any call.
- **The C2-WEB amendment** was registered before any confirmation call.
- **No live Claude lane:** the Claude CLI is blocked inside the session sandbox, so Claude contributed offline via subagents (C2-E1's primary judge, C2-E2, C2-FA2, proposer round c1, reviews). Their tool use was not machine-audited; this is disclosed.
- **Phase B started before 3 confirmed keeps** (budget trigger), if so; a disclosed deviation that tests an earlier system. **[pending]**
