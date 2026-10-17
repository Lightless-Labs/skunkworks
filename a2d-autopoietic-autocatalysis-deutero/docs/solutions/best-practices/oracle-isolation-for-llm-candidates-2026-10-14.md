---
title: "Oracle Isolation for LLM-Written Rust Candidates"
date: 2026-10-14
category: best-practices
module: a2d-lift
problem_type: evaluation_integrity
component: oracle
symptoms:
  - "Hidden tests appended to the candidate file run inside the candidate's macro and source scope"
  - "A read-only Codex sandbox still lets the coder read hidden tests from disk"
  - "Child processes that fill a pipe buffer stall and get misreported as timeouts"
root_cause: shared_compilation_unit_and_tool_enabled_generator
---

# Oracle Isolation for LLM-Written Rust Candidates

## Context

`a2d lift` scores one-file Rust submissions with hidden tests. The first version appended the tests to the candidate's source as a `#[cfg(test)]` module and parsed libtest's summary. The P0 gate review (Codex gpt-6.1-sol and a fresh Claude subagent) found that every part of that is reachable by candidate code, and that the coder lane could read the tests directly.

## What reaches the tests, and the fix

| Channel | Why it works | Fix |
|---|---|---|
| `macro_rules! assert_eq { … }` in the candidate | Textual macro scope flows into later items, including the appended test module | Declare the test module **before** the candidate's code |
| `#[macro_export] macro_rules! assert_eq` | `use super::*` glob-imports it, and a glob beats the prelude | Explicit `use ::std::{assert, assert_eq, …};` in the test module (an explicit import beats a glob) |
| `include_str!(file!())` | Same file means the candidate can read the test source at compile time | Candidate is the crate root; tests live in a separately named `#[path = "t_<nonce>.rs"] mod …;` file. The candidate can only learn the file name, and `include_str!` cannot take a computed path |
| Reading sources at run time | Source files sit beside the test binary | Delete the sources after compiling; run the binary from an empty cwd |
| Printing a fake `test result: ok. N passed` | The parser trusted the first summary line | Read the **last** summary line and require exit status 0 and `passed == expected` |
| Coder reads files during generation | `codex exec --sandbox read-only` restricts writes only; reads and web search stay open | Lane flags `--disable shell_tool --disable unified_exec --disable js_repl --disable web_search_request --disable standalone_web_search --disable browser_use --disable computer_use --disable apps`. Live-checked: asked to run `ls /`, the model made no exec and replied it could not |
| Scores visible while later arms run | Scoring happened inline, and the ledger is on disk | Two phases: all arms generate and submit, then every submission is scored once |

Residual risk, accepted: candidate code that runs inside a hidden test could print a forged summary and then call `process::exit(0)`. Submissions are statically flagged for `process::exit`, `include_str!`, `std::fs`, `env::var` and `"test result:"`, and any flagged submission gets reviewed before its run's result is used.

## Related pitfall: pipe buffers

Rust's `Command::output()` and `wait_with_output()` only read stdout and stderr after the child exits. A child that writes more than about 64 KB blocks on a full pipe and never exits, and a timeout loop then reports it as hung. Codex echoes the prompt and response on stderr, so a large repair prompt is enough to trigger this. Drain both pipes with reader threads while polling `try_wait()`. On timeout, don't join the readers: a grandchild may still hold the pipe open.

## Testing the oracle

The tests in `crates/a2d-lift/src/oracle.rs` cover each attack: macro shadowing (textual and exported), compile-time self-reads, a forged summary followed by a failing exit, and hoisting of inner attributes and `//!` docs. One pitfall in writing them: a self-read test must build its needle at run time (`["sees", "_nothing"].concat()`). Otherwise the candidate finds the needle in its own source, and the test fails for the wrong reason.

See also: `static-guards-are-not-isolation-use-os-sandbox-2026-10-16.md` (static guards are lint; enforce the boundary with an OS sandbox).
See also: `seatbelt-sandbox-for-compilers-and-test-binaries-2026-10-17.md` (compile and run steps under seatbelt, closing the absolute-path `include_str!` and `std::fs` channels).
