---
module: a2d-cli
tags: [fitness-evidence, provenance, evidence-gating, self-improvement]
problem_type: best-practice
---

# Fitness evidence source revisions must be full object IDs

## Problem

`source_diff_hash` already used a full git object ID, but `source_revision` was emitted with `git rev-parse --short HEAD:<scope>`. Short revisions are human-friendly, but they weaken machine provenance: they can become ambiguous as the repository grows and make stale evidence harder to distinguish from display-only summaries.

## Pattern

Evidence that gates source persistence should record the full git object ID for `source_revision`, and validators should reject short or malformed values before comparing with the current scoped source revision.

This applies to:

- exported `a2d.fitness-evidence.v1` inspection;
- autopilot source-fitness evidence validation;
- Senior SWE-Bench local evaluation and retry evidence validation;
- test fixtures that fabricate current `crates` provenance.

Historical run artifacts may still contain short revisions; they remain lineage, not current persistence-ready evidence. Fresh evidence must use the full 40-character object ID and must still match the current scoped `crates` revision and `source_diff_hash`.

## Evidence

Implemented in `crates/a2d-cli/src/main.rs` and updated fixtures in `crates/a2d-cli/tests/`. Regression coverage now asserts full-length `source_revision`, rejects short revisions, and rejects a well-formed stale `HEAD^:<crates>` revision when available.

Fresh source-patch evidence: `runs/20260708-source-revision-full-hash-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, full-passing `a2d.fitness-evidence.v1`, `source_revision` length 40, `source_diff_hash: 8bd947344234a680de3d3032c28ab90f40e632d0` matching the current scoped crates diff.
