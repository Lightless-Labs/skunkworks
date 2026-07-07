---
module: a2d-cli
tags: [senior-swe-bench, fitness-evidence, retry, portability, provenance]
problem_type: evidence-portability
---

# Senior SWE-Bench evaluation artifacts must be portable

## Problem

A bounded live-provider retry-chain smoke produced passing local-wrapper `a2d.fitness-evidence.v1`, but the artifact was not persistence-ready: it was tied to an older source revision and its local-evaluation/evidence JSON embedded host-local paths for the candidate patch preflight command and isolated temp evaluator checkout.

That makes otherwise useful retry evidence hard to commit, replay, or inspect from another machine/CWD.

## Decision

Senior SWE-Bench local-evaluation and fitness-evidence artifacts now serialize in-project paths through retry artifact path semantics, so project artifacts are repo-relative. Isolated patched evaluator checkouts outside the project root are represented as the explicit marker `isolated_temp_checkout` instead of a `/var/...` or `/tmp/...` path.

The binding validators resolve repo-relative candidate patch/artifact paths before comparison and accept the isolated-temp marker only for `evaluator_checkout_mode: isolated_copy`.

## Evidence

Fresh source-patch gate: `runs/20260707-senior-swe-bench-evaluation-artifact-path-sanitization-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, full-passing `a2d.fitness-evidence.v1`, `source_diff_hash: d45429fdb7effe5b37436743bdb2442885a51fea`.

Validation included focused retry-attempt-evaluate coverage, full retry-execute coverage, full `CARGO_BUILD_JOBS=2 cargo test`, and `fitness-evidence-inspect --require-all-tests-pass`.

This is local-wrapper artifact portability hardening only, not official Senior SWE-Bench mastery or OS/network no-egress proof.
