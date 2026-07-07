---
module: a2d-cli
tags: [senior-swe-bench, retry, fitness-evidence, artifact-paths, cwd-stability]
problem_type: portability
---

# Retry fitness-evidence paths must be CWD-stable

Senior SWE-Bench retry artifacts are consumed across multiple commands and sometimes from different working directories. Persisted `fitness_evidence_path` values therefore cannot rely on the producer's process CWD.

## Failure mode

`senior-swe-bench-evaluate` exported a valid `a2d.fitness-evidence.v1` file, but serialized its path with direct `Path::to_string_lossy()`. For in-project artifacts this leaked host-local absolute project-root paths into local-evaluation/retry artifacts. For outside-project relative export dirs, the inverse bug appears: persisting `fitness/...` is not CWD-stable when a later retry gate runs from the repo root or a crate subdirectory.

## Pattern

Serialize retry artifact paths through retry semantics:

- in-project artifacts should be repo-relative for portability and to avoid host-local path leakage;
- outside-project artifacts should be absolute so later gates resolve the same file from any CWD;
- consumers should resolve persisted paths through the same retry artifact resolver before spawning `fitness-evidence-inspect` or reading evidence bytes.

## Evidence

Implemented in `crates/a2d-cli/src/main.rs`. Regression coverage:

- `retry_attempt_evaluate_serializes_in_project_fitness_evidence_path_repo_relative`
- `retry_attempt_evaluate_serializes_external_relative_fitness_evidence_path_as_absolute`
- retry-status tamper tests now resolve repo-relative final evidence paths before mutation.

Fresh source-patch evidence: `runs/20260707-retry-fitness-evidence-path-hardening-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, full-passing `a2d.fitness-evidence.v1`, `source_diff_hash: 6eac6834ac85a53a12842a232c0fb35b7f00bb89`, matching the scoped crates diff.

This is path portability hardening only. It is not official Senior SWE-Bench mastery or no-egress proof.
