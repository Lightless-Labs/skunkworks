---
title: "A Static Source Guard Is a Lint, Not a Boundary: Isolate LLM-Written Code at the OS Level"
date: 2026-10-16
category: best-practices
module: a2d-lift
problem_type: evaluation_integrity
component: external-harness
symptoms:
  - "An AST allowlist guard blocked imports and named calls, yet attribute routes still reached the filesystem"
  - "Both design reviewers independently found sys.modules['io'].open, sys.modules['os'], random._os and json.codecs.open"
  - "A planted hidden_tests.rs next to the harness was readable, and $HOME was listable"
  - "Python died silently at startup under a seatbelt profile (empty stderr, Broken pipe on the controller side)"
root_cause: source_level_guard_used_as_isolation_boundary
---

# A Static Source Guard Is a Lint, Not a Boundary: Isolate LLM-Written Code at the OS Level

## Context

Cycle-2 harnesses are LLM-written Python files that the controller runs once per task (`python3 -I`, empty temp cwd, minimal env). `nano/eval.py` `guard()` was meant to keep them away from hidden tests, holdouts and the oracle's build dirs, which sit on the same machine. It allowed a few stdlib imports and banned named calls (`open`, `exec`, `eval`, `__import__`, `getattr`) and dunder attributes. The two design reviewers (Codex gpt-6.1-sol and a fresh Claude subagent) found the hole independently: allowlisted modules expose the filesystem through attributes. See `C2-ISO` in `docs/experiments/LOG.md`.

## What we did

1. **Confirmed it live first.** A new controller test planted a `hidden_tests.rs` beside the harness. Before the fix, the harness read it and listed `$HOME`. The reviewers' claim became a failing test, not an opinion.
2. **Bounded the blast radius.** A mechanical scan of all 9 distinct harness files that ever ran found no such access. Past results stand.
3. **Fix 1, the boundary** (`harness_sandbox_profile` in `crates/a2d-lift/src/arms.rs`). The harness runs under macOS `sandbox-exec`: `(allow default)(deny network*)(deny file-read* file-write* ...)` for `$HOME`, `$TMPDIR`, `/private/tmp` and `/private/var/folders`. Later allow rules reopen the interpreter prefix, the script (literal, read), the harness's own cwd and its stderr log. Later rules override earlier denies. It fails closed if `sandbox-exec` is missing, and the test asserts the secret is unreadable while the harness still runs and submits.
4. **Fix 2, defence in depth** (`guard()`). It now rejects private `_x` attributes, `sys.*` beyond stdin/stdout/stderr/exit, a denylist of module and filesystem attributes (`modules`, `os`, `io`, `open`, `codecs`, `inspect`, ...), any use of a forbidden builtin name (not only calls), and dunder names. 7 canaries are rejected and all real harnesses pass.

## Gotchas

- **Seatbelt checks inherited file descriptors.** The controller-opened stderr log lived under a denied path, and Python died silently at startup. Allowing only `file-write*` was not enough, because startup also stats fd 2. It needed `(allow file-read* file-write* (literal <stderr path>))`.
- **Use canonical paths** in profiles (`/var` is `/private/var`, `/tmp` is `/private/tmp`). Seatbelt matches the resolved path, so a non-canonical rule silently matches nothing.
- **Frozen controller paths.** A registered run freezes `crates/` and `Cargo.*`, so an instrument fix after registration voids it. The look was re-registered before any data existed.

## Practice

- A source-level guard can reduce accidents but cannot enforce isolation; Python reaches almost anything through attributes. Put the boundary in the OS (seatbelt, namespaces, containers) and keep the guard as a lint.
- Prove a boundary with a planted-secret test that asserts both "secret unreadable" and "the sandboxed program still works".
- When a reviewer points at a hole, reproduce it with a failing test before fixing.
- After closing a hole, scan past artifacts to say whether earlier results were exposed.

See also: `oracle-isolation-for-llm-candidates-2026-10-14.md`, `reviewer-clis-need-filesystem-isolation-2026-10-14.md`.
