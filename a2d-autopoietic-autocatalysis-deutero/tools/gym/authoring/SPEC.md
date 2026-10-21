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
  rubric.json        {"difficulty": [...], "mutants": {"m1": {...}, ..., "m4": {...}}, <evidence per tag, below>}
out/<family>/REPORT.md
```

## Rules

1. **Single file, std only, Rust 2024.** `requirements` must say: implement in Rust as a SINGLE source file that compiles with `rustc --edition 2024`, std only, and "Output ONLY the Rust source code inside ```rust fences." It gives the exact public API (types, `pub fn` signatures, error enums with exact variants) so tests can call it. All API items `pub`. No `#![...]` inner attributes or `//!` docs in reference/mutants.
2. **Numbered clauses.** Every requirement is a clause that starts on its own line as `R<n>. ` (R1, R2, … in order, no gaps; ASCII digits). l1 and l2 have at least 5 clauses; l3 and l4 at least 10, whatever the difficulty tags, and as many as the behaviour needs. Each clause is stated affirmatively. Text before `R1.` (the rule-1 boilerplate) is not a clause.
3. **Complete, unambiguous spec.** Every behaviour a hidden test checks follows from the requirements text. A careful engineer reading only the requirements must be able to pass every hidden test. See the forbidden list below.
4. **Tests.** Tests call only the specified API, are deterministic, and run fast. They are module contents placed in a module that does `use super::*;`. Hidden tests use inputs different from public tests. No `#[should_panic]` unless the spec requires a panic.
5. **Mutants.** 4 per task. Each must FAIL at least one hidden test by a failing assertion (a compile error is not a catch; a timeout is a catch only for the `naive` mutant of a scale task). At least 2 of the 4 must PASS all public tests.
6. **Verify everything** with `./check.sh <file.rs> <tests.rs>` (exit 0 = all pass): reference passes public and hidden; every mutant fails hidden; at least 2 mutants pass public. Fix and re-check until true.
7. Finish with `out/<family>/REPORT.md`: one line per task with the check results, plus anything you could not verify.
8. Before finishing, run `./api_check.sh <task>/reference.rs <task>/public_tests.rs` and the same for hidden_tests.rs: both must print API_OK.

Work only inside the current directory.

## Frozen rubric

### Allowed difficulty sources
Each task lists its sources in `rubric.json` `"difficulty"`: at least 2 at l1/l2, at least 3 at l3/l4. One of them is always your family's rubric class. Every listed source carries its **structured evidence** in `rubric.json`, which the pipeline checks against the text (quoted strings are matched as literal substrings, with no whitespace or case normalisation):
- **depth:** at least 10 numbered requirements at l3/l4, at least 3 of which modify another clause (a precedence or an exception that names the clause it modifies, e.g. "R9. … overrides R4 when …"). Each clause is stated affirmatively.
  Evidence: `"modifiers": [{"clause": "R9", "modifies": "R4"}, ...]`: at least 3 distinct modifying clauses at l3/l4 (at least 1 at l1/l2); each modifying clause's own text names the clause it modifies (`R4` as a reference).
- **state:** an explicit transition table in the requirements (at least 5 states at l4), with hidden tests that replay sequences of at least 5 operations.
  Evidence: `"states": [...]`: at least 5 distinct state names at l4 (at least 3 otherwise), each appearing verbatim in the requirements; `"sequences": [...]`: the names of the hidden tests that replay the sequences (at least 1; these names are opaque and stay in the sealed rubric).
- **scale:** an explicit size bound and the required complexity in the text. **The binding timing rule:** the reference's hidden-test run uses at most 5% of `test_timeout_secs`, and the mutant labelled `naive` (the obvious slower algorithm: it passes the public tests) exceeds the limit on the hidden tests (it times out), so it is at least 20× slower than the reference. (The plan's phrase "20× the limit" was a drafting error; the rule is 20× the reference.) The validator applies a margin: the larger of libtest's own time and the wall clock must be at most 4% of the limit, measured under the validator's parallelism.
  Evidence: `"scale": {"bound": "...", "complexity": "..."}`: both verbatim in the requirements, the bound with a number.
- **surface:** at least 6 public items whose verbatim signatures are in the requirements.
  Evidence: `"signatures": [...]`: at least 6 distinct items, each a verbatim substring of the requirements that starts with `pub fn|struct|enum|trait|type|const <name>`.
- **edges:** every boundary a hidden test checks is named in the text.
  Evidence: `"boundaries": [...]`: at least 1, each at least 3 words and a verbatim substring of the requirements.
- **What stays semantic (disclosed):** that a transition table is complete and the `sequences` tests really replay 5 or more operations; that the stated complexity is the one the reference meets; that `boundaries` lists *every* boundary a hidden test checks; that a modifier is a genuine precedence or exception. These are reviewed by the adjudicators (M3, M4), not checked mechanically.

### Forbidden
- Testing any unwritten convention: ordering, tie-break, rounding, overflow, error precedence, formatting, case, Unicode, float tolerance. Each of these may be tested only when a clause states it.
- Exact error or `Debug` text that the requirements do not quote.
- Float equality without a stated tolerance.
- HashMap-order or time-dependent outputs.
- A literal example in the requirements that a hidden test contradicts.
- "Trap" wording. An intended misreading is stated as the rule *plus* the rejected reading ("… sorted by descending id, not by arrival order").

### Mechanical checks (enforced by the pipeline; a failing task is rejected and repaired)
- **M1 traceability.** Every hidden test has a comment line directly above its `#[test]` of exactly the form `// R3` or `// R3,R7` naming the clauses it checks; a trace line names at most 3 clauses. Every named clause exists, and every clause is named by at least one hidden test whose trace names at most 3 clauses.
- **M2 mutants.** `rubric.json` labels each mutant: `{"violates": ["R<n>", ...], "kind": "misreading" | "bug" | "naive", "refuted_by": "<sentence>"}`. At least 2 of the 4 are plausible misreadings (`"kind": "misreading"`), each refuted by an explicit sentence: `refuted_by` is copied verbatim (a literal substring, at least 8 words, no whitespace changes) from one of the clauses the mutant violates.
- **M3 adjudication.** When the independent solver fails a hidden test, two independent adjudicators judge it. A DETERMINED verdict quotes at least 8 words verbatim (a literal substring, no whitespace changes) from a clause that the test's own `// R<n>` trace names (for a submission that did not compile, `*`: from any numbered clause; the text before R1 never counts) and names the excluded reading. The test counts as determined only if both agree with valid quotes; otherwise the task gets a requirements-only repair (hidden tests are never edited after the audit).
- **M4 spec review of every task.** The second adjudicator also reviews 2 seeded hidden tests per task under the same rule: tests the solver passed (positive evidence: the audit's record that every hidden test passed, or the hidden output's passing lines), or any hidden tests when the submission did not compile or both audit attempts were void. Without that evidence the task stays pending.
- **M5 opaque names.** Hidden test functions are named `h_<hex>` (at least 8 lowercase hex digits, all distinct). No name occurs anywhere in the requirements, the public tests or the reference, not even inside a longer word (literal substring check); the same check guards everything the pipeline publishes.

### Selection policy (not an authoring rule; disclosed here so the rubric is complete)
- **Never filter on solver solvability.** A task the independent solver fails stays when the requirements determine the tested behaviour (both adjudicators agree); a solver failure only routes the task to adjudication.
- **Class quotas** are equal over the five difficulty sources above (the neutral taxonomy: depth, state, scale, surface, edges), fixed before any brief. Their origin is disclosed: the C2-FA2 failure taxonomy of cycle 2 (boundary/edge, semantics/state model, performance/complexity, precedence, exact format, API contract, overflow).
- **Regression stratum (rounding registered):** every E007 4/4 family of every wave is a regression candidate; at final selection the stratum is drawn once, seeded, over the pooled candidates: k = round(0.175 × N) clamped to [ceil(0.15 N), floor(0.20 N)], where N is the final number of kept (1–3 of 4) dev families; when that range is empty, the nearest feasible integer to [0.15 N, 0.20 N] is used, and k never exceeds the candidates. Both cases are disclosed with the achieved share.
