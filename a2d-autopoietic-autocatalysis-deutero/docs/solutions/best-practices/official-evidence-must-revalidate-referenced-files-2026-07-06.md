---
module: senior-swe-bench
tags: [evidence, provenance, official-evaluator, path-containment]
problem_type: verifier-gap
---

# Official Evidence Must Revalidate Referenced Files

## Problem

Serialized `a2d.fitness-evidence.v1` provenance can look complete while the referenced official evaluator manifest or inspection sidecar is missing, stale, outside the project, or replaced after evidence export.

## Principle

Official Senior SWE-Bench evidence must not trust path/hash metadata by itself. The verifier must re-read the referenced files, recompute their hashes, parse the manifest, revalidate the inspection sidecar, and ensure referenced paths are contained under the A²D project root.

## Practice

- Resolve relative official evidence paths from `a2d_project_root()`, never from the process CWD.
- Reject lexical `..` traversal before filesystem access.
- Canonicalize the project root and candidate paths to reject symlink escapes.
- Reject absolute official evidence paths outside the A²D project root.
- Recompute `git hash-object` for both manifest and inspection files and compare with evidence fields.
- Re-parse the official manifest and re-run sidecar validation against task/repo/command before accepting the evidence.

## Evidence

Implemented in the 2026-07-06 verifier slice. Fresh source-patch evidence: `runs/20260706-official-evidence-verifier-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json` (`source_diff_hash: ae8a6b2c6618b331c9b3d711c0560394d668548b`).

This is provenance hardening only; it is not official Senior SWE-Bench mastery.
