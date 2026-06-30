# Fitness Evidence Export Live Smoke — 2026-06-29

## Purpose

Live-inspect a freshly produced `a2d.fitness-evidence.v1` artifact from the real `a2d challenge` path, including hidden-holdout status and non-regression metadata.

## Command

```bash
A2D_GERMLINE=seed \
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260629-fitness-evidence \
A2D_PROVIDER_TIMEOUT_SECS=120 \
A2D_MAX_CYCLE_SECS=180 \
cargo run -p a2d -- challenge sudoku 1 \
  2>&1 | tee /tmp/a2d-sudoku1-fitness-evidence-export-20260629-r2.log
```

## Result

- Provider: `opencode/opencode/deepseek-v4-flash-free`
- Fitness: 67% (4/6)
- Fresh exported evidence: `runs/20260629-fitness-evidence/sudoku-solver-cycle-0-fitness-evidence.json`
- Lineage challenge commit: `.a2d/lineage` commit `f110d29` (`Cycle 0: 1 invocations, 0 accepted, 0 rejected, RAF 100%, Fitness 67% (4/6)`)

## Evidence Inspection

The exported artifact has:

- `schema_version: a2d.fitness-evidence.v1`
- `actual_tests_evaluated: true`
- `cycle: 0`, matching the zero-based runtime cycle in the filename
- `non_regressing: true`
- `delta_from_last_non_regressing_fitness: 0.6666666666666666`
- hidden-holdout aggregate status through `all_tests_pass: false`
- no leaked non-public case names; failed cases are public labels: `all_tests_pass`, `has_tests`

Validation predicate:

```bash
jq -e 'keys_unsorted == ["actual_tests_evaluated","cycle","delta_from_last_non_regressing_fitness","diagnostic_present","failed","failed_cases","fitness","non_regressing","passed","results","schema_version","total"] and .schema_version=="a2d.fitness-evidence.v1" and .actual_tests_evaluated==true and .cycle==0 and .non_regressing==true and (.delta_from_last_non_regressing_fitness>=0) and (.results | map(.name) | index("all_tests_pass")) and (.results | all((keys_unsorted == ["name","passed"]) and (.name == "hidden_acceptance" or .name == "compiles" or .name == "has_tests" or .name == "all_tests_pass" or (.name|startswith("has_"))))) and (.failed_cases | all(. == "hidden_acceptance" or . == "compiles" or . == "has_tests" or . == "all_tests_pass" or startswith("has_")))' \
  runs/20260629-fitness-evidence/sudoku-solver-cycle-0-fitness-evidence.json
```

Output: `true`.

## Multicycle Feedback Export Follow-up

A second live smoke covered feedback cycles that consume fresh previous-cycle evidence without producing new code in the current cycle:

```bash
A2D_GERMLINE=seed \
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260629-fitness-evidence-multicycle \
A2D_PROVIDER_TIMEOUT_SECS=90 \
A2D_MAX_CYCLE_SECS=120 \
cargo run -p a2d -- challenge sudoku 2 \
  2>&1 | tee /tmp/a2d-sudoku2-fitness-evidence-export-20260629-r3.log
```

Artifacts:

- `runs/20260629-fitness-evidence-multicycle/sudoku-solver-cycle-0-fitness-evidence.json`
- `runs/20260629-fitness-evidence-multicycle/sudoku-solver-cycle-0-consumed-by-cycle-1-fitness-evidence.json`

Both artifacts validate as `a2d.fitness-evidence.v1`, `actual_tests_evaluated: true`, `cycle: 0`, `non_regressing: true`, nonnegative delta, and contain only reviewed public/aggregate case labels. The second filename explicitly states that cycle 1 consumed the fresh cycle 0 evidence rather than producing new benchmark evidence. A reviewer found that provider-produced `fitness_report` outputs must not be trusted as prior evidence; the selector now accepts only the current artifact store or lineage inputs and has a regression test rejecting fabricated provider output evidence.

## Notes

This is live actual-test evidence for the export/inspection path and the structured evidence schema, not evidence that Sudoku is solved perfectly in these runs. The hidden holdout aggregate failed (`all_tests_pass: false`), so the next benchmark objective remains reaching full fitness repeatedly under this evidence-export path.
