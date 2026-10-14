---
title: "Blind Holdout Authoring: Confirmation Sets the Developer Never Reads"
date: 2026-10-14
category: best-practices
module: a2d-lift
problem_type: experiment_design
component: holdouts
symptoms:
  - "Confirmation tasks must never reach the developing agent, yet must be validated and repaired"
  - "References share an author with the tests, so 'reference passes' proves only consistency"
root_cause: holdout_quality_without_inspection
---

# Blind Holdout Authoring

Confirmation-1 (148 tasks) and Confirmation-2 (48 tasks) were produced, validated, repaired and audited without the developing agent (Claude) reading any task content. The only things it saw were task ids, family names it had written itself, verdict lines, counts and hashes.

## Pipeline

1. **Spec.** A fixed authoring spec (`SPEC.md`, v2) and a one-paragraph `FAMILY.md` per family, written before anything else. Difficulty was calibrated on a visible calibration suite built from the same spec (E03 saturated, so the spec was redesigned once; E04 passed headroom).
2. **Authoring.** `codex exec` runs with `gpt-6.1-sol` at high effort, one job per family, 6 in parallel, each in `$A2D_HOLDOUT_DIR/authoring/<set>/<family>/` (outside the repo):
   - `--sandbox workspace-write`, `-c approval_policy="never"`, `--ignore-user-config`, stdin closed;
   - a `check.sh` that mirrors the oracle layout;
   - Codex's own `rustc` via the toolchain path, because the rustup proxy hangs inside its sandbox.
   The driver records exit codes only. Never read `codex.out`, `codex.err` or `REPORT.md`.
3. **Assembly.** `assemble-holdout.py` copies only contract files (`task.json`, `public_tests.rs`, `hidden_tests.rs`, `reference.rs`, `mutants/*.rs`) and prints task ids and counts.
4. **Validation.** `a2d lift validate` prints verdict lines only. It enforces:
   - the reference passes public and hidden tests, deterministically;
   - at least 4 mutants, each compiling and failing a hidden *assertion*;
   - at least 2 mutants pass public (hidden tests add discrimination);
   - at least 3 public and at least 20 hidden tests;
   - public tests call every `pub fn` the requirements name;
   - tests compile against the reference **built as a separate crate**, i.e. through the public API only.
5. **Blind repair.** Rejections go back to a fresh Codex job in the family dir. It gets only the verdict lines (`FIX-VERDICTS.txt`), a fix brief and `api_check.sh`, and its final message must be a single summary line.
6. **Independent-solve audit.** `a2d lift run --arms D --model codex/gpt-6.1-sol --k 3 --unregistered` on the set, with output inside the holdout dir. Any task whose independent solution fails hidden tests is staged (`stage-audit.py`) for an adjudicator (`ADJUDICATE.md`). The adjudicator classifies each failing test as DETERMINED or UNDERDETERMINED by the requirements and repairs the task, never dropping it. Here the audit passed 148/148 and 48/48, so no adjudication was needed.
7. **Hash.** `a2d lift hash` is computed after the last repair and logged. Runs on holdouts write to `$A2D_HOLDOUT_DIR/runs/`. Only a content-free manifest, the ledger and the summary come into the repo.

## Pitfalls hit

- **Validator false positives cost blind repairs.** `pub fn Rope::from_str` was parsed as a function named `Rope`, and four tasks were rejected wrongly. Run new validation rules against a visible suite first. This one was caught because the rejections followed an obvious pattern.
- **Authoring jobs and registered runs compete for the same account.** Six parallel Sol jobs slowed Luna calls from about 23 s to about 65 s (E05). Pause the driver at batch boundaries (`kill -STOP` while it waits; in-flight jobs finish).
- **Transient "model at capacity" failures** leave a family with no output and `exit=1`. Move the markers aside and rerun only that family.
- **`pgrep -f '<pattern>'` inside a wait loop matches the loop itself**, so the loop never ends. Wait on files (`grep -q` in a ledger or log), or break the pattern with a character class (`'SP[E]C'`).
