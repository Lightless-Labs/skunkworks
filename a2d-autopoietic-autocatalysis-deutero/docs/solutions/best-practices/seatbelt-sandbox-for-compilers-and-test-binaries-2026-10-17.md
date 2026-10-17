---
title: "Seatbelt-Sandbox the Compiler and the Test Binary, Not Just the Harness"
date: 2026-10-17
category: best-practices
module: a2d-core
problem_type: evaluation_integrity
component: oracle
symptoms:
  - "A harness's public_test Rust could include_str!(\"<absolute path>\") or std::fs-read hidden tests under $HOME or the temp roots and get them back in the output"
  - "The submission flag list (include_str!, std::fs, ...) was a lint and never saw public_test traffic"
  - "After sandboxing, every link took 5-10 s instead of ~0.2 s and the first test hit the 120 s compile timeout"
  - "xcrun: error: couldn't create cache file ... Operation not permitted"
root_cause: oracle_compiled_and_ran_untrusted_rust_as_the_user
---

# Seatbelt-Sandbox the Compiler and the Test Binary, Not Just the Harness

## Context

`run_isolated_tests` in `crates/a2d-core/src/sandbox.rs` compiled with `rustc` and ran the test binary as the user. A harness controls the Rust it sends via `public_test`, so compile-time `include_str!` with an absolute literal path, or run-time `std::fs`, could read hidden tests and return them in the output. The Python harness was already sandboxed (`harness_sandbox_profile` in `crates/a2d-lift/src/arms.rs`); the oracle it calls was not.

## What we did

Both steps now run under `/usr/bin/sandbox-exec -p <profile>` (`oracle_sandbox_profile`). It fails closed if `sandbox-exec` is missing.

1. **Base:** `(version 1)(allow default)(deny network*)(deny file-read* file-write* ...)` over `$HOME`, `$TMPDIR`, `/private/tmp`, `/private/var/folders`, `/private/var/tmp`. SBPL lets later rules win, so the re-allows below go after the deny.
2. **Compile step:** read the toolchain sysroot, read/write its own build dir. The real `rustc` is located once via `rustc --print sysroot` (run from an empty dir), so the sandboxed step never goes through the rustup proxy and `~/.rustup` stays closed.
3. **Run step:** read only its binary (literal), read/write its run dir, then `(deny process-fork process-exec* mach-lookup appleevent-send)(allow process-exec (literal <binary>))`.
4. **Environment:** cleared and rebuilt (`PATH`, `HOME`, `LANG`, `TMPDIR`, `DEVELOPER_DIR`, `SDKROOT`); run-step `TMPDIR` is its own dir; stdin is null.

## Gotchas

- **The linker needs the xcrun cache.** `/usr/bin/cc` is the xcrun shim; its toolchain cache is `$TMPDIR/xcrun_db`, written via `xcrun_db-<random>`. With TMPDIR denied, xcrun could not write it (`couldn't create cache file ... Operation not permitted`) and every link took 5-10 s. Fix, compile step only: `(allow file-read* file-write* (regex #"^<canonical TMPDIR>/xcrun_db(-[A-Za-z0-9]+)?$"))`. Keep it anchored; reviewers flagged a broader first version. No candidate code runs at compile time (single-file crates: no build scripts, no proc macros).
- **`(allow default)` leaves IPC open.** Live: `launchctl submit` inside the profile was refused, but the same submit without the profile ran a job outside any sandbox that read the file. Brokers (launchd, LaunchServices `open`, Apple Events) are hard to enumerate, so the run step and the Python harness profile deny fork/exec (except the binary, or the interpreter's own re-exec under its prefix), `mach-lookup` and `appleevent-send`. Harnesses still run: JSON ops work, `os.system` returns 32512.
- **Hard links.** A hard link created *outside* the sandbox reads through. Fresh build/run dirs per call prevent that; inside, `std::fs::hard_link` fails.
- **Testing by hand:** if your shell's stdout is a file under a denied path, the sandboxed child's writes vanish silently. Pipe through `| cat`.

Verified denied: the direct path, the APFS firmlink twin `/System/Volumes/Data/Users/...`, `..` paths, symlinks; sandboxed `ln`/`cp`/`mv` of a denied file fail with EPERM.

## Tests (TDD)

Plant a needle file in the temp root and use the crate's own manifest (under `$HOME`), plus both `/System/Volumes/Data` twins. Assert: `include_str!` fails to compile; the needle is absent from output; `read_to_string` and `hard_link` return errors; `env!("CARGO_MANIFEST_DIR")` fails; `std::env::var` sees none of the user's environment; spawning `/bin/echo` fails; a private `temp_dir()` file round-trips. Red check: with the sandbox disabled, the `include_str!` test compiles and the test fails.

## Parity when the instrument changes mid-experiment

- Re-score an old run's 420 submissions with **both** the new and the old binary, same machine, same load (`a2d lift score` on a copy of the ledger cut at `generation_complete`). New vs old: 420/420 identical.
- Both binaries differed from the *recorded* ledger on one record: a timeout that only happened under the original run's load. Comparing only against the ledger would have blamed the sandbox.
- Run `a2d lift validate` (references pass, mutants caught) on every suite, holdouts printing only the count line: 140/140, 100/100, 144/144.
- Cost: scoring wall time +22% (~30 ms per sandboxed process).

## Review

Codex Sol (no tools) and a fresh Claude agent (working on copies only) found the broker gap, the env leak, temp-root coverage that was too narrow, and the xcrun regex breadth. All fixed before use.

## Practice

- Sandbox every process that touches candidate-controlled code, including the compiler. Compile-time file reads are a channel.
- Start from deny on the secret-bearing roots, re-allow the minimum, and deny process spawning and IPC brokers in the run step, not only files.
- Expect toolchain side channels (caches, shims) to break under the profile; allow the narrowest anchored path.
- Prove parity of an instrument change by old-vs-new at equal load, not new-vs-recorded.

See also: `static-guards-are-not-isolation-use-os-sandbox-2026-10-16.md`, `oracle-isolation-for-llm-candidates-2026-10-14.md`.
