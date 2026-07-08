# Senior SWE-Bench HTML/entity public-reference detection — 2026-07-08

## Purpose

Harden Senior SWE-Bench artifact diagnosis/selection against public GitHub solution references hidden behind HTML/XML character entities and mixed percent/entity/base64 layers.

## Lineage

Prior slices rejected raw GitHub URLs/refs, raw-content hosts, common obfuscated host spellings, GitHub CLI commands, bounded percent/nested-percent encodings, and base64/base64url tokens. A remaining bypass was entity-encoded punctuation such as `github&#46;com`, `github&period;com`, or `refs&#x2f;pull`, plus mixed layers like percent-encoded entities and percent/entity-obfuscated base64 tokens.

## Change

- `contains_public_github_solution_reference` now evaluates bounded percent/entity decoded layers.
- Each decoded layer runs the normal host/ref/CLI detector and, for outer artifact scans, the bounded base64/base64url detector.
- HTML entity decoding is bounded by maximum entity length and existing decode-pass caps; benign local entity metadata remains accepted.
- Regression coverage rejects decimal, zero-padded decimal, hex, and named entity GitHub hosts; entity-encoded pull refs; HTML→percent and percent→HTML forms; nested `&amp;#...` forms; and percent/entity-obfuscated base64 GitHub URL tokens.

## Validation

```bash
cargo fmt --check
CARGO_BUILD_JOBS=2 cargo test -p a2d --test senior_swe_bench_select_candidate_artifact -- --nocapture
CARGO_BUILD_JOBS=2 cargo test -p a2d --test senior_swe_bench_diagnose_artifact -- --nocapture
CARGO_BUILD_JOBS=2 cargo test
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260708-senior-swe-bench-html-entity-reference-detection-evidence/actual-test-score-artifact \
  cargo run -q -p a2d -- score-artifact sudoku runs/20260701-score-artifact-fitness-evidence/good-sudoku-artifact.rs
cargo run -q -p a2d -- fitness-evidence-inspect \
  runs/20260708-senior-swe-bench-html-entity-reference-detection-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json \
  --require-all-tests-pass
```

Fresh source-patch evidence: `runs/20260708-senior-swe-bench-html-entity-reference-detection-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`.

Evidence hash / scoped crates diff hash: `b21036b7853f831fdf428422248e50d36b7d8c9e`.

Postcommit clean-head replay support evidence: `runs/20260708-postcommit-fitness-evidence-html-entity/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json` (`source_tree_dirty: false`, clean crates diff hash `e69de29bb2d1d6434b8b29ae775ad8c2e48c5391`).

Scope: artifact safety hardening only. This is not official Senior SWE-Bench mastery, not hidden official holdout evidence, and not OS/network no-egress proof.
