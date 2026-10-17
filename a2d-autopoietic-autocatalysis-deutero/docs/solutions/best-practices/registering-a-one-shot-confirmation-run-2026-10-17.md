---
title: "A Registration Is Code: Test a One-Shot Confirmation Run Against Itself Before the First Call"
date: 2026-10-17
category: best-practices
module: a2d-lift
problem_type: experiment_design
component: confirmation-runner
symptoms:
  - "The decision rule did not say how the validity gate, mixed outcomes or the choice of summary decide the verdict"
  - "No policy for an operational abort before scoring, or for per-tuple voids that hit only one arm"
  - "The runner accepted any --registered-at and hardcoded settings instead of reading registration.json"
  - "git status --porcelain reported a clean tree while the bound file was untracked or gitignored"
  - "git show <sha>:<path> failed from the subproject although the file was committed"
root_cause: registration_not_bound_to_runner_and_never_exercised_end_to_end
---

# A Registration Is Code: Test a One-Shot Confirmation Run Against Itself Before the First Call

## Context

After the dev-holdout gate (C2-LOOK1) passed, we spent Confirmation-2 (144 tasks, one shot) on the frozen harness E002 vs D and B (`C2-CONFIRM2`). The owner rule requires one Codex Sol review and one fresh-Claude review before registering. The runner is `confirm_registration`, `cmd_confirm`, `cmd_confirm_manifest` and `cmd_confirm_score` in `nano/eval.py`; the bound files are `runs/c2-confirm2/registration.json` and `frozen.py`.

## What the two reviews caught

Both reviews were needed; they overlapped only partly.

- **Decision rule underspecified.** Mapped onto the controller's pooled `--claim` (all-supported; RULED OUT decided by the primary comparison; Invalid/IntegrityReview dominate). Added a worst-case void imputation guard (treatment voids count as fails, comparator voids as passes), computed in code and written to `decision.json`.
- **No failure policy.** Covers an operational abort before scoring and per-tuple voids, which can be lopsided if only the treatment's judge lane fails.
- **Runner not bound to its registration.** It took an arbitrary `--registered-at`, hardcoded settings, wrote no durable attempt record before the first model call, opened its log in overwrite mode, allowed manifests to be overwritten, and let the pooled summary be recomputed. `git status --porcelain` also passes for untracked or gitignored files. Use `git ls-files --error-unmatch`, `git diff --quiet HEAD --` and `git merge-base --is-ancestor HEAD origin/main`.
- **Claim wording.** "Equal invocation budget" overstated it; the claim now says "equal invocation cap". The treatment arm (E002) used ~4.6x D's tokens, and one of its calls (the judge) uses a stronger model. Disclose that there is no cost-matched control.
- **Power.** Report every outcome class, not just P(SUPPORTED). Simulating the implemented rule gave P(RULED OUT | true 0) = 0.73, below the protocol's 0.8 target (a reviewer had guessed 0.88), so a dated exception was recorded before any call. Winner's curse: the fresh confirm stage alone (+7.1 pp) implies P(SUPPORTED) of about 0.20, against 0.37 at the averaged dev estimate.
- **Authorship confound.** The judge model also authored the confirmation tasks. Disclosed, and bounded by a cross-vendor judge check on dev (66/68 vs 68/68).

## Checks added after an advisor review

1. **Dry-run the exact deferred-scoring chain** on a 1-task dev suite (22 model calls): two deferred replicates, `a2d lift manifest`, `score --expect-head`, `pool --claim`. It confirmed that pooling offsets replicate indices, so two r = 1 runs do not collide.
2. **Test a binary pin before committing to it.** Debug builds are not byte-reproducible: an identical-source rebuild changed the sha. Pin by source instead (the controller already refuses if `crates/` differs from the registration commit) and log the binary sha without enforcing it.
3. **Run the runner against its own registration commit**, with a call guaranteed to defer on the daily cap (no attempt record, no model call). It caught a real bug: `git show <sha>:<path>` resolves paths from the git top level, not the subproject root. Use `git -C <root> show <sha>:./<path>`. The fix sat inside the bound runner section, so the run was re-registered at the next commit (78727f29), which cost nothing because nothing had been spent.

## Practice

- A registration is code. Test it end to end against itself before the first irreversible call, through the real entry point and the real git state.
- Bind the runner to the registration: read settings from it, verify the commit, refuse to overwrite anything, and write an attempt record before the first call.
- Every refusal fails closed but has a documented manual recovery path; see the relaunch procedure in `docs/HANDOFF.md`.
- Get two independent reviews; neither one alone caught everything.
- Power-plan the rule as implemented, for every outcome class, and record any shortfall before the first call.

See also: `power-plan-the-implemented-decision-rule-2026-10-14.md`, `static-guards-are-not-isolation-use-os-sandbox-2026-10-16.md` (frozen controller paths force re-registration).
