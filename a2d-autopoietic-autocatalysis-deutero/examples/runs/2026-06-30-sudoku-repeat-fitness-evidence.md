# Sudoku Repeat Fitness Evidence — 2026-06-30

## Purpose

Run repeated bounded Sudoku challenge smokes through the live `a2d challenge` path with `A2D_FITNESS_EVIDENCE_EXPORT_DIR` enabled. This checks whether the current seed metabolism repeatedly reaches full fitness with fresh `a2d.fitness-evidence.v1` actual-test evidence, instead of relying on older log-only `sudoku 3` / `sudoku 5` claims.

Lineage search before this run found that older full-fitness references were mostly logs or handoff excerpts, not tracked structured evidence. The newest structured evidence before this run was:

- `runs/20260629-fitness-evidence*`: challenge/feedback export plumbing, but `all_tests_pass: false` / 67%.
- `runs/20260630-topology-fitness-evidence/*`: comparison-export plumbing with full-passing seed/evolved legs.

## Command template

Each replica used the seed germline, a 90s provider timeout, a 120s cycle wall-clock cap, and a unique export directory:

```bash
A2D_GERMLINE=seed \
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260630-sudoku-repeat-evidence/rN \
A2D_PROVIDER_TIMEOUT_SECS=90 \
A2D_MAX_CYCLE_SECS=120 \
cargo run -p a2d -- challenge sudoku 1 \
  2>&1 | tee /tmp/a2d-sudoku-repeat-rN-20260630.log
```

## Results

| Replica | Provider | Fitness | `all_tests_pass` | Evidence path | SHA-256 |
|---|---|---:|---|---|---|
| r1 | DeepSeek v4 flash | 67% (4/6) | false | `runs/20260630-sudoku-repeat-evidence/r1/sudoku-solver-cycle-0-fitness-evidence.json` | `bbf8547e01ffeba66022f336f0e8b017578732f0f175e82b8cb80d91fd4c4a4f` |
| r2 | DeepSeek v4 flash | 100% (6/6) | true | `runs/20260630-sudoku-repeat-evidence/r2/sudoku-solver-cycle-0-fitness-evidence.json` | `6aa4f715aaa5dd155371519737ff569c3deb0233a01a18cc263e9ec0e2c62abe` |
| r3 | DeepSeek v4 flash | 100% (6/6) | true | `runs/20260630-sudoku-repeat-evidence/r3/sudoku-solver-cycle-0-fitness-evidence.json` | `6aa4f715aaa5dd155371519737ff569c3deb0233a01a18cc263e9ec0e2c62abe` |
| r4 | Kimi k2p6 | 100% (6/6) | true | `runs/20260630-sudoku-repeat-evidence/r4/sudoku-solver-cycle-0-fitness-evidence.json` | `6aa4f715aaa5dd155371519737ff569c3deb0233a01a18cc263e9ec0e2c62abe` |
| r5 | Kimi k2p6 | 100% (6/6) | true | `runs/20260630-sudoku-repeat-evidence/r5/sudoku-solver-cycle-0-fitness-evidence.json` | `6aa4f715aaa5dd155371519737ff569c3deb0233a01a18cc263e9ec0e2c62abe` |

All five artifacts validate as `schema_version: a2d.fitness-evidence.v1`, `actual_tests_evaluated: true`, `cycle: 0`, and `non_regressing: true`. Four of five replicas are full-passing; r1 failed public aggregate checks `all_tests_pass` and `has_tests`.

## Interpretation

This is fresh non-regressing actual-test evidence that the seed challenge path can repeatedly solve Sudoku under the current provider portfolio (4/5 one-cycle replicas full-passing). It is not yet enough to claim parity with the documented one-shot baselines, which were 100% in their recorded runs. The remaining gap is reliability/noise: the same bounded setup still produced one 67% artifact.

No source patch, provider-policy change, mutation, or self-improvement was accepted from these runs. The live `a2d challenge` command did write lineage commits for each observed cycle, but every cycle had a matching fresh evidence export and reported `0 mutations`, `0 patches`, and `0 provider policy changes`.
