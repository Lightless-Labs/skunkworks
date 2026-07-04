---
title: "Diagnose Provider Artifacts Before Context Enrichment"
date: 2026-07-04
category: best-practices
module: senior-swe-bench
problem_type: benchmark_integration
component: cycle-output-artifacts
severity: medium
tags:
  - senior-swe-bench
  - artifact-contract
  - evidence-gates
  - no-solution-search
---

# Diagnose Provider Artifacts Before Context Enrichment

## Problem

A live Senior SWE-Bench `cycle-input` smoke captured exact provider output, but patch extraction rejected it because the artifact was prose: "I'll inspect the local checkout...".

That observation is ambiguous. It could mean the task bundle lacks usable checkout context, or it could mean the provider ignored the requested unified-diff output contract. Adding more context before classifying the failure risks solving the wrong problem.

## Resolution

Add a diagnostic-only CLI layer:

```text
a2d senior-swe-bench-diagnose-artifact <artifact|->
```

The command emits `a2d.senior-swe-bench-artifact-diagnosis.v1` JSON with:

- whether a unified diff candidate patch is extractable,
- whether public GitHub solution references appear,
- a bounded preview, redacted if public GitHub references are present,
- `failure_kind` values such as `checkout_context_not_exercised`, `output_contract_not_followed`, `public_solution_reference`, or `candidate_patch_extractable`.

The extractor/evaluator gates remain authoritative. Diagnosis is not fitness evidence and cannot support a benchmark mastery claim.

## Validation

- Focused tests cover checkout-deferral prose, extractable diffs, mixed-case public GitHub references, and redacted previews.
- `senior-swe-bench-extract-patch` now rejects mixed-case GitHub references case-insensitively.
- Fresh source-patch evidence: `runs/20260704-senior-swe-bench-artifact-diagnosis-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, `source_diff_hash: 1d526318a6426ba61cc79bda7c6c5c04a5867397`.
- Diagnostic output for the prior prose artifact: `runs/20260704-senior-swe-bench-artifact-diagnosis-evidence/diagnosis/prose-artifact-diagnosis.json`.

## Follow-up

The current classified failure is `checkout_context_not_exercised`, so the next Senior SWE-Bench integration slice should verify/provide usable local checkout context without weakening the no-public-solution-search policy or the artifact-only provider boundary.
