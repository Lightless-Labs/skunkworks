---
title: "CLI Providers Must Run in an Isolated Working Directory"
date: 2026-04-28
category: runtime-bugs
module: providers
problem_type: safety_boundary
component: cli-provider
symptoms:
  - "A model invocation modified repository source files outside the SystemPatch/self-sandbox path"
  - "GLM architect changed the then-existing core challenge catalog directly during a live challenge run"
  - "Report showed zero accepted patches, but git status showed source modifications"
root_cause: provider_cli_had_repo_cwd_and_tool_write_access
resolution_type: code_fix
severity: critical
tags:
  - cli-provider
  - filesystem-isolation
  - self-sandbox
  - autopoiesis
  - constitutional-boundary
  - opencode
  - glm-5.1
---

# CLI Providers Must Run in an Isolated Working Directory

## Problem

During live validation after switching the default provider to GLM 5.1, the provider invocation returned successfully, but `git status` showed the then-existing core challenge catalog modified even though the cycle reported `0 patches`. That catalog later moved to `crates/a2d-cli/src/challenges.rs` as part of the 2026-07-04 core-boundary cleanup.

The diff changed the Sudoku challenge requirements directly. That means the CLI coding provider had edited repository files as a side effect of model execution, bypassing the intended autopoietic path:

```text
Architect output → SystemPatch JSON → self_sandbox validation → accepted patch → CLI applies patch
```

This bypass is unacceptable. The model may propose source changes, but it must not be able to directly mutate the repo.

## Why this happened

`CliProvider` spawned external coding tools with the A²D repository as the current working directory. Tools like OpenCode can write files. Even if A²D only consumes stdout, the provider process may still perform filesystem side effects in its cwd.

The self-sandbox protected the patch path, but there was no filesystem isolation around provider execution itself.

## Fix

Each CLI provider invocation now runs in a fresh empty temp directory:

```rust
let sandbox_dir = isolated_provider_cwd(&self.command)?;
Command::new(&self.command)
    .args(&args)
    .current_dir(&sandbox_dir)
    ...
```

The temp directory is removed after success, failure, wait error, or timeout. Provider artifacts still flow through stdout/stderr only.

## Why this works

The provider can still use its global auth/config, but its process cwd is no longer the repository. If the model attempts to edit files, it edits an ephemeral temp directory, not A²D source.

This preserves the intended safety boundary:

- **Allowed:** provider emits code/text/JSON artifacts on stdout
- **Allowed:** architect proposes a `SystemPatch` artifact
- **Allowed:** CLI applies accepted patches after self-sandbox validation
- **Forbidden:** provider mutates repo files directly during generation

## Relation to the Constitution

This protects the spirit of:

- Invariant 2: mechanical verification only
- Invariant 4: information barriers
- Invariant 5: irreversible review gates

Without cwd isolation, automated actors could alter source outside the mechanical validation path and make `CycleReport.accepted_patches` lie by omission.

## Live validation

After the fix, `A2D_TRACE=1 cargo run -p a2d -- challenge sudoku 1` with GLM default completed with clean `git status` (only pre-existing untracked files remained). No source files were mutated despite architect/coder provider invocations.

## Prevention

- New provider integrations must not execute with repo cwd unless they are explicitly read-only.
- If a provider needs repository context, pass that context as an artifact, not filesystem access.
- Treat any untracked/modified source file after a provider invocation with zero accepted patches as a boundary violation.

## Related

- `crates/a2d-providers/src/cli.rs`
- `crates/a2d-core/src/self_sandbox.rs`
- `docs/solutions/runtime-bugs/provider-circuit-breaker-temporary-cooldown-2026-04-23.md`
