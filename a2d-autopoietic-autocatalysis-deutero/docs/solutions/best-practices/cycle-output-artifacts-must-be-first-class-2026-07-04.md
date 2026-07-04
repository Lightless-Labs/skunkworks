---
module: a2d-cli
tags: [cycle-input, senior-swe-bench, evidence, artifact-provenance]
problem_type: best-practice
---

# Cycle output artifacts must be first-class

## Context

After `a2d cycle-input` could seed Senior SWE-Bench task bundles, a live smoke showed a provider could produce a `code` artifact without any durable file path for the next gate (`senior-swe-bench-extract-patch`). stdout summaries were not enough: downstream evaluation needs exact bytes and hashes.

## Practice

When a cycle is used as part of an evaluation pipeline, persist materialized outputs with a manifest:

- write exact artifact bytes to files;
- record cycle/workcell/enzyme/provider/artifact type;
- record a `git hash-object` content hash;
- accumulate all cycles in one manifest;
- fail closed on collisions or pre-existing manifests.

## Why it matters

A captured artifact is still not fitness evidence. But without exact captured bytes, the extraction/evaluator path cannot be mechanically bound to what the provider actually produced. The manifest creates the missing bridge while keeping the existing fail-closed boundaries: prose-only output is diagnosed before patch extraction and cannot become Senior SWE-Bench evidence.

When consuming the manifest later, add a deterministic selector that:

- requires exactly one coder/code candidate instead of guessing;
- re-reads artifact bytes and verifies byte count plus `git hash-object` hash;
- rejects public GitHub solution references before extraction, even if a valid diff is present;
- emits extraction args and diagnosis only, not fitness evidence.

## Evidence

- Source gate for artifact capture: `runs/20260704-cycle-output-artifacts-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, `source_diff_hash: ed02b5802a540a7800696e8c8110277afd471cda`.
- Live artifact manifest: `runs/20260704-cycle-output-artifacts-evidence/live-cycle/artifacts/manifest.json`.
- Extraction rejection: `runs/20260704-cycle-output-artifacts-evidence/extract/extract.err`.
- Run doc: `examples/runs/2026-07-04-cycle-output-artifacts.md`.
- Source gate for deterministic artifact selection: `runs/20260704-senior-swe-bench-candidate-artifact-selection-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, `source_diff_hash: 5c973a79c75109b52d0b6e1349e9ceba84c3b6a9`.
- Selection run doc: `examples/runs/2026-07-04-senior-swe-bench-candidate-artifact-selection.md`.
