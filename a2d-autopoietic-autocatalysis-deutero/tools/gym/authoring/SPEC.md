# Authoring spec and frozen rubric: one task family for the A²D gym v3 (dev benchmark)

You are authoring benchmark tasks that measure whether coding systems produce *working* software. A coder model receives ONLY `requirements` and the public tests; hidden tests then decide pass/fail. Your family brief is `FAMILY.md`; your family's rubric class is given in `INSTRUCTIONS.md`.

This file is the gym's frozen rubric. It is hashed with the suite (certification and every pilot record its sha256). It was written before any brief. Plan: `docs/plans/2026-10-21-gym-v3.md`.

## Output layout (write under ./out/<family>/)

```
out/<family>/<family>-l1/ ... <family>-l4/
  task.json          {"id": "<family>-lN", "family": "<family>", "requirements": "...", "test_timeout_secs": 10}
  public_tests.rs    3–4 #[test] functions (no module wrapper)
  hidden_tests.rs    20–35 #[test] functions (no module wrapper)
  reference.rs       a correct, complete solution
  mutants/m1.rs .. m4.rs   faulty variants of reference.rs
  rubric.json        {"difficulty": [...], "mutants": {"m1": {...}, ..., "m4": {...}}}
out/<family>/REPORT.md
```

## Rules

1. **Single file, std only, Rust 2024.** `requirements` must say: implement in Rust as a SINGLE source file that compiles with `rustc --edition 2024`, std only, and "Output ONLY the Rust source code inside ```rust fences." It gives the exact public API (types, `pub fn` signatures, error enums with exact variants) so tests can call it. All API items `pub`. No `#![...]` inner attributes or `//!` docs in reference/mutants.
2. **Numbered clauses.** Every requirement is a clause that starts on its own line as `R<n>. ` (R1, R2, … in order, no gaps). l1 has at least 15 clauses; l3 and l4 at least 10 (depth tasks) and as many as the behaviour needs. Each clause is stated affirmatively.
3. **Complete, unambiguous spec.** Every behaviour a hidden test checks follows from the requirements text. A careful engineer reading only the requirements must be able to pass every hidden test. See the forbidden list below.
4. **Tests.** Tests call only the specified API, are deterministic, and run fast. They are module contents placed in a module that does `use super::*;`. Hidden tests use inputs different from public tests. No `#[should_panic]` unless the spec requires a panic.
5. **Mutants.** 4 per task. Each must FAIL at least one hidden test by a failing assertion (a compile error is not a catch; a timeout is a catch only for the `naive` mutant of a scale task). At least 2 of the 4 must PASS all public tests.
6. **Verify everything** with `./check.sh <file.rs> <tests.rs>` (exit 0 = all pass): reference passes public and hidden; every mutant fails hidden; at least 2 mutants pass public. Fix and re-check until true.
7. Finish with `out/<family>/REPORT.md`: one line per task with the check results, plus anything you could not verify.
8. Before finishing, run `./api_check.sh <task>/reference.rs <task>/public_tests.rs` and the same for hidden_tests.rs: both must print API_OK.

Work only inside the current directory.

## Frozen rubric

### Allowed difficulty sources
Each task lists its sources in `rubric.json` `"difficulty"`: at least 2 at l1/l2, at least 3 at l3/l4. One of them is always your family's rubric class.
- **depth:** at least 10 numbered requirements at l3/l4, at least 3 of which modify another clause (a precedence or an exception that names the clause it modifies, e.g. "R9. … overrides R4 when …"). Each clause is stated affirmatively.
- **state:** an explicit transition table in the requirements (at least 5 states at l4), with hidden tests that replay sequences of at least 5 operations.
- **scale:** an explicit size bound and the required complexity in the text. Validation shows the reference within 5% of the per-test time limit (`test_timeout_secs`), and a mutant labelled `naive` (the obvious slower algorithm) that exceeds the limit, so it is at least 20× slower than the reference, under the validator's parallelism.
- **surface:** at least 6 public items whose verbatim signatures are in the requirements.
- **edges:** every boundary a hidden test checks is named in the text.

### Forbidden
- Testing any unwritten convention: ordering, tie-break, rounding, overflow, error precedence, formatting, case, Unicode, float tolerance. Each of these may be tested only when a clause states it.
- Exact error or `Debug` text that the requirements do not quote.
- Float equality without a stated tolerance.
- HashMap-order or time-dependent outputs.
- A literal example in the requirements that a hidden test contradicts.
- "Trap" wording. An intended misreading is stated as the rule *plus* the rejected reading ("… sorted by descending id, not by arrival order").

### Mechanical checks (enforced by the pipeline; a failing task is rejected and repaired)
- **M1 traceability.** Every hidden test has a comment line directly above its `#[test]` of exactly the form `// R3` or `// R3,R7` naming the clauses it checks. Every named clause exists, and every clause is named by at least one hidden test.
- **M2 mutants.** `rubric.json` labels each mutant: `{"violates": ["R<n>", ...], "kind": "misreading" | "bug" | "naive", "refuted_by": "<sentence>"}`. At least 2 of the 4 are plausible misreadings (`"kind": "misreading"`), each refuted by an explicit sentence: `refuted_by` is copied verbatim (at least 8 words) from one of the clauses the mutant violates.
- **M3 adjudication.** When the independent solver fails a hidden test, two independent adjudicators judge it. A DETERMINED verdict quotes at least 8 words verbatim from the requirements (checked as a substring) and names the excluded reading. The test counts as determined only if both agree with valid quotes; otherwise the task gets a requirements-only repair (hidden tests are never edited).
- **M4 spec review of every task.** The second adjudicator also reviews 2 seeded hidden tests per task that the solver passed, under the same rule.
- **M5 opaque names.** Hidden test functions are named `h_<hex>` (at least 8 lowercase hex digits, all distinct). These names appear nowhere in the requirements, the public tests or the reference.

### Selection policy (not an authoring rule; disclosed here so the rubric is complete)
- **Never filter on solver solvability.** A task the independent solver fails stays when the requirements determine the tested behaviour (both adjudicators agree); a solver failure only routes the task to adjudication.
- **Class quotas** are equal over the five difficulty sources above (the neutral taxonomy: depth, state, scale, surface, edges), fixed before any brief. Their origin is disclosed: the C2-FA2 failure taxonomy of cycle 2 (boundary/edge, semantics/state model, performance/complexity, precedence, exact format, API contract, overflow).
