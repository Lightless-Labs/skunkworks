---
module: senior-swe-bench
tags: [retry, evidence, path-resolution, cwd-stability]
problem_type: verifier-gap
---

# Retry Status Final Evidence Paths Must Use Artifact Resolution

## Problem

A successful retry execution can serialize `final_evidence_path` as a project-relative retry artifact path. If `senior-swe-bench-retry-status` later reads that string directly with the process CWD, the same persisted retry execution can validate from the project root but fail from a subdirectory.

## Principle

Persisted retry artifact paths are data with A²D retry semantics, not arbitrary CWD-relative paths. Status validation must resolve them the same way other retry handoffs do before reading and inspecting evidence.

## Practice

- Serialize in-project retry artifacts as project-relative paths.
- Resolve persisted retry paths with `resolve_retry_artifact_path` before file access.
- Exercise status from a non-root CWD in integration tests when accepting a project-relative final evidence path.
- Keep `fitness-evidence-inspect --require-all-tests-pass` as the authoritative success gate after resolution.

## Evidence

Implemented in the 2026-07-06 retry-chain smoke/status path slice. Fresh source-patch evidence: `runs/20260706-retry-chain-smoke-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json` (`source_diff_hash: a53e71d5418002600df0cd8c44e92ae7028f5b76`).

This is retry-chain/status path hardening only; it is not official Senior SWE-Bench mastery.
