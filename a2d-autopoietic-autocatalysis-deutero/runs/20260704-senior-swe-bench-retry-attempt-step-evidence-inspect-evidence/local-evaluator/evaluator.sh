#!/usr/bin/env bash
set -eu
test "$(cat lib.rs)" = "new"
test "$(cat "$A2D_SENIOR_SWE_BENCH_ORIGINAL_CHECKOUT/lib.rs")" = "old"
test "$A2D_SENIOR_SWE_BENCH_PUBLIC_SOLUTION_SEARCH_FORBIDDEN" = "true"
