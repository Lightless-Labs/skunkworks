#!/usr/bin/env bash
set -euo pipefail
test "${A2D_SENIOR_SWE_BENCH_GITHUB_SOLUTION_SEARCH_ALLOWED}" = false
test "${A2D_SENIOR_SWE_BENCH_PUBLIC_SOLUTION_SEARCH_FORBIDDEN}" = true
for key in HTTP_PROXY HTTPS_PROXY ALL_PROXY FTP_PROXY NO_PROXY http_proxy https_proxy all_proxy ftp_proxy no_proxy GIT_PROXY_COMMAND CARGO_HTTP_PROXY CARGO_HTTP_CAINFO CARGO_HTTP_CHECK_REVOKE RUSTUP_DIST_SERVER RUSTUP_UPDATE_ROOT; do
  if [ -n "${!key-}" ]; then
    echo "$key leaked" >&2
    exit 42
  fi
done
grep -q '^new$' src/lib.rs
echo evaluator-env-scrub-ok
