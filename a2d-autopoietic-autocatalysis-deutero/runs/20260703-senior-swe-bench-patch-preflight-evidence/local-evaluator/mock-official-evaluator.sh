#!/usr/bin/env bash
set -euo pipefail
test "${A2D_SENIOR_SWE_BENCH_TASK_ID:-}" = "firezone-fix-connlib-align-device-hard"
test "${A2D_SENIOR_SWE_BENCH_REPO:-}" = "firezone/firezone"
test "${A2D_SENIOR_SWE_BENCH_CANDIDATE_PATCH_APPLIED:-}" = "true"
test -f "${A2D_SENIOR_SWE_BENCH_CANDIDATE_PATCH:-}"
grep -q '^+patched-by-candidate' "$A2D_SENIOR_SWE_BENCH_CANDIDATE_PATCH"
test "$(cat lib.rs)" = "patched-by-candidate"
test "$(cat "$A2D_SENIOR_SWE_BENCH_ORIGINAL_CHECKOUT/lib.rs")" = "original"
test "$PWD" = "$A2D_SENIOR_SWE_BENCH_EVALUATOR_CHECKOUT"
echo "patched evaluator workspace: $PWD"
echo "original checkout preserved: $A2D_SENIOR_SWE_BENCH_ORIGINAL_CHECKOUT"
echo "candidate patch: $A2D_SENIOR_SWE_BENCH_CANDIDATE_PATCH"
