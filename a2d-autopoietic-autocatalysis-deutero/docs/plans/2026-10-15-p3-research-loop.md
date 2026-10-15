# Plan: P3 — The Outer Research Loop (autoresearch on the harness)

**Created:** 2026-10-15 (P2 gate; supersedes `2026-10-14-p3-ablation.md`)
**Status:** Completed 2026-10-15 — not promoted (exit and gate: roadmap Addendum 2026-10-15)
**Parent:** roadmap § P3 and the P2-gate Addendum; rules in `docs/PROTOCOL.md`.
**Reviewed:** 2026-10-15, P2 gate (Codex `gpt-6.1-sol`, fresh Claude subagent); 2026-10-15, P3 → P4 gate (same reviewers).
**Completed:** 2026-10-15. S1–S4 were killed by their rules, and S4b survived its development screen. The frozen P4 base is H0 v5. Validation looks used: 1 of 4. Spend: about 1,450 invocations.
**Addendum:** 2026-10-15 — the S4 → S4b revision used the mechanism-5 slot after S4's result. This is disclosed in the LOG (S4b gate disclosure).

## The constraint (from P2)

D fails 96/296 pooled, and **88 of those failures are public-green**. That is about 29.7 pp of headroom, which public tests cannot see. To gain +10 pp over D, a mechanism must convert a share *conv* of D's public-green-but-hidden-fail tasks while breaking a share *regr* of D's passes, with conv × 29.7 − regr × 67.6 ≥ 10:

| regr | conv needed for +10 pp |
|---|---|
| 0 | 0.34 |
| 0.05 | 0.45 |
| 0.10 | 0.56 |

Resampling selected by public tests cannot reach this. B (5 samples) ≤ D, and C's second attempt fixes about as many tasks as it breaks. **Only a signal beyond the public tests can.**

## Step 0 — offline, about 0 invocations, gates everything
1. **Treatment audit.** Does C's second coder prompt contain its previous file? Inertness of the architect and tester is shown by unit test (mock provider: the second coder prompt is byte-identical with and without them).
2. **Failure taxonomy** on train (`tasks/c1-train`, with hidden tests visible). Map each public-green-but-hidden-fail failure to its numbered clause and tag it omission, misinterpretation or slip. This sets each mechanism's ceiling.
3. **Selection headroom.** Hidden-score all existing E07/E07b candidates on train families (A 2, B 10, D about 2.7, C 4 per task). Compare the any-pass rate with the public-rule pick.

## Mechanisms, in order (each pre-registered in one row: mechanism metric with a kill threshold, end metric, budget)
1. **Blind spec tests** drive post-green repair and selection.
   - A test-writer sees the numbered requirements and the public tests (for the API), never the code, and writes one test per clause.
   - The harness compiles each test separately and drops tests that compile against no candidate.
   - Repair gets the failing tests plus the clause text ("clause is authoritative; the test may be wrong"), and a repair is kept only if public tests stay green.
   - Selection key: (public, spec, latest).
   - **Offline falsification first,** with Luna-low writing tests once per train task:
     - kill if validity against the reference is below 0.85;
     - kill if AUC (spec-pass fraction → hidden pass, among public-green candidates) is below 0.65;
     - kill if recall on public-green-but-hidden-fail candidates is below 0.45.
2. **Clause-by-clause review and fix after green** (the re-targeted, consumed tester). Kill if conv < 0.25, regr > 0.05, or fewer than 50% of GAP flags hit the failing clause.
3. **Agreement selection** (CodeT or MBR-exec), only if Step 0 headroom is at least 5 pp.
4. **Prompt hygiene** (clause map before code; drop the seed prompt's has_tests and GitHub boilerplate): folded into H0, with a "D + same prompt" control.

## Harness = fixed interpreter + JSON config (built only after mechanism 1 or 2 survives offline)
Steps:
- `code` (prompt template; previous file and feedback included);
- `spec_tests` (test-writer template);
- `review` (clause-review template);
- `stop` (at public-green, or continue post-green with a named action);
- `select` (the key; it reaches the submission).

Every step's output has a declared consumer, and the registration fails otherwise (the reachability audit). P4's architect edits this config.

## Loop rules
- **Train** = calibration-v2 + dev-v2 + c1-train. **Validation** = c1-validation, aggregates only, at most 4 looks in P3, each logged with the harness hash. D runs alongside every validation look.
- At most 5 primary mechanisms, each with at most 1 train-only revision (10 configurations). Every configuration is recorded, including failures.
- **Promotion gate to the Confirmation-2 run:** validation point lift ≥ +12 pp over the concurrent D, a paired lower bound above 0, and train mechanism metrics above break-even. Otherwise the best-by-validation harness is frozen as P4's H0 and recorded as "not promoted".
- Budget: P3 ≤ 3,000 invocations.
