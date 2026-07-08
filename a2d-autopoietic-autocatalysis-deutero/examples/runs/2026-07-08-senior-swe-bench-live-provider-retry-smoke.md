# Senior SWE-Bench live-provider retry smoke — 2026-07-08

## Purpose

Regenerate a bounded Senior SWE-Bench local-wrapper retry-chain smoke on current HEAD after prior stale live evidence was discarded for source-revision and host-local path issues.

## Lineage

Prior retry/controller slices made each gate deterministic, but the stale `runs/20260707-live-provider-retry-execute-evidence/` bundle was not persistence-ready. The latest portability hardening made in-project paths repo-relative and isolated evaluator checkouts symbolic, leaving one remaining leak in non-authoritative `fitness-evidence-inspect` stdout preview text.

## Change

- `senior-swe-bench-retry-attempt-step-evidence` now emits `fitness_evidence_inspect_stdout_preview` through a portable preview helper.
- Structured evidence paths remain unchanged and authoritative for validation; only bounded diagnostic preview text is relativized.
- Regression coverage asserts the structured `fitness_evidence_path` remains parseable while the preview no longer leaks the host-local temp fixture root.

## Validation

```bash
cargo fmt --check
CARGO_BUILD_JOBS=2 cargo test -p a2d --test senior_swe_bench_retry_attempt_step_evidence -- --nocapture
CARGO_BUILD_JOBS=2 cargo test
cargo run -q -p a2d -- fitness-evidence-inspect \
  runs/20260708-live-provider-retry-smoke-head-867c327/attempt-0/fitness/senior-swe-bench-firezone-fix-connlib-align-device-hard-cycle-0-fitness-evidence.json \
  --require-all-tests-pass
```

Fresh source-patch/live-smoke evidence: `runs/20260708-live-provider-retry-smoke-head-867c327/attempt-0/fitness/senior-swe-bench-firezone-fix-connlib-align-device-hard-cycle-0-fitness-evidence.json`.

Evidence hash / scoped crates diff hash: `5f4408beb1d62447d57f1f680e3cd58fcb61d4e5`.

Live smoke summary: `cycle-input` invoked `opencode/kimi-for-coding/k2p6`, captured an extractable unified diff artifact, `senior-swe-bench-retry-execute` evaluated it with a provided local evaluator, `fitness-evidence-inspect --require-all-tests-pass` passed, and the persisted run directory passed a host-local prefix scan.

Scope: local provided-evaluator smoke and retry-artifact portability hardening only. This is not official Senior SWE-Bench mastery, not hidden official holdout proof, and not OS/network no-egress proof.
