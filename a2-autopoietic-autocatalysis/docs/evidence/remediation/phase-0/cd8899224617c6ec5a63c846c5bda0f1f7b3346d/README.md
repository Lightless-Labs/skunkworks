# Phase-0 Boundary Gate Evidence — Source `cd8899224617c6ec5a63c846c5bda0f1f7b3346d`

**Captured:** 2026-10-14
**Tested source commit:** `cd8899224617c6ec5a63c846c5bda0f1f7b3346d` (clean `crates/` and audit script at capture time)
**Disposition:** `python3 bench/agent_network_boundary_check.py --require-sandbox-runtime --json` passes (rc 0) with `child_agent_containment_mechanism = inherited_process_tree_sandbox`. Fresh provider-backed evidence remains blocked; this is not benchmark evidence.

Supersedes the red boundary result in `../46fb0a756c69f6d0b74fb24a77f9ebc182575e37/` for the boundary gate only. The mechanism change is recorded in the roadmap addendum dated 2026-10-14.

## Findings

- Every A²-owned `pi` launch (`broker.pi`, `worktree_catalyst.run_pi`) builds its argv through `pi_provider_args`, which adds `--no-extensions --no-skills --no-prompt-templates`. In Pi 0.80.6 `noExtensions` drops all non-CLI extension paths, including settings packages, so Foundry's `pi-foundry-team` never loads in A²-launched Pi.
- Restricted-policy `pi` launches are `sandbox-exec`-materialized (existing audit check).
- Live probe with A²'s exact `Isolated` profile: unsandboxed control grandchild connected to a loopback listener; `sandbox-exec` grandchild returned 1 with `Operation not permitted`.
- Rust test `sandbox_profile::tests::isolated_profile_denies_network_to_grandchild_processes` passed.
- `@anthropic-ai/sandbox-runtime` remains absent and the Foundry/subagent spawn sites remain unwrapped; both are now informational.

## Bazel affected targets

`bazel test //crates/a2_workcell:a2_workcell_test //crates/a2_broker:a2_broker_test` (rc 3):

- `//crates/a2_broker:a2_broker_test` passed.
- `//crates/a2_workcell:a2_workcell_test` 57/58: the new Pi-args and grandchild-denial tests pass under Bazel's own darwin sandbox. `worktree_catalyst::tests::full_self_modification_pipeline_create_task_execute_diff_apply_cargo_check` fails because it spawns `cargo check` and `cargo` is not on the Bazel test PATH. The test predates this change and is not touched by it; it is a test-hermeticity gap that keeps the Bazel target red and blocks Phase-0 completion.

Cargo/Bazel target parity: 25/25 (`cargo-bazel-parity.json`).

## Remaining blockers (from `fresh_provider_backed_evidence_blockers`)

- usable audited sandbox/provider allowlist enforcement not recorded in fresh rows;
- no fresh provider-backed benchmark, current-HEAD loop evidence, or uncontaminated Senior SWE Bench evidence.

Known residual risk: Seatbelt does not confine work handed to launchd/LaunchServices (`open`, `launchctl submit`); other providers (Claude, Codex, Gemini, OpenCode) still load their own ambient user configuration.

## Artifacts

- `agent-network-boundary.json.gz` / `.rc` — full audit JSON and exit code.
- `sandbox-profile-tests.txt` — focused Rust test results.
- `boundary-audit-unit-tests.txt` — 77 audit unit tests OK.
- `bazel-affected-targets.txt` — Bazel test results for the changed crates.
- `cargo-bazel-parity.json` — parity checker output.
- `SHA256SUMS` — digests of the above.
