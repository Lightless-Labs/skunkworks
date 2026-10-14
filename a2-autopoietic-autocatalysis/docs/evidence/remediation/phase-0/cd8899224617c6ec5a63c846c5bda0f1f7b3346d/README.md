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

## Remaining blockers (from `fresh_provider_backed_evidence_blockers`)

- usable audited sandbox/provider allowlist enforcement not recorded in fresh rows;
- no fresh provider-backed benchmark, current-HEAD loop evidence, or uncontaminated Senior SWE Bench evidence.

Known residual risk: Seatbelt does not confine work handed to launchd/LaunchServices (`open`, `launchctl submit`); other providers (Claude, Codex, Gemini, OpenCode) still load their own ambient user configuration.

## Artifacts

- `agent-network-boundary.json.gz` / `.rc` — full audit JSON and exit code.
- `sandbox-profile-tests.txt` — focused Rust test results.
- `boundary-audit-unit-tests.txt` — 77 audit unit tests OK.
- `SHA256SUMS` — digests of the above.
