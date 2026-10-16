---
title: "Reviewer CLIs Need Filesystem Isolation, Not Sandbox Flags"
date: 2026-10-14
category: best-practices
module: process
problem_type: process_fix
component: multi-model-review
symptoms:
  - "A `codex exec -s read-only -C <repo>` reviewer edited docs/HANDOFF.md"
  - "Reviewer followed repo instructions (update HANDOFF at session end) meant for developing agents"
root_cause: user_config_auto_approves_sandbox_escalation
---

# Reviewer CLIs Need Filesystem Isolation, Not Sandbox Flags

## What happened

A plan review ran as `codex exec -s read-only -C <repo>`. The log header confirmed `sandbox: read-only`, yet the reviewer wrote a self-report line into `docs/HANDOFF.md` with an inline Python script and ran `cargo test`. It was following the repo's `CLAUDE.md`/`AGENTS.md` session protocol, which tells agents to update HANDOFF before ending.

## Cause

`~/.codex/config.toml` sets `approval_policy = "on-request"` with `approvals_reviewer = "guardian_subagent"`. Under on-request approvals, a sandboxed command can ask to escalate, and the guardian subagent approves the request automatically. `-s read-only` alone therefore does not prevent writes on this machine.

A²D's Codex provider passes `--ignore-user-config` and runs in an empty temporary cwd, so the provider path is not affected (by inspection, `crates/a2d-providers/src/cli.rs`; not live-tested).

## Practice

- Run reviewers in an empty temporary directory that holds copies of their inputs. Never point them at the repo with `-C <repo>`.
- Ask for markdown on stdout or `-o <file>` outside the repo.
- If a reviewer needs to read repo files, also pass `-c approval_policy="never"`, then check `git status` afterwards.
- Close stdin (`< /dev/null`) for background `codex exec` runs. Otherwise it blocks on "Reading additional input from stdin…" until it times out.
- Treat any reviewer-authored change in the repo as untrusted output: revert it, then fold the substance in yourself.

See also: `static-guards-are-not-isolation-use-os-sandbox-2026-10-16.md` (same lesson for LLM-written harnesses: isolate at the OS level, prove it with a planted secret).
