#!/usr/bin/env bash
set -euo pipefail
test "${A2D_SENIOR_SWE_BENCH_TASK_ID:-}" = "firezone-fix-connlib-align-device-hard"
test "${A2D_SENIOR_SWE_BENCH_REPO:-}" = "firezone/firezone"
test -f "${A2D_SENIOR_SWE_BENCH_CANDIDATE_PATCH:-}"
grep -q '^diff --git ' "$A2D_SENIOR_SWE_BENCH_CANDIDATE_PATCH"
echo "cycle-input replay evaluator passed for $A2D_SENIOR_SWE_BENCH_TASK_ID"
echo "local checkout only; no GitHub or public solution search"
