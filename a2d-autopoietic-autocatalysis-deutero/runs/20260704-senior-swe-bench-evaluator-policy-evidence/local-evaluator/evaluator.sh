set -eu
test "$(cat lib.rs)" = "patched"
test "$(cat "$A2D_SENIOR_SWE_BENCH_ORIGINAL_CHECKOUT/lib.rs")" = "original"
test "$A2D_SENIOR_SWE_BENCH_TASK_ID" = "firezone-fix-connlib-align-device-hard"
test "$A2D_SENIOR_SWE_BENCH_REPO" = "firezone/firezone"
test "$A2D_SENIOR_SWE_BENCH_CANDIDATE_PATCH_APPLIED" = "true"
test "$A2D_SENIOR_SWE_BENCH_GITHUB_SOLUTION_SEARCH_ALLOWED" = "false"
test "$A2D_SENIOR_SWE_BENCH_PUBLIC_SOLUTION_SEARCH_FORBIDDEN" = "true"
echo senior-swe-bench-policy-env-ok
