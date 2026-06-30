# Comparison Fitness Evidence Export — 2026-06-30

## Purpose

Validate that non-persistent comparison modes (`compare-topologies` and `compare-provider-policy`) use the same canonical `fitness_report` artifact produced by `Metabolism::run_cycle()` / `challenge.scoring_benchmark()` and can export auditable `a2d.fitness-evidence.v1` evidence without inventing evidence-shaped JSON.

## Topology comparison command

```bash
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260630-topology-fitness-evidence \
A2D_PROVIDER_TIMEOUT_SECS=90 \
A2D_MAX_CYCLE_SECS=120 \
cargo run -p a2d -- compare-topologies sudoku 1 \
  2>&1 | tee /tmp/a2d-compare-topologies-sudoku1-fitness-evidence-export-20260630-r2.log
```

## Topology result

Both legs ran the real challenge-scoring path with persistence disabled:

- `seed`: 100% (6/6), full fitness at cycle 1, exported `runs/20260630-topology-fitness-evidence/seed-sudoku-solver-cycle-0-fitness-evidence.json`
- `evolved`: 100% (6/6), full fitness at cycle 1, exported `runs/20260630-topology-fitness-evidence/evolved-sudoku-solver-cycle-0-fitness-evidence.json`

Both artifacts have SHA-256 `6aa4f715aaa5dd155371519737ff569c3deb0233a01a18cc263e9ec0e2c62abe` and validate as:

- `schema_version: a2d.fitness-evidence.v1`
- `actual_tests_evaluated: true`
- `cycle: 0`
- `non_regressing: true`
- `fitness: 1.0`
- `failed: 0`
- `all_tests_pass: true`
- no failed cases

## Provider-policy comparison command

```bash
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260630-provider-policy-fitness-evidence \
A2D_PROVIDER_TIMEOUT_SECS=90 \
A2D_MAX_CYCLE_SECS=120 \
cargo run -p a2d -- compare-provider-policy sudoku 1 \
  2>&1 | tee /tmp/a2d-compare-provider-policy-sudoku1-fitness-evidence-export-20260630.log
```

## Provider-policy result

This was a no-policy-delta smoke (`current` and `proposed` policy were identical), so it is evidence for export plumbing, not for a durable provider-policy change. Stochastic provider output differed across legs:

- `current`: 67% (4/6), exported `runs/20260630-provider-policy-fitness-evidence/current-sudoku-solver-cycle-0-fitness-evidence.json`, SHA-256 `bbf8547e01ffeba66022f336f0e8b017578732f0f175e82b8cb80d91fd4c4a4f`, `all_tests_pass: false`
- `proposed`: 100% (6/6), exported `runs/20260630-provider-policy-fitness-evidence/proposed-sudoku-solver-cycle-0-fitness-evidence.json`, SHA-256 `6aa4f715aaa5dd155371519737ff569c3deb0233a01a18cc263e9ec0e2c62abe`, `all_tests_pass: true`

The gate printed ACCEPT because proposed was non-regressing within the bounded comparison, but there was no assignment delta and no durable provider-policy commit.

## Validation predicate

```bash
jq -e 'keys_unsorted == ["actual_tests_evaluated","cycle","delta_from_last_non_regressing_fitness","diagnostic_present","failed","failed_cases","fitness","non_regressing","passed","results","schema_version","total"] and .schema_version=="a2d.fitness-evidence.v1" and .actual_tests_evaluated==true and .cycle==0 and .non_regressing==true and (.results | map(.name) | index("all_tests_pass")) and (.results | all((keys_unsorted == ["name","passed"]) and (.name == "hidden_acceptance" or .name == "compiles" or .name == "has_tests" or .name == "all_tests_pass" or (.name|startswith("has_"))))) and (.failed_cases | all(. == "hidden_acceptance" or . == "compiles" or . == "has_tests" or . == "all_tests_pass" or startswith("has_")))' \
  runs/20260630-topology-fitness-evidence/*.json \
  runs/20260630-provider-policy-fitness-evidence/*.json
```

Output: `true` for all four artifacts.

## Caveat

The topology run provides fresh full-passing actual-test evidence for the comparison-export plumbing (`all_tests_pass: true`, no hidden-specific case names leaked). The provider-policy smoke includes one failing leg and no policy delta, so it should not be used as evidence for a durable provider-policy change.
