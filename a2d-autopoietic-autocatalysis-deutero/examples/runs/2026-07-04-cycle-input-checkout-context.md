# Cycle Input Checkout Context — 2026-07-04

## Purpose

Close the next narrow Senior SWE-Bench integration gap after artifact diagnosis. The previous live artifact was classified as `checkout_context_not_exercised`: the coder asked to inspect a checkout, but providers run artifact-only/no-tools in isolated cwd and cannot inspect a filesystem checkout directly.

## Lineage constraints

- Senior SWE-Bench orchestration remains CLI/evaluation-layer only; `a2d-core` stays benchmark-generic.
- Coding agents must not search GitHub or the public web for solutions.
- Providers must not get mutable checkout access. They receive a bounded read-only snapshot artifact and still return a candidate artifact; evaluator/preflight gates remain authoritative.
- Captured or enriched context is not benchmark fitness evidence.

## Change

`a2d cycle-input <artifact-bundle.json|-> [cycles]` now accepts:

```text
--checkout <dir>
```

When provided, the CLI reads a bounded UTF-8 source/context snapshot from the checkout and injects it into the coder-visible `design` artifact, plus a reserved `benchmark_checkout_context` artifact. User-supplied bundles cannot seed `benchmark_checkout_context` directly.

Safety gates:

- root checkout symlinks are rejected;
- file symlinks are skipped and files are canonicalized/revalidated under the checkout root immediately before read;
- secret-like paths/names (`.env*`, `*secret*`, `*credential*`, `*token*`, key/cert containers, `.npmrc`, `.pypirc`, etc.) and common dependency/build directories are excluded;
- the provider-facing checkout path is redacted as `<benchmark-checkout>`;
- size/file-count limits bound context volume.

## Validation

- `cargo fmt --check`
- `cargo test -p a2d cycle_input -- --nocapture`
- `cargo test` — 309 passed, 2 ignored
- Reviewer pass after blocker fixes for secret leakage and symlink/TOCTOU risks.
- Boundary check: `runs/20260704-cycle-input-checkout-context-evidence/boundary/a2d-core-boundary-rg.txt` has no matches for checkout-context/Senior-SWE-Bench terms in `a2d-core`.

Negative smoke:

- `runs/20260704-cycle-input-checkout-context-evidence/negative-smoke/empty-checkout.err`
- `runs/20260704-cycle-input-checkout-context-evidence/negative-smoke/empty-checkout.status` (`1`)

Fresh source-patch gate:

- `runs/20260704-cycle-input-checkout-context-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`
- `schema_version: a2d.fitness-evidence.v1`
- `actual_tests_evaluated: true`
- `non_regressing: true`
- `fitness: 1.0`
- `failed_cases: []`
- `source_diff_hash: c003e6f4143413a5b40973ae7093ea14516bf12f`, matching `git diff --binary HEAD -- crates | git hash-object --stdin` before commit.

This gates the checkout-context source patch only. It is not official Senior SWE-Bench mastery.
