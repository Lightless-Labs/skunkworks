---
title: "Close a Known Leak Channel Before Spending a Once-Only Set; an Audit Afterwards Only Tells You It Is Gone"
date: 2026-10-18
category: best-practices
module: nano/eval.py (confirm-web-audit) / a2d-providers / registration process
problem_type: experiment_design
component: confirmation-sets
symptoms:
  - "An open tool channel was found one day before a once-only confirmation set was spent"
  - "The fix was deferred behind a code freeze; a post-hoc count-only audit was registered instead"
  - "The first replicate's audit found 2 web searches in a comparator arm, so the set was spent with no claim"
root_cause: deferred_instrument_fix_justified_by_a_zero_base_rate
---

# Close a Known Leak Channel Before Spending a Once-Only Set

**Evidence:** A²D LOG, cycle 2: "C2-WEB" (2026-10-17) and "C2-CONFIRM2 replicate 1: … IntegrityReview" (2026-10-18). Related: `deprecated-cli-flag-silently-reenables-a-tool-2026-10-17.md`, `blind-holdout-authoring-pipeline-2026-10-14.md`.

## What happened
- **10-17, before any confirmation call:** codex-cli 0.162.0 had web search on by default and ignored the disable flag (C2-WEB). The controller's tool count could not see searches.
- **The fix was deferred:** the registered instrument (crates at 32e72a53) was frozen until `decision.json`, so the provider fix waited on a branch.
- **What was registered instead:** a strictly tightening amendment.
  - A count-only web audit runs after each replicate.
  - Any web-search event in any arm, in either replicate, makes the outcome IntegrityReview.
  - It was justified by the record: 0 searches in about 15.8k earlier transcripts ("at low or medium effort the models never used it").
- **10-18, replicate 1:** arm B, a comparator rather than the tested harness, made 2 searches in 720 transcripts. D had 0 in 185 and X:frozen 0 in 713.
  - The outcome became IntegrityReview, so confirmation-2 is spent with no claim.
  - A SUPPORTED claim now needs a newly authored blind set.

## What went right
- **The rule was written before the spend, and applied as written.**
  - It was not narrowed to "only B searched" or "only 2 of 720".
  - Nothing was examined to learn what was searched: that would mean opening the set. The cause stays unknown by design.
- **The audit was count-only,** so applying it leaked nothing.

## The lesson (about the decision, not the base rate)
- **A zero base rate does not license spending a once-only set through a known open channel.**
  - The 15.8k calls came from other tasks and prompts.
  - A rare event across about 1.6k calls per replicate is still likely enough to cost the set.
- **A freeze protects the registered instrument; it is no reason to defer a known integrity fix.**
  - The right move was to fix the channel, re-register the fixed instrument and accept the re-review, all before the first call.
- **An audit after the spend can only tell you the set is gone.** Use audits as a second check behind a closed channel, never as the only control.

## How to apply (before spending any once-only set)
1. **Enumerate every channel the model CLI exposes by default:** web, fetch, MCP, shell, file and image tools. Do this again after every CLI upgrade.
2. **Close each one, and check it with a live probe** that asks the model to use it. A flag the CLI accepts is not proof.
3. **Make the controller's integrity check count each channel,** so `integrity_ok` fails on any use, not just `exec`.
4. **If the instrument is frozen, re-register the fixed instrument** and repeat the reviews. Don't spend the set on the old one.
5. **Keep the count-only audit as a second check.** Pre-register that any event voids the claim, then apply that rule as written.
