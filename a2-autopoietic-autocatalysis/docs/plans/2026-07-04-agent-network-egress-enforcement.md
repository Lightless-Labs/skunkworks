# Agent Network Egress Enforcement Plan

**Created:** 2026-07-04
**Status:** Open

## Why

Senior SWE Bench and other external benchmarks cannot be treated as uncontaminated while coding agents can reach GitHub, public issue trackers, PRs, patches, or solution writeups. Prompt text and `no_external_solution_search=true` metadata are audit markers only; the control must be enforced at the child-agent/process boundary and recorded in run artifacts.

## Current evidence

- A²-owned `a2ctl run --network-policy isolated` currently fails closed before provider launch; this prevents contaminated runs but is not a usable offline/provider-allowlisted execution sandbox.
- `bench/network_policy_smoke.py --self-test` proves this host can run a generic child under macOS `sandbox-exec` with TCP denied, but that primitive is not wired around coding agents.
- `docs/benchmark-results/network-policy/20260704-subagent-foundry-boundary-probe.json` is negative boundary evidence:
  - a pi `subagent` scout successfully opened a localhost TCP connection to a parent listener;
  - Foundry `foundry_team` source audit found child `pi` spawning with isolation flags for sessions/extensions/skills/templates, but no OS-level network sandbox/provider allowlist.
- This does **not** prove public internet/GitHub reachability; it proves the child-agent boundary must not be assumed network-isolated.

## Exact enforcement points found

Audited source versions:

- Pi package: `/opt/homebrew/lib/node_modules/@earendil-works/pi-coding-agent`, package version `0.80.2`.
- Foundry repo: `/Users/thomas/.pi/agent/git/github.com/Lightless-Labs/foundry`, tracked `HEAD=06e26c308ea8101a4918248e0e304e3b352e183a` during the audit. The repo had untracked `node_modules/` and `package-lock.json`; the tracked extension source below was inspected at that HEAD.

- Pi example subagent extension:
  - `/opt/homebrew/lib/node_modules/@earendil-works/pi-coding-agent/examples/extensions/subagent/index.ts`
  - `runSingleAgent()` builds child args and spawns `pi`.
  - `getPiInvocation()` selects the `pi` executable/current script.
  - spawn site: `spawn(invocation.command, invocation.args, { cwd: cwd ?? defaultCwd, shell: false, stdio: ["ignore", "pipe", "pipe"] })`.
- Foundry Pi team extension:
  - `/Users/thomas/.pi/agent/git/github.com/Lightless-Labs/foundry/extensions/pi-foundry-team/index.ts`
  - `runDispatch()` builds PromptEnvelope child args and spawns `pi`.
  - `piInvocation()` selects the `pi` executable/current script.
  - spawn site: `spawn(invocation.command, invocation.args, { cwd, shell: false, stdio: ["ignore", "pipe", "pipe"] })`.
- Pi sandbox example:
  - `/opt/homebrew/lib/node_modules/@earendil-works/pi-coding-agent/examples/extensions/sandbox/index.ts`
  - shows `SandboxManager.wrapWithSandbox(command)` and `SandboxManager.initialize({ network: ... })` via `@anthropic-ai/sandbox-runtime`.
  - Current local global install check: `npm list -g @anthropic-ai/sandbox-runtime --depth=0` showed empty, so this runtime is not currently available globally.
- Repo-local audit command:
  - `python3 bench/agent_network_boundary_check.py --self-test` verifies the child-agent launch points and sandbox example remain locatable, while explicitly reporting whether the sandbox runtime is available.
  - `python3 bench/agent_network_boundary_check.py --require-sandbox-runtime` is an intentional fail-closed precondition check for future enforcement work; on 2026-07-04 it exited 1 with `@anthropic-ai/sandbox-runtime not installed globally`.
  - This audit is not benchmark evidence and must stay outside sentinel/pass criteria until it verifies actual child-agent egress enforcement rather than a missing prerequisite.

## Implementation direction

1. Add a reviewed child-process network policy option to the Pi subagent/foundry execution path, not just benchmark prompts.
2. Wrap child `pi` invocations at the spawn boundary with an auditable sandbox command/profile:
   - isolated/offline mode: deny all outbound network;
   - provider-allowlisted mode: deny GitHub/public solution domains while permitting only required model-provider endpoints if a live model call must happen.
3. Fail closed when a benchmark task requests `network_policy=Isolated` or Senior SWE Bench-style anti-cheat but the sandbox/provider allowlist cannot be activated.
4. Record the enforced policy in artifacts: sandbox engine, profile/hash, allowlist/denylist, child command, cwd, and smoke result.
5. Add regression/smoke coverage from inside the actual child-agent path:
   - a local TCP probe denied under isolated policy;
   - a controlled external egress probe or domain-policy test that demonstrates GitHub/public-solution egress is blocked;
   - a positive local filesystem probe so the agent can still edit/test code;
   - fail-closed behavior when sandbox dependencies are missing.

## Acceptance criteria

- Senior SWE Bench evidence remains blocked unless run artifacts include enforced child-agent network policy evidence.
- `subagent`/`foundry_team` benchmark dispatches cannot silently run unrestricted when isolation is requested.
- Handoff and TODOs continue to distinguish prompt/metadata guards, fail-closed launch gates, generic sandbox primitives, and real child-agent sandbox enforcement.
