---
module: a2d-providers
tags: [provider-policy, benchmark-integrity, senior-swe-bench, codex]
problem_type: best-practice
---

# Codex artifact providers should use read-only sandbox mode, not full-auto

A²D provider roles are artifact producers: they should return typed text artifacts, while A²D applies source changes through its own `SystemPatch`, self-sandbox, retry, and fitness-evidence gates. A broad Codex `--full-auto` invocation is therefore the wrong default for provider subprocesses, even when the process is already launched in an isolated temporary cwd.

Use explicit least-privilege Codex flags for artifact roles:

- `--sandbox read-only`
- `--ephemeral`
- `--skip-git-repo-check`
- `--ignore-user-config`
- `--ignore-rules`

Regression coverage should also assert that Codex invocations do not include `--full-auto`, `--dangerously-bypass-approvals-and-sandbox`, `--dangerously-bypass-hook-trust`, or writable `--add-dir` arguments.

Scope note: this is a filesystem/tooling sandbox hardening measure for Codex CLI behavior. It is not OS/network no-egress enforcement and does not prove a provider could not access public solution sources over the network.
