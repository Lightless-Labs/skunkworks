---
title: "Pin a Spend-Once Instrument's Toolchain by Path, Not by Convention"
date: 2026-10-21
category: best-practices
module: nano/confirm3.py, ~/.a2d-queue, tools/gym
problem_type: evaluation_integrity
component: confirmation_runner
tags: [codex, homebrew, fingerprint, registration, shared-machine]
symptoms:
  - "/opt/homebrew/bin/codex moved from the registered 0.162.0 to 0.163.0 hours after a 'do not upgrade codex before 10-26' rule"
  - "confirm3.py generate would have refused at 00:05 UTC on the spend day: 'codex differs from the registered binary'"
  - "A detached phase-B round armed earlier would have run on an untested codex"
---

# Pin a Spend-Once Instrument's Toolchain by Path, Not by Convention

## What happened
- Confirmation-3 registers codex by version and by sha256 of every file beside the binary (`codex`, `codex-code-mode-host`).
- The runner checks that fingerprint before reserving an attempt. That is fail-closed, but it costs the day.
- The session had set a rule: "no `brew upgrade codex` before 10-26".
- At 10:32 UTC on the registration day, Homebrew moved the cask to 0.163.0 anyway, and its cleanup removed 0.162.0. This session did not do it; something else on the shared machine did.
- No test caught it. It came up only because a reviewing subagent noticed the version while copying a pin constant.

## Fix
1. Download the exact release package (`rust-v0.162.0/codex-package-aarch64-apple-darwin.tar.gz`) into its own empty directory, and **check the extracted binaries against the registered hashes**. Both were byte-identical.
2. Install it with its package layout (`bin/`, `codex-path/`, `codex-resources/`, `codex-package.json`) under a private root (`~/.a2d-codex-pin/0.162.0/`). Leave the global install alone, because other sessions use it.
3. Put the pin first on `PATH` in every job that must match the registration: the queue lines and detached rounds. The provider spawns `codex` by name, so the fingerprint check, the health probe and the provider all resolve to the pin.
4. Verify with the runner's own functions: `codex_fingerprint()` must equal the registration, `--drained` must pass, and a live PONG probe must be healthy.
5. Re-arm already-detached jobs. A running process keeps the PATH it started with.

## Practice
- **At registration, copy the toolchain to a private path and register that path**, together with the hashes. Never rely on a global package manager staying put on a shared machine.
- When a pinned binary moves under HOME, **sandbox profiles that used to allow it implicitly** (from `/opt/homebrew`) need an explicit allow for the whole package root, and a sandboxed `--version` smoke test.
- Fingerprint **the executable the child will actually run** (resolved through the child's PATH), and treat a missing fingerprint as a failure, never as equal to another missing one.
- A "do not touch X" rule that depends on other actors is a hazard, not a control. Turn it into a path the instrument owns.
