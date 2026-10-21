#!/bin/sh
# usage: ./api_check.sh <reference.rs> <tests.rs>
# Compiles the reference as a separate library crate, then the tests as a
# test crate that can reach ONLY its `pub` items (`use a2d_reference::*;`).
# Exit 0 = tests use only the public API.
RUSTC="${RUSTC:-$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc}"
d=$(mktemp -d ./.api.XXXXXX) || exit 3
cp "$1" "$d/lib.rs"
{ printf '#![allow(unused_imports, dead_code)]\nuse a2d_reference::*;\n'; cat "$2"; } > "$d/tests.rs"
if ! "$RUSTC" --edition 2024 --crate-type lib --crate-name a2d_reference "$d/lib.rs" -o "$d/liba2d_reference.rlib" 2> "$d/err"; then
  echo "LIB_COMPILE_FAIL"; head -30 "$d/err"; rm -rf "$d"; exit 2; fi
if ! "$RUSTC" --test --edition 2024 --extern a2d_reference="$d/liba2d_reference.rlib" "$d/tests.rs" -o "$d/bin" 2> "$d/err"; then
  echo "API_FAIL (tests need non-public items)"; head -40 "$d/err"; rm -rf "$d"; exit 1; fi
echo API_OK; rm -rf "$d"
