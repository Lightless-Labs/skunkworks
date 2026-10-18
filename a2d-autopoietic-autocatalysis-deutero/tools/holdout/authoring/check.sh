#!/bin/sh
# usage: ./check.sh <candidate.rs> <tests.rs>
# Mirrors the lift oracle: candidate is the crate root, tests are a #[path]
# module declared first with `use super::*;`. Prints the libtest summary.
# Exit 0 only if every test in the module passes.
RUSTC="${RUSTC:-$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc}"
d=$(mktemp -d ./.check.XXXXXX) || exit 3
{ printf '#[cfg(test)]\n#[path = "t.rs"]\nmod oracle;\n'; cat "$1"; } > "$d/c.rs"
{ printf '#![allow(unused_imports, dead_code)]\nuse super::*;\n'; cat "$2"; } > "$d/t.rs"
if ! "$RUSTC" --test --edition 2024 "$d/c.rs" -o "$d/bin" 2> "$d/err"; then
  echo "COMPILE_FAIL"; head -40 "$d/err"; rm -rf "$d"; exit 2
fi
( ulimit -t 60; "$d/bin" oracle:: ) > "$d/out" 2>&1; status=$?
grep -E '^test |test result' "$d/out" | tail -40; rm -rf "$d"; exit $status
