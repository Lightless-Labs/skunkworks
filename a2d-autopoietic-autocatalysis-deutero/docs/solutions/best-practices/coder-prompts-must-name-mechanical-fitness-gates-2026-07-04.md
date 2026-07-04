---
title: "Coder Prompts Must Name Mechanical Fitness Gates"
date: 2026-07-04
module: cli
tags:
  - fitness-evidence
  - prompt-contracts
  - hidden-holdouts
  - benchmark-reliability
problem_type: best-practice
---

# Coder Prompts Must Name Mechanical Fitness Gates

## Problem

Repeated seed Sudoku evidence showed a reliability failure that was not a parser API miss: `runs/20260630-sudoku-repeat-evidence/r1/sudoku-solver-cycle-0-fitness-evidence.json` compiled and exposed the required public functions, but failed the public `has_tests` gate because the generated artifact omitted `#[cfg(test)] mod tests`.

A generic instruction to "include tests" was not enough for a stochastic coder path. The system's mechanical gate has a specific public predicate, so the coder contract should name it explicitly.

## Practice

When a public, non-hidden fitness gate checks a concrete artifact shape, the generation prompt should name the exact required shape and the consequence of omission. For Rust-source deliverables, the seed coder prompt now requires the exact `#[cfg(test)] mod tests` header, at least three tests, and meaningful coverage of normal, edge/invalid, and end-to-end behavior. It explicitly states that omission fails the mechanical `has_tests` fitness gate.

This does not leak hidden holdouts: `has_tests` is a public aggregate/scaffolding gate already present in exported `a2d.fitness-evidence.v1`.

## Validation

Fresh generated challenge evidence after the prompt change:

- `runs/20260704-sudoku-test-prompt-evidence/challenge-seed-r1/sudoku-solver-cycle-0-fitness-evidence.json`
- `schema_version: a2d.fitness-evidence.v1`
- `actual_tests_evaluated: true`
- `non_regressing: true`
- `fitness: 0.8333333333333334`
- `has_tests: true`
- `failed_cases: ["all_tests_pass"]`
- `source_diff_hash: d44224bb8edd9d79608bb2b2646b00867f4cf4f1`

Fresh full-passing replay/source-patch gate evidence:

- `runs/20260704-sudoku-test-prompt-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`
- `fitness: 1.0`
- `failed_cases: []`
- same `source_diff_hash`

The generated challenge evidence proves the specific `has_tests` compliance gap is closed in this run; it does not claim Sudoku mastery because `all_tests_pass` still failed.
