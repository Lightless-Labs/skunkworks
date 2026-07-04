set -eu
grep -q 'align_device_resource' lib.rs
grep -q 'normal_path_succeeds' lib.rs
test "$(cat "$A2D_SENIOR_SWE_BENCH_ORIGINAL_CHECKOUT/lib.rs")" = "original"
test "$A2D_SENIOR_SWE_BENCH_CANDIDATE_PATCH_APPLIED" = "true"
test "$A2D_SENIOR_SWE_BENCH_PUBLIC_SOLUTION_SEARCH_FORBIDDEN" = "true"
echo senior-swe-bench-no-tools-prompt-ok
