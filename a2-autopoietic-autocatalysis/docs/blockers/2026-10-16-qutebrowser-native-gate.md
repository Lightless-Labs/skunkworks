# Blocker: qutebrowser does not gate natively on this host

**Date:** 2026-10-16
**Blocks:** qutebrowser's share of the sealed confirmation pool (`bench/external-value/protocol-v2.toml`).
**Status:** Closed by a design change. The sealed pool is openlibrary only.

## Facts

These are validity-gate results only. No agent has run on any sealed task.

| Gate | openlibrary passed | qutebrowser passed | Fix applied before the gate |
|---|---|---|---|
| 1 | 28 / 62 | 0 / 63 | none |
| 2 (stopped at 84/125) | 45 / 62 | 0 / 14 | lockfile `-raw` fallbacks; plugin autoload; Dockerfile Python pin |
| 3 | 46 / 62 | 0 / 14 (first rows) | import roots restored after pytest is pinned; PyQt wheels only |

Gate 3's first 14 qutebrowser rows break down as:
- 7 setup errors (dependency resolution or builds);
- 7 failed gold runs.

The 2019–2021 bases need Qt and PyQt combinations that have no arm64 wheels. Newer PyQt5 5.15.x installs, but gold still fails on the bases tried.

## Decision

The rule is two attempts without movement, then a design change, so I am making no third qutebrowser fix.

The protocol's selection rule already takes per-repo shares in proportion to gate yield. qutebrowser therefore contributes nothing, and the sealed pool is openlibrary's gated tasks:
- 46 passed gate 3;
- after the 5 calibration tasks, the sample is about 41, above `minimum_sample = 40`.

No protocol edit is needed beyond the existing addenda.

## Cost of the change, and the way to recover it

Power falls with the smaller sample. At n ≈ 41 and a 4% baseline-only win rate, the one-sided McNemar test at p ≤ 0.05 detects a true +10 pp gain with probability 0.29, +15 pp with 0.53, and +20 pp with 0.74. The decision record must report this table.

To regain power, a later and separate decision could add a Go repo (flipt: 63 tasks with |F2P| ≥ 2; vuls: 47). That needs two things:
- a Go toolchain on this host, which is not installed today;
- a `go test` grader, because the current grader is pytest/junit only.

It would be its own dated addendum to protocol-v2, made before any sealed agent run.
