# Senior SWE-Bench live self-iteration smoke — 2026-07-09

## Purpose

Run A²D against a Senior SWE-Bench task through the existing provider/retry/evaluator gates, with no public GitHub solution search, and verify whether a failed candidate can feed a second provider attempt.

## Summary

Run directory: `runs/20260709-senior-swe-bench-live-self-iteration-goal-20260708234448-ocqbbp/`.

Task: `firezone-fix-connlib-align-device-hard` (`firezone/firezone`).

Attempt 0 invoked `cycle-input --checkout --output-artifacts` and produced an extractable but checkout-inapplicable candidate patch. Retry execution persisted a mechanical evaluation-error artifact and generated `next-cycle-input.json` feedback without starting the evaluator or claiming fitness.

Attempt 1 ran the persisted next-cycle boundary and produced a corrected candidate patch. The local evaluator wrapper applied it in an isolated checkout and exported fresh `a2d.fitness-evidence.v1`.

## Evidence

Authoritative local-wrapper evidence:

`runs/20260709-senior-swe-bench-live-self-iteration-goal-20260708234448-ocqbbp/attempt-1-fitness-direct2/senior-swe-bench-firezone-fix-connlib-align-device-hard-cycle-0-fitness-evidence.json`

Inspection passed:

```bash
cargo run -q -p a2d -- fitness-evidence-inspect \
  runs/20260709-senior-swe-bench-live-self-iteration-goal-20260708234448-ocqbbp/attempt-1-fitness-direct2/senior-swe-bench-firezone-fix-connlib-align-device-hard-cycle-0-fitness-evidence.json \
  --require-all-tests-pass
```

Result: `actual_tests_evaluated: true`, `non_regressing: true`, `fitness: 1.0`, `passed/total: 3/3`, `hidden_acceptance: true`, `evaluator_kind: provided_local_command`.

Scoped crates source hash: `d3da1fc5fbb40b2695cf523754d5af10c4aa2ec0`, matching `git diff --binary HEAD -- crates | git hash-object --stdin`.

## Code change exercised

- Retry execution now converts evaluation-wrapper/preflight failures into bounded next-cycle feedback when retry budget remains, instead of stopping before self-correction.
- Candidate patch preflight/application uses `git apply --recount` so semantically valid model diffs with stale hunk counts can still be judged by checkout/evaluator gates.

## Scope

This is a live provider self-iteration smoke over a provided local evaluator and synthetic checkout. It is not official Senior SWE-Bench mastery, not official hidden-holdout proof, and not OS/network no-egress proof.
