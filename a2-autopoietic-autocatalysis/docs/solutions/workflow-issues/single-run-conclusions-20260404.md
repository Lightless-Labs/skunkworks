---
module: benchmark
date: 2026-04-04
problem_type: workflow_issue
component: testing_framework
severity: high
tags: [benchmark, methodology, variance, statistics, conclusions]
applies_when:
  - "Reporting benchmark results"
  - "Comparing model or system capability"
  - "Drawing conclusions about success/failure rates"
---

# One run is not a result

## Rule

Never draw conclusions about model or system capability from a single benchmark run. Run N >= 3 and report the distribution before claiming anything.

## What went wrong

Running GLM via A² on the same five-task benchmark produced 5/5, 4/5, 3/5, 5/5 across four runs. The variance came from arbitrary infrastructure limits (token budget, timeout, OpenCode latency) tripping at different points in different runs — not from anything about the model or A² itself.

If the session had stopped after the first 5/5 it would have concluded "GLM solves this benchmark." If it had stopped after the 3/5 it would have concluded "GLM fails 40% of this benchmark." Both would have been wrong.

## What to do instead

1. Before reporting any benchmark number, run it at least 3 times.
2. Report min/max/median, not a single score.
3. If variance is high (>20% spread), the bottleneck is probably infrastructure (timeout, budget, retries), not capability. Investigate the limits before re-running.
4. If a benchmark is too slow to run 3 times, the benchmark is too slow — fix that before trusting any number it produces.

## Red flags that you are over-fitting to one run

- "The benchmark shows X" after one execution
- Updating HANDOFF.md with capability claims based on a single ./run.sh
- Designing the next change based on which task failed once
