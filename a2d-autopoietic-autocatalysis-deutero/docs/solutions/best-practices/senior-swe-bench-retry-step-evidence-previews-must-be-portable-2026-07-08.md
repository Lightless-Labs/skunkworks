---
module: a2d-cli
tags: [senior-swe-bench, retry, fitness-evidence, portability, provenance]
problem_type: evidence-portability
---

# Senior SWE-Bench retry step-evidence previews must be portable

## Problem

A fresh live-provider retry smoke produced valid structured `a2d.fitness-evidence.v1` and repo-relative retry/evaluation artifacts, but the diagnostic `fitness_evidence_inspect_stdout_preview` copied stdout from `fitness-evidence-inspect`, including the absolute path it printed for the inspected evidence file.

The preview is non-authoritative, but committing host-local paths in retry artifacts makes otherwise portable evidence harder to review and reuse.

## Decision

`senior-swe-bench-retry-attempt-step-evidence` now serializes only that bounded stdout preview through `portable_preview_text_lossy`, which rewrites the A²D project root and temp directory prefixes.

Structured fields (`fitness_evidence_path`, candidate patch paths, selected artifact paths, and `fitness_evidence_summary`) remain unchanged and parseable for validation. The evidence gate still re-runs `fitness-evidence-inspect`, re-reads the evidence JSON, and binds candidate patch/artifact hashes before permitting a local-wrapper success claim.

## Evidence

Fresh source/live-smoke evidence: `runs/20260708-live-provider-retry-smoke-head-867c327/attempt-0/fitness/senior-swe-bench-firezone-fix-connlib-align-device-hard-cycle-0-fitness-evidence.json`, full-passing `a2d.fitness-evidence.v1`, `source_diff_hash: 5f4408beb1d62447d57f1f680e3cd58fcb61d4e5`.

Validation included focused retry-attempt-step-evidence coverage, full `CARGO_BUILD_JOBS=2 cargo test`, independent review, `fitness-evidence-inspect --require-all-tests-pass`, and a run-directory host-local prefix scan.

This is local-wrapper retry portability hardening and a bounded live-provider smoke. It is not official Senior SWE-Bench mastery, hidden official holdout proof, or OS/network no-egress proof.
