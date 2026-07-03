#!/usr/bin/env bash
set -euo pipefail
: "${A2D_SENIOR_SWE_BENCH_TASK_ID:?missing task id}"
: "${A2D_SENIOR_SWE_BENCH_REPO:?missing repo}"
: "${A2D_SENIOR_SWE_BENCH_CANDIDATE_PATCH:?missing candidate patch}"
test -f "$A2D_SENIOR_SWE_BENCH_CANDIDATE_PATCH"
echo "local evaluator received $A2D_SENIOR_SWE_BENCH_TASK_ID for $A2D_SENIOR_SWE_BENCH_REPO"
echo "using local checkout only; no network or GitHub search"
