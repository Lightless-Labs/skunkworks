---
title: "Escalation Rung 6 Should Be Bounded Provider Consensus"
date: 2026-06-01
module: metabolism
tags:
  - escalation
  - provider-routing
  - multi-model
  - fitness
problem_type: architectural-insight
---

# Escalation Rung 6 Should Be Bounded Provider Consensus

## Problem

Rungs 4–5 swap a stuck enzyme to an alternate provider, first with history and then with a clean session. If that still repeats the same behavioral signature, the system needs a differentiated intervention rather than another single-provider retry.

The coder already had a provider portfolio path, but it was specific to code generation and normal coder scheduling. Rung 6 needed a generalized mechanism that could run on any enzyme without waiting for provider process failures.

## Fix

Rung 6 now invokes a bounded provider portfolio when `enzyme_loop_count >= 6`:

- collects role-isolated eligible providers, avoiding providers currently in cooldown;
- caps the portfolio size with `A2D_RUNG6_MAX_PROVIDERS` (default: 3, invalid/zero values ignored);
- invokes candidates sequentially to keep the mechanism simple and bounded for all enzyme types;
- materializes each candidate output and records candidate evaluations in lineage;
- if a candidate produces `code` and a benchmark is attached, selects the highest-fitness candidate;
- otherwise selects the first materialized success, then first success, then first error as the deterministic fallback;
- records non-selected provider successes/failures through the same provider-health mechanisms used by the coder portfolio.

This reuses the candidate-selection logic shared with the existing coder portfolio, so the sandbox remains the oracle where fitness evidence exists.

## Coverage

Added tests proving:

- rung 6 selects the higher-fitness code candidate when a benchmark is available;
- rung 6 records candidate evaluations and lineage rung/swap/clean metadata;
- rung 6 works for non-code enzymes by selecting the first materialized success after an earlier provider failure.

Validation:

```text
cargo test
37 CLI tests + 144 core tests + 11 bootstrap + 7 provider + 1 doctest = 200 passing, 2 ignored
```

## Why bounded sequential first

Parallel consensus would reduce wall-clock in the happy case, but it risks waiting on slow losers and makes every stuck enzyme consume multiple provider windows concurrently. Sequential bounded consensus is easier to reason about, easier to test deterministically, and still provides the key non-prompt intervention: multiple model attempts in one rung-6 invocation with mechanical selection.
