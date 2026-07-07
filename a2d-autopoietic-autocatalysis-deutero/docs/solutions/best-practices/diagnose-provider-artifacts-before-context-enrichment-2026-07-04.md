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

## 2026-07-07 public GitHub reference detector follow-up

Do not restrict solution-reference detection to browser-style `github.com/` URLs. Provider artifacts can cite public GitHub in SSH remote or ref notation, such as `git@github.com:org/repo.git` or `refs/pull/123/head`, without containing the original URL substrings. The shared detector now treats any `github.com`, `/pull/`, `/commit/`, `/issues/`, or `refs/pull` occurrence as a public solution-reference indicator. Diagnosis redacts previews for these forms, and candidate-artifact selection rejects them even when the artifact otherwise contains a valid unified diff.

TDD baseline checks with only the tests changed failed against the old detector, then passed after widening the detector. Fresh source-patch evidence: `runs/20260707-senior-swe-bench-public-github-reference-detection-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, full-passing `a2d.fitness-evidence.v1` with `source_diff_hash: 195c9ca41f615e64a8a8fedbe486183b8a48ddca`. `hidden_acceptance: not_present` is expected for this local source-patch gate and must not be cited as official Senior SWE-Bench hidden-holdout evidence.

## 2026-07-07 raw/obfuscated GitHub host follow-up

Do not stop at canonical `github.com` host spellings. Provider artifacts can cite public GitHub material through raw content URLs or common obfuscated host forms such as `raw.githubusercontent.com/...`, `github[.]com/...`, `github dot com/...`, or `github . com/...`. These are still public solution-reference indicators in the Senior SWE-Bench no-public-solution-search context.

The shared detector now rejects `githubusercontent.com`, `github[.]com`, `github dot com`, and `github . com` in addition to prior GitHub URL/ref forms. Extractor unit coverage rejects these forms before candidate patch materialization; diagnosis coverage redacts previews; candidate-selection coverage rejects valid-looking diffs containing these references before extraction/evaluation. TDD baseline checks with only the tests changed failed against the previous detector, then passed after widening it.

Fresh source-patch evidence: `runs/20260707-senior-swe-bench-githubusercontent-obfuscation-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, full-passing `a2d.fitness-evidence.v1` with `source_diff_hash: 175282b7c7368b956557c63567b86f98a57ad79a`. The evidence command scored the tracked good Sudoku artifact to bind the current A²D source diff to actual tests; `hidden_acceptance: not_present` remains expected for this local source-patch gate and is not official Senior SWE-Bench hidden-holdout evidence.

## 2026-07-07 GitHub CLI command reference follow-up

Provider artifacts can cite public solution lookup without writing a browser URL, for example by saying to use `gh pr view`, `gh api repos/.../pulls/...`, or `hub pr checkout`. Those are still public GitHub solution-reference indicators for Senior SWE-Bench. The shared detector now tokenizes artifact text and rejects reviewed `gh`/`hub` solution-search subcommands (`api`, `pr`, `issue`, `repo`, `search`, `browse`, `clone`) before extraction/evaluation.

Keep the detector bounded: negative coverage proves ordinary `GH`/`PR` prose fragments such as a high-priority PR review note are not enough to fail an otherwise extractable diff. Fresh source-patch evidence: `runs/20260707-senior-swe-bench-github-cli-reference-detection-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, full-passing `a2d.fitness-evidence.v1` with `source_diff_hash: 1d45de32464f9e58a5fbd7dcdd4384ad594d01fa`. This is artifact-safety evidence only, not official benchmark mastery or network no-egress proof.

## 2026-07-07 percent-encoded GitHub reference follow-up

Provider artifacts can hide public GitHub references behind URL percent encoding, for example `github%2ecom/...` or `refs%2fpull%2f123%2fhead`. These are still public solution-reference indicators in the Senior SWE-Bench no-public-solution-search context. The shared detector now decodes ASCII `%XX` sequences once and applies the existing host/ref/CLI checks to the decoded view before extraction or evaluation.

Fresh source-patch evidence: `runs/20260707-senior-swe-bench-percent-encoded-github-reference-detection-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, full-passing `a2d.fitness-evidence.v1` with `source_diff_hash: 2eb9a3e509b8442569b4fab43c56603bb7b363dd`. Post-commit clean-HEAD evidence: `runs/20260707-postcommit-fitness-evidence-90e6ffe/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, full-passing with clean crates diff hash `e69de29bb2d1d6434b8b29ae775ad8c2e48c5391`. This is artifact-safety evidence only, not official benchmark mastery or network no-egress proof.

## 2026-07-07 nested percent-encoding follow-up

Single-pass percent decoding is insufficient for the Senior SWE-Bench no-public-solution-search boundary: `github%252ecom` decodes only to `github%2ecom`, and `refs%252fpull...` decodes only to `refs%2fpull...`. The shared detector now performs bounded iterative ASCII percent decoding with an eight-pass cap, checks after every pass, lowercases decoded text after each pass, and treats still-changing over-depth encodings as suspicious when the current or next decoded form carries public GitHub/ref structure. This preserves a negative case for harmless deep local `issue` metadata while closing double/triple/over-depth GitHub host/ref bypasses. Candidate-selection boundary coverage pins depth-8 public host/ref forms as detected at the cap and depth-9 public host/ref forms as rejected by the bounded fallback, including fully percent-encoded forms where `github`/`refs` appears only after one more decode, while accepting depth-9 local metadata.

Fresh source-patch evidence: `runs/20260707-senior-swe-bench-double-percent-encoded-reference-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, full-passing `a2d.fitness-evidence.v1` with `source_diff_hash: d68df156ee6531f78c71fe7eb78db107f77c1249`. This is detector safety evidence only, not official benchmark mastery or network no-egress proof.
