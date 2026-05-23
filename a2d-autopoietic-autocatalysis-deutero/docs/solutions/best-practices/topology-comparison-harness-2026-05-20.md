---
title: "Topology Comparison Harness"
date: 2026-05-20
category: best-practices
module: cli
problem_type: benchmark_methodology
component: live-benchmarks
symptoms:
  - "Seed-vs-evolved topology comparisons required manual env switching"
  - "Manual challenge runs could commit lineage mutations and apply accepted patches"
  - "Fitness, wall-clock, invocation count, caps, and provider failures were not summarized side by side"
root_cause: topology_benchmarks_were_ad_hoc_and_stateful
resolution_type: instrumentation
severity: medium
tags:
  - benchmark
  - topology
  - seed-germline
  - lineage
  - live-validation
---

# Topology Comparison Harness

## Problem

A²D needed to compare the hardcoded 4-enzyme seed topology against the lineage-loaded 7-enzyme evolved topology. Before this harness, the comparison required running two separate commands with different environment variables and manually reading logs.

That made results easy to skew:

- `a2d challenge` commits lineage when fitness improves.
- Accepted architect patches are applied to the working tree.
- Seed and evolved runs were not reported in a common summary.
- Wall-clock, invocation count, provider failures, caps, cycles-to-100%, and mutation/patch counts had to be extracted manually.

## Solution

The CLI now has a comparison command:

```bash
a2d compare-topologies sudoku 3
# alias:
a2d benchmark-topologies sudoku 3
```

The command runs, in order:

1. the hardcoded seed germline (`4 enzymes`), then
2. the lineage-loaded evolved germline when present, falling back to seed if lineage is empty.

Persistence is disabled for the comparison path:

- no lineage commits;
- no accepted system patches applied to the real source tree.

The per-cycle output reports:

- invocation/failure/kill/mutation/patch counts;
- cycle fitness when benchmarked;
- every invocation's enzyme, provider, and compact outcome (`OK`, `FAIL: ...`, or `KILL: ...`);
- coder candidate portfolio fitness/materialization details.

The final table reports:

- topology and enzyme count;
- best fitness;
- cycle where full fitness was first reached;
- wall-clock seconds;
- total invocations;
- provider failures;
- invocation/wall-clock caps;
- accepted mutations;
- accepted/rejected patch counts.

## Validation

Unit coverage added in `crates/a2d-cli/src/main.rs`:

- `topology_summary_tracks_best_fitness_and_full_cycle`
- `topology_summary_preserves_zero_fitness_denominator`
- `topology_lineage_entry_formats_failure_on_one_line`
- `topology_lineage_entry_truncates_long_errors`

Full test suite passes: 154 tests passing, 2 ignored.

Smoke validation with deliberately tiny provider/cycle budgets:

```bash
A2D_TRACE=1 A2D_PROVIDER_TIMEOUT_SECS=1 A2D_MAX_CYCLE_SECS=1 \
  cargo run -p a2d -- compare-topologies sudoku 1
```

Observed:

- seed topology ran with 4 enzymes;
- evolved topology ran with 7 enzymes;
- both prioritized `coder` first;
- both used the parallel GLM/Kimi coder race;
- both timed out as expected with 1s provider budgets;
- no lineage commits or patches were applied.

## 2026-05-22 addendum: per-invocation lineage details

A bounded 2026-05-22 comparison showed total non-coder failures but did not identify which enzyme/provider failed without `A2D_TRACE=1`. The harness now prints each lineage entry below the cycle summary, for example:

```text
  cycle 1: 1 invocations, 1 failures, 0 killed, 0 mutations, 0 patches
    [coder via opencode/kimi-for-coding/k2p6] FAIL: model invocation failed: opencode timed out after 1s
    candidate portfolio for coder:
      opencode/kimi-for-coding/k2p6: no materialized artifact — model invocation failed: opencode timed out after 1s
```

Failure text is compacted to one line and truncated so provider dumps cannot flood the comparison summary.

Smoke validation:

```bash
A2D_PROVIDER_TIMEOUT_SECS=1 A2D_MAX_CYCLE_SECS=1 \
  cargo run -p a2d -- compare-topologies sudoku 1
```

Log: `/tmp/a2d-topology-compare-sudoku1-lineage-details-smoke-20260522.log`

## Related

- `todos/bounded-live-benchmarks.md`
- `docs/solutions/best-practices/parallel-cheap-coder-race-2026-05-19.md`
