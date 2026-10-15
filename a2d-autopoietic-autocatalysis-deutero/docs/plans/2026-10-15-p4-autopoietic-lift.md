# Plan: P4 — Autopoietic Lift (self-written lessons)

**Created:** 2026-10-15
**Status:** Completed 2026-10-15 — no-go at validation (E12). Confirmation-2 not spent. v3, adopted at the P3 → P4 gate; base amended to H0 v6 (Addendum).
**Completed:** 2026-10-15 (no-go: C(H\*) − D = −1.2 pp on validation, below the +10 pp go bar; control tied, so the "learning from failures" wording is withdrawn).
**Reviewed:** 2026-10-15, P3 → P4 gate. Codex `gpt-6.1-sol` (high) and a fresh-context Claude subagent, each in a copies-only temp dir. Every item has a disposition in § Review dispositions.
**Addendum:** 2026-10-15 — the v2 draft (cross-fit ratchet from H0 v3) was superseded at the gate. v3 changes:
- the base is H0 v5 (evolver-free, plain rendering);
- the incumbent is re-evaluated fresh every generation;
- two informed and two control searches;
- a validation go/no-go;
- a multi-harness Confirmation-2 run at r = 2.

**Addendum:** 2026-10-15 — **G0 (E10) failed its never-green criterion (C 5 vs D 2). H0 v6 replaces H0 v5.**
- **Diagnosis:** C is better on the first attempt (0.82 vs 0.77 public-green) but worse at repair (0.80 vs 0.94).
- **Fix:** full parity with D. D's system prompt plus the `main()` clause, D's repair prompt (`prompt_format: repair`), and escalation off (`harnesses/h0-v6.json`, sha256 `e4e2a937…`).
- **Retry:** E10b, with unchanged criteria. A second failure stops P4, and the residual gap becomes a finding.
- **Disclosures:**
  - S4b screened lessons on H0 v4's clause-check prompt; the searches measure them on D's prompt.
  - With parity, C(H0) ≈ D. The descriptive D + H\*-lessons arm on Confirmation-2 is optional, and is dropped if the envelope is tight.
  - Everywhere below, "H0 v5" now reads "H0 v6".
- **P4 spend estimate:** about 4.35k (E10 and E10b about 0.74k, searches about 2.9k, validation about 0.7k).

**Parent:** `docs/plans/2026-10-14-lift-first-roadmap.md` § P4. Rules: `docs/PROTOCOL.md`.

## Question

Do **lessons that A²D writes itself** from its own train failures, added to a repair-loop base, make it build working software more reliably than its coder used directly (D and B) and than the same base without lessons (H0)? This is measured on fresh families at an equal invocation cap.

- **Claim scope:**
  - fresh families of Sol-authored, single-template Rust specification tasks;
  - `gpt-6-luna` at low effort, no tools, k = 5;
  - "configured A²D": a repair-loop base plus self-written lessons. A result does not show that the seed metabolism, RAF closure or multiple enzymes caused a gain.
- **Why this lever** (LOG):
  - S1–S3: within-task self-signals failed their criteria.
  - T0: train failures concentrate in a few recurring, task-agnostic patterns.
  - P4-0: Luna-low names a violated clause from labels alone (0.56 vs chance 0.25).
  - S4b: four researcher-written lessons added +11.1 pp over H0 v4 (net +8 of 72) and +5.6 pp over D on families T0 never saw. That survival authorises this spend only.

## Baseline: H0 v5 (`harnesses/h0-v5.json`, sha256 `5c066914…`)

- The seed coder enzyme only. The architect, tester and evolver are dropped, and `enzyme_defs` is removed from the coder's catalysts, so RAF closure holds on the coder's own merits.
  - The evolver is dropped because it rewrote the coder template in 100% of multi-call tasks and took 17–40% of the calls (LOG E08r correction).
- A cleaned coder prompt that keeps the sandbox's `main()` clause.
- Previous-attempt routing.
- `plain_inputs`: the inputs are rendered as D sees them.
- `dedupe_input`: the task is rendered once.
- `stop_at_public_green`.
- The metabolism's loop-escalation ladder remains. Its calls are counted against k and reported.

**G0 (E10)** is the live check of H0 v5 on `tasks/train-all`, with D alongside. Its pass criteria:
- never-green C − D ≤ 2 tasks;
- rate within ±5 pp of D;
- zero evolver calls.

If G0 fails, the base is fixed before any search.

## What evolves

- **Only the harness `lessons` key:** at most 8 lessons, each at most 300 characters.
- **Every search starts from empty lessons.** S4/S4b's researcher lessons never seed a search and never influence the choice of H\*.
- The writer is `a2d distill` (`crates/a2d-cli/src/lift_distill.rs`), with the architect running on Luna at low effort.
- **Dossiers contain:** the requirements, the public tests, the submitted file, and one per-task all-hidden-tests bit (failures only). No hidden test text or hidden output (sentinel unit test).

### Before the first search, `distill` gains:
- a family allowlist per `--from`, so that only the proposal half is read;
- a train-suite-hash allowlist;
- `--max-dossiers`;
- token accounting;
- the uninformed pool deduplicated by task.

## Searches (4, run in parallel)

- **Searches:** I1 and I2 are informed. U1 and U2 are the uninformed control: as many dossiers as there are failures, sampled from every submission regardless of outcome, requirements only.
- **Split:** each search gets its own seeded family split of `train-all` into halves (X, Y), committed before it starts.
- **Generation g = 1…3:**
  1. **Propose.** `distill` reads the incumbent's latest failures on X only, and the merge writes the candidate lessons.
  2. **Evaluate.** Run C(candidate) and C(incumbent) **fresh, in the same batch**, on Y.
  3. **Accept** if the candidate gains **≥ 4 net tasks** over the incumbent on Y.
  4. **Swap.** The next proposal half is Y, using the latest Y run of whichever harness is now the incumbent.
- **Tripwires** (reported every generation):
  - E0601 count;
  - hidden compile failures;
  - any change to `coder_prompt` (only `lessons` may change).
- **Wording:** this is adaptive training with alternating proposal inputs, not an unbiased cross-fit estimate.
- **Reporting:** all four trajectories, accepted or not, with spend.

## Validation (one look, `c1-validation`, aggregates only)

- **Arms:** C(H\*_I1), C(H\*_I2), C(H\*_U1), C(H\*_U2), C(H0 v5) and D, frozen before the look.
- **H\*** is the better of I1 and I2 on validation; ties go to I1. Its validation lift is selection-biased and is never claimed.
- **Control consequence:** if the better control ≥ the better informed search, the wording "learning from failures" is withdrawn.
- **Go/no-go** (roadmap lesson: never spend confirmation data without development evidence at the claim threshold):
  - **Go:** C(H\*) − D ≥ +10 pp on validation.
  - **No-go:** Confirmation-2 is not spent. P4 is reported as "not supported at the development screen", never as "ruled out". The roadmap completes on its falsified path, worded precisely:
    - the seed's 10 pp lift is ruled out;
    - bounded development found no promotable successor;
    - inner self-modification was not confirmed.

## Confirmation-2 (one registered multi-harness run; deferred scoring; scored once)

- 144 tasks, 36 families, hash `cb7cb496…`, **r = 2**, k = 5, seeded tuple and arm order.
- **Arms:** D, B, C(H0 v5) and C(H\*), plus two descriptive arms:
  - **C(R):** the researcher reference. S4b's 4 Sol-derived lessons on H0 v5, disclosed as "human + Sol-derived". It is excluded from Completion and never feeds H\*.
  - **D + H\*-lessons:** D with H\*'s lessons appended. It shows whether the A²D harness adds anything beyond the lesson text.
- **Completion requires all three comparisons to be SUPPORTED** (each with CI low > 0 and lift ≥ 10 pp: intersection–union):
  - C(H\*) vs C(H0 v5), the self-modification effect (the 10 pp bar applies here too);
  - C(H\*) vs D;
  - C(H\*) vs B.

  B and D are compared separately, never "max".
- **Descriptive:**
  - C(H0 v5) vs D (prompt and rendering attribution);
  - C(H\*) vs D + H\*-lessons;
  - C(R) vs C(H\*).
- **Before registration:**
  - a multi-harness controller (named C variants and the D-note arm in one run), with tests;
  - a **joint** power simulation of the three-comparison rule, using development discordances from G0, the searches and validation aggregates.
  - If joint power is below 0.8 at a true +15 pp, the shortfall is recorded with a dated exception before scoring.
- **Cost:** about 288 tuples × 12 ≈ 3.5k invocations.

## Budget
- P4: G0 about 400; 4 searches × 3 generations × about 240 ≈ 2.9k; validation about 700. **Total ≈ 4.0k ≤ 5k.**
- Confirmation-2: about 3.5k (envelope about 5k).
- Search spend is reported separately from deployment spend.

**Decision date:** the P4 outcome is recorded by 2026-11-30.

## Review dispositions (P3 → P4 gate, 2026-10-15)

| # | Source | Item | Disposition |
|---|---|---|---|
| 1 | Sol minor | Word the S1–S3 conclusions narrowly | Accepted: the compound doc says those configurations failed their criteria |
| 2 | Sol major | S4 is not a measured ceiling | Accepted: the LOG calls S4 a researcher-lessons screen |
| 3 | Sol major / Claude major | S4b is a revision informed by validation; forking-path content | Accepted: disclosed in the LOG. The stricter-only amendment was recorded before the primary comparison existed |
| 4 | Sol major | Clarify S4b's rule before its result | Done (LOG S4b clarification) |
| 5 | Sol major | H0 changes are not all repairs; the claim is scoped to configured A²D; a "D + same prompt" control | Accepted: claim scope above. C(H0 v5) vs D and D + H\*-lessons are the attribution arms |
| 6 | Sol minor / Claude critical | The evolver is not inert; freeze one choice | Accepted: the evolver is dropped in H0 v5 (`drop_coder_catalysts`) and the E08r claim corrected |
| 7 | Sol critical | `distill` does not enforce the split or train-only sources | Accepted: family allowlist and suite-hash allowlist before the first search |
| 8 | Sol major / Claude critical | The +2 ratchet accepts noise; the incumbent is never re-run | Accepted: fresh incumbent on Y each generation; ≥ 4 net |
| 9 | Sol major / Claude major | The control's description, equal counts, and its consequence | Accepted: the plan matches the code; 2 + 2 searches; withdrawal rule |
| 10 | Sol critical / Claude major | Three registered comparisons; the 10 pp bar for self-modification; joint power | Accepted (§ Confirmation-2) |
| 11 | Sol major / Claude major | Power shortfall at r = 1 | Accepted: r = 2 pre-registered now. The residual shortfall (0.75) stays a recorded exception |
| 12 | Sol major | Scope to the author's distribution; report all trajectories | Accepted |
| 13 | Sol major / Claude major | A multi-harness controller instead of two runs | Accepted: required before registration |
| 14 | Sol major | The budget arithmetic was understated | Accepted: recomputed above |
| 15 | Sol major | `distill` caps and token accounting | Accepted (§ What evolves) |
| 16 | Sol minor / Claude major | The plan named H0 v3; E09 pending; S4 spend per task | Accepted: H0 v5 named; E09 closed; S4 corrected |
| 17 | Sol major / Claude Q4 | A researcher lessons arm on Confirmation-2 | Accepted as the descriptive arm C(R), excluded from Completion; dated PROTOCOL addendum |
| 18 | Claude major | Validation go/no-go | Accepted: go only at ≥ +10 pp vs D |
| 19 | Claude minor | Diagnosis read validation prompts | Accepted: deviation recorded (LOG S4) |
| 20 | Claude minor | T0 / P4-0 have only 21 distinct tasks; family concentration | Accepted: disclosed (LOG P4-0 note) |
| 21 | Claude minor | Lessons can override prompt contracts | Accepted: per-generation tripwires |
| 22 | Claude minor | The uninformed pool can draw the same task twice | Accepted: deduplicate by task |
| 23 | Claude minor | "Widens the margin" really means "creates" one | Accepted: the Question is reworded |
| 24 | Claude minor | Report S4b's per-run timing | Accepted: per-run provider_ms in the LOG |
| 25 | Claude Q5 | MBR-exec differential adjudication as a fallback | Parked: low prior (S2 and S3 show Luna judges poorly); revisit only if P4 is a no-go |
