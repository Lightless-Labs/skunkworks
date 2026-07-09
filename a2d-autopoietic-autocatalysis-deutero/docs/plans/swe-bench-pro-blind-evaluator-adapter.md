# Plan: SWE-Bench Pro Blind Evaluator Adapter

**Created:** 2026-07-09
**Status:** Blocked on reviewed SWE-Bench Pro access/evaluator artifact

## Objective

Allow A²D to iterate against SWE-Bench Pro without exposing benchmark sources, hidden tests, or solution material to coding agents. A²D may receive only bounded public task context, an instance/task id, mechanical pass/fail/metrics, and sanitized retry feedback. Any accepted self-improvement remains gated by fresh non-regressing `a2d.fitness-evidence.v1`.

## Current blocker

The repo currently contains Senior SWE-Bench/Snorkel integration plumbing and one committed local-wrapper self-iteration proof (`d46777f`). A read-only check of `https://senior-swe-bench.snorkel.ai/tasks` identifies that endpoint as Senior SWE-Bench; no distinct reviewed SWE-Bench Pro task/evaluator/access artifact is present in the repo or environment. `a2d swe-bench-pro-readiness` must therefore report blocked and must not treat Senior SWE-Bench manifests as Pro.

## Information barrier

- Coder-visible input: public task id, public problem statement/context, allowed local checkout summary, no-search policy, and sanitized prior pass/fail summaries.
- Evaluator-only input: benchmark sources, hidden tests, secret oracle data, official evaluator credentials/tokens, and solution/reference patches.
- Persisted evidence: `a2d.fitness-evidence.v1` with evaluator kind/provenance, source diff hash, candidate patch hash, non-regression status, and redacted case labels.
- Never feed hidden evaluator stdout/stderr, reference solution diffs, credentials, or benchmark-private files into `cycle-input`, retry feedback, provider prompts, docs examples, or committed run artifacts.

## Required slices

1. **Access/readiness gate (current slice):** block without a reviewed Pro manifest/access artifact; reject Senior SWE-Bench manifests as Pro; do not load benchmark sources or solutions.
2. **Reviewed Pro manifest schema:** define a minimal `a2d.swe-bench-pro-access-manifest.v1` containing benchmark identity, instance id, public context path/hash, sealed evaluator command/adapter path, hidden-holdout declaration, and no-solution-search policy. The manifest must not contain sources, hidden tests, solution patches, or credentials.
3. **Blind evaluator adapter:** run the sealed evaluator in an isolated process/checkout, provide candidate patch by hash/path, parse only pass/fail/metrics, and emit local evaluation JSON with hidden output redacted by default.
4. **Fitness evidence export:** convert successful sealed evaluator output to `a2d.fitness-evidence.v1`; require `fitness-evidence-inspect --require-all-tests-pass` before persistence or self-improvement claims.
5. **Retry feedback boundary:** transform failed sealed evaluations into bounded next-cycle feedback that contains no hidden test names/output and no solution references.
6. **Live Pro smoke:** only after a reviewed manifest exists, run one instance end-to-end and document limitations honestly.

## Acceptance criteria

- Readiness refuses to start without reviewed Pro access.
- Senior SWE-Bench local/official manifests cannot be mislabeled as SWE-Bench Pro.
- Hidden/solution material is never present in coder-visible JSON or provider artifacts.
- Every source change is committed only with fresh matching `a2d.fitness-evidence.v1` source diff evidence.
- Official Pro claims require sealed evaluator provenance; local-wrapper evidence cannot satisfy them.
