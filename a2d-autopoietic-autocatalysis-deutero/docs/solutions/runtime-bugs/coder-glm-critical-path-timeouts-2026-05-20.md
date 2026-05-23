---
title: "Keep Slow Providers Off the Coder Critical Path"
date: 2026-05-20
category: runtime-bugs
module: cli
problem_type: provider_assignment
component: provider-registry
symptoms:
  - "GLM repeatedly timed out on coder invocations"
  - "Parallel coder race waited for GLM loser timeout before selecting any usable output"
  - "Critical code-producing path consumed the whole bounded cycle"
root_cause: slow_provider_registered_as_coder_default_and_race_candidate
resolution_type: configuration_change
severity: high
tags:
  - provider
  - coder
  - glm
  - critical-path
  - bounded-benchmarks
---

# Keep Slow Providers Off the Coder Critical Path

## Problem

The topology comparison run showed that GLM is a poor default for the critical coder path under current OpenCode behavior.

In `/tmp/a2d-topology-compare-sudoku3-20260520.log`:

- seed topology: GLM timed out on every coder attempt and reached 0% best fitness;
- evolved topology: the first coder cycle produced a 67% artifact through fallback, but GLM still consumed the loser timeout window;
- the parallel race implementation waits for scoped provider threads to finish, so a slow loser still determines wall-clock when it is included in the race.

## Solution

The default live provider registry now separates coder from non-coder roles:

- coder default: `opencode/kimi-for-coding/k2p6`;
- coder fallback/race peer: `opencode/opencode/deepseek-v4-flash-free`;
- tester/evolver/architect: `opencode/zai-coding-plan/glm-5.1`.

Because GLM is explicitly assigned to non-coder roles, `parallel_providers_for_avoiding` excludes it from speculative coder races. MiniMax highspeed and Kimi k2.5 were also removed from the default coder race after smoke validation showed they were not faster for the coding prompt class.

## Validation

Unit coverage added in CLI: `live_registry_keeps_glm_off_coder_critical_path`.

Smoke validation first confirmed GLM was absent from the coder race, but MiniMax highspeed + Kimi k2.5 still timed out at 60s. A direct provider smoke then tested additional verified OpenCode model IDs:

- `opencode/qwen3.6-plus-free`: simple response in ~11s;
- `opencode/deepseek-v4-flash-free`: simple response in ~3s;
- `kimi-for-coding/k2p6`: simple response in ~3s;
- `minimax-coding-plan/MiniMax-M2.7-highspeed`: timed out at 45s on the simple smoke.

The coder pool was updated to Kimi k2.6 + DeepSeek v4 flash.

Topology smoke with a one-invocation cycle:

```bash
A2D_TRACE=1 A2D_PROVIDER_TIMEOUT_SECS=90 A2D_MAX_CYCLE_SECS=1 \
  cargo run -p a2d -- compare-topologies sudoku 1
```

Observed:

- seed: 0% (coder timeout);
- evolved: 67% (4/6) from one coder invocation;
- GLM absent from coder race;
- log: `/tmp/a2d-topology-compare-sudoku1-fastpool-oneinvoke-20260520.log`.

## Related

- `docs/solutions/best-practices/parallel-cheap-coder-race-2026-05-19.md`
- `docs/solutions/runtime-bugs/failed-consultation-double-timeout-2026-05-20.md`
- `todos/bounded-live-benchmarks.md`
