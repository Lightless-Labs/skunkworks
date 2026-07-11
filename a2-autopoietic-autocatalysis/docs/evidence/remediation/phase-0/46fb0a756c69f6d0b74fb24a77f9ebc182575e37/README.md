# Phase-0 Evidence — Source `46fb0a756c69f6d0b74fb24a77f9ebc182575e37`

**Captured:** 2026-07-11
**Tested source commit:** `46fb0a756c69f6d0b74fb24a77f9ebc182575e37`
**Disposition:** Verification liveness is bounded and green; the overall Phase-0 stop/go gate remains red because Bazel target parity is absent and the required sandbox boundary gate fails closed.

This directory is intentionally keyed to the commit that was executed. The later commit that archives these files is not the tested source commit and must not be presented as such.

## Findings

- `cargo test -p a2ctl maturity -- --nocapture` completed serially in 21.91 seconds, returned 0, passed 2 tests, and left its child-owned process group absent with zero attributed orphans.
- The timeout assay launched a shell plus descendant that ignored `SIGTERM`. The harness enforced the two-second bound with 0.049 seconds of enforcement delay, escalated to `SIGKILL`, reaped the group, and reported zero orphans.
- The earlier source `e881ee7` diagnostic run compiled for 6 minutes 40 seconds and then ran the focused tests in 0.18 seconds. It did not time out. Its report was inconclusive because the first harness revision classified unrelated shared-host Cargo processes as owned orphans. Commit `46fb0a7` fixed attribution using process identity plus cwd/target boundaries; the superseded raw report is preserved under the `e881ee7...` evidence directory.
- The warm current-source run compiled in 14.26 seconds and ran tests in 0.31 seconds. Together, the reports point to cold-build/host/target throughput rather than a test deadlock. Neither retained Cargo report contains Cargo's `Blocking waiting for file lock` diagnostic, so these artifacts do not establish build-lock contention.
- `a2ctl status --json` reports `PreStage0` and live germline mutation disabled.
- Mutation-capable resident `--apply` exits 2 before side effects with the expected `PreStage0` refusal.
- `agent_network_boundary_check.py --require-sandbox-runtime --json` remains red: the sandbox runtime is not installed and actual child-agent launch functions do not show sandbox enforcement. No provider run is authorized.

## Artifacts

`evidence-index.json` contains command/result summaries and SHA-256 digests for every artifact. Detailed harness reports are gzip-compressed because process inventories are operationally verbose:

```bash
gzip -dc focused-cargo-test.json.gz | python3 -m json.tool
```

Key files:

- `focused-cargo-test.json.gz` — green serial focused Cargo verification.
- `timeout-cleanup-assay.json.gz` — expected timeout and forced process-group cleanup.
- `status-command.json.gz` and `maturity.json` — executable and compact maturity evidence.
- `apply-lockout.json.gz` — expected fail-closed mutation refusal.
- `agent-network-boundary.json.gz` — expected red stop/go boundary result.
- `harness-self-test.txt` — 10 deterministic harness regressions passed.

A nonzero expected-refusal or expected-timeout command is not labeled as a passing verification command in its raw report. Its expected disposition is recorded separately in `evidence-index.json` and validated against exact return code, cleanup state, and output.
