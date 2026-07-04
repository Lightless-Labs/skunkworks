#!/usr/bin/env bash
set -euo pipefail
touch ../evaluator-ran.marker
test "${A2D_SENIOR_SWE_BENCH_PUBLIC_SOLUTION_SEARCH_FORBIDDEN}" = true
test "${A2D_SENIOR_SWE_BENCH_GITHUB_SOLUTION_SEARCH_ALLOWED}" = false
grep -q '^new$' src/lib.rs
