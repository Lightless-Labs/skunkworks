---
module: senior-swe-bench
tags: [artifact-safety, no-search, encoded-references, benchmark-integrity]
problem_type: best-practice
---

# Senior SWE-Bench artifact detectors must normalize encoded references

## Problem

No-public-solution-search gates that only inspect raw text can miss copied public GitHub references hidden in transport encodings such as percent encoding or base64/base64url.

## Practice

Run candidate artifacts through bounded normalization layers before extraction/evaluation:

- raw text normalization for GitHub hosts, refs, issues, pulls, commits, and CLI search commands;
- bounded repeated percent decoding;
- bounded HTML/XML entity decoding;
- bounded base64/base64url token decoding over reviewed decoded layers;
- then re-use the same normalized detector on decoded text.

Keep the detector bounded and conservative: cap decode depth/entity length/token length, avoid recursive base64 decoding, and keep benign local metadata regressions so ordinary long base64-looking or entity-bearing notes do not fail closed.

## Evidence

The 2026-07-07 base64 detector slice adds regressions for base64/base64url GitHub URLs, assignment-style `source=<base64>` metadata, encoded `refs/pull`, encoded `raw.githubusercontent.com`, and benign local base64-looking notes. Fresh source-patch evidence: `runs/20260707-senior-swe-bench-base64-reference-detection-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`.

The 2026-07-08 HTML/entity detector slice adds bounded entity normalization and layer composition regressions for `github&#46;com`, zero-padded decimal entities, `github&period;com`, `refs&#x2f;pull`, HTML→percent, percent→HTML, nested `&amp;#...`, and percent/entity-obfuscated base64 tokens. Fresh source-patch evidence: `runs/20260708-senior-swe-bench-html-entity-reference-detection-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json` with source diff hash `b21036b7853f831fdf428422248e50d36b7d8c9e`.

Scope: detector hardening only; not official Senior SWE-Bench mastery or OS/network no-egress proof.
