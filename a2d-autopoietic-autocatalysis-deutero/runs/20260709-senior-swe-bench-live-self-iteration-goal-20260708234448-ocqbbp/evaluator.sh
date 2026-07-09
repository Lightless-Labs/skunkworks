#!/usr/bin/env bash
set -euo pipefail
test "${A2D_SENIOR_SWE_BENCH_TASK_ID:-}" = "firezone-fix-connlib-align-device-hard"
test "${A2D_SENIOR_SWE_BENCH_REPO:-}" = "firezone/firezone"
test "${A2D_SENIOR_SWE_BENCH_PUBLIC_SOLUTION_SEARCH_FORBIDDEN:-}" = "true"
test "${A2D_SENIOR_SWE_BENCH_GITHUB_SOLUTION_SEARCH_ALLOWED:-}" = "false"
test -f "${A2D_SENIOR_SWE_BENCH_CANDIDATE_PATCH:-}"
grep -Eq '^(diff --git|--- a/)' "${A2D_SENIOR_SWE_BENCH_CANDIDATE_PATCH}"
grep -q 'pub fn release(&mut self, resource: &str) -> bool' connlib/device_pool.rs
grep -q 'true' connlib/device_pool.rs
grep -q 'false' connlib/device_pool.rs
printf 'live retry smoke evaluator passed: release reports success/failure and no public solution search env is set\n'
