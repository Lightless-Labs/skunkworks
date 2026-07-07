# Senior SWE-Bench Unterminated Fenced Diff Extraction

**Date:** 2026-07-07
**Scope:** Candidate-patch extraction tolerance for otherwise valid provider artifacts; not official Senior SWE-Bench mastery.

## Lineage

A live Senior SWE-Bench retry-chain smoke produced a coder artifact at `runs/20260707-live-provider-retry-execute-evidence/live-cycle/artifacts/cycle-0-wc-0001-coder-code.artifact` from `opencode/opencode/deepseek-v4-flash-free`. The artifact began with a fenced `diff` block but omitted the closing fence at EOF. Existing extraction treated that as `output_contract_not_followed`, even though the buffered content was a structurally valid unified diff and the later evaluator/preflight gates still bind the exact patch bytes.

Prior constraints still apply:

- public GitHub solution-reference detection runs over the whole artifact before extraction;
- extraction is not fitness evidence;
- `git apply --check` must validate the candidate against the benchmark checkout, not the A²D repo root;
- only `fitness-evidence-inspect --require-all-tests-pass` can permit a fitness claim.

## Change

`extract_fenced_unified_diff` now accepts an open fenced block at EOF only when the buffered content passes the same `looks_like_unified_diff` structural check used for closed fences. Prose-only unterminated fences remain rejected.

## Validation

```bash
cargo fmt --check
CARGO_BUILD_JOBS=2 cargo test -p a2d senior_swe_bench_candidate_patch_extractor_accepts_diff_and_fenced_diff_only -- --nocapture
CARGO_BUILD_JOBS=2 cargo test
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260707-unterminated-fenced-diff-extractor-evidence/actual-test-score-artifact \
  cargo run -q -p a2d -- score-artifact sudoku runs/20260701-score-artifact-fitness-evidence/good-sudoku-artifact.rs
cargo run -q -p a2d -- fitness-evidence-inspect \
  runs/20260707-unterminated-fenced-diff-extractor-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json \
  --require-all-tests-pass
git diff --binary HEAD -- crates | git hash-object --stdin
```

Fresh source-patch gate support: `runs/20260707-unterminated-fenced-diff-extractor-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, full-passing `a2d.fitness-evidence.v1` actual-test evidence with `actual_tests_evaluated: true`, `non_regressing: true`, `fitness: 1.0`, `failed_cases: []`, `source_diff_scope: crates`, and `source_diff_hash: 5ce155fae48cbea4d6786d12c1e91a34b6ec67b8`, matching the scoped crates diff.

The focused extractor regression is the behavior-specific check: it proves the valid unterminated fenced diff is accepted and an unterminated prose-only fence is still rejected. The score-artifact evidence gates the source patch under A²D's current hard invariant; it is not itself an extractor behavior test.

## Interpretation

This closes a live provider artifact-format gap without weakening the no-public-solution-search barrier or the evaluator preflight/evidence gates. It is local extractor hardening plus a bounded live smoke observation, not OS/network no-egress enforcement, hidden-holdout Senior SWE-Bench evidence, official benchmark mastery, or top-level goal completion.
