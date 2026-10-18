# Authoring spec: one task family for the A²D lift benchmark

You are authoring benchmark tasks that measure whether coding systems produce *working* software. A coder model will receive ONLY `requirements` (plus the public tests); hidden tests then decide pass/fail. Your family: see FAMILY.md in this directory.

## Output layout (write under ./out/<family>/)

```
out/<family>/<family>-l1/ ... <family>-l4/
  task.json          {"id": "<family>-lN", "family": "<family>", "requirements": "...", "test_timeout_secs": 10}
  public_tests.rs    3–4 #[test] functions (no module wrapper)
  hidden_tests.rs    20–35 #[test] functions (no module wrapper), together covering every numbered clause
  reference.rs       a correct, complete solution
  mutants/m1.rs .. m4.rs   realistic faulty variants of reference.rs
```

## Rules

1. **Single file, std only, Rust 2024.** `requirements` must say: implement in Rust as a SINGLE source file that compiles with `rustc --edition 2024`, std only, and "Output ONLY the Rust source code inside ```rust fences." It must give the exact public API (types, `pub fn` signatures, error enums with exact variants) so tests can call it. All API items `pub`. No `#![...]` inner attributes or `//!` docs in reference/mutants.
2. **Complete, unambiguous spec.** Every behaviour a hidden test checks must follow from the requirements text: list edge cases and error conditions explicitly (empty input, boundaries, ordering, tie-breaking, error variant and position). A careful engineer reading only the requirements must be able to pass every hidden test. Never test unspecified behaviour.
3. **Graded difficulty — this is version 2; v1 was far too easy.** Measured on v1: the coder model (a strong model at low reasoning effort, one shot, no compiler) passed every hidden test on 19 of 24 tasks, including all levels of dependency resolution, LRU repair, scheduling and nested transactions. It only failed on (a) a base64 codec with byte-exact error offsets and canonical-encoding rules, and (b) the hardest semver level (range semantics with pre-release rules). Well-known algorithms are solved from memory; what defeats one-shot generation is **many precise, interacting, non-standard clauses** — exact error variants and positions, error precedence when several apply, canonical/strict-format rules, and state that crosses features.
   Therefore:
   - Number every requirement clause in `requirements` (e.g. `R1.`, `R2.` …). l1 has ≥ 15 clauses, l4 ≥ 35. Each hidden test cites the clauses it checks in a comment.
   - Every level must include custom (non-textbook) rules that a model cannot recall from training data — e.g. specified error precedence, specified tie-breaks, specified output formats.
   - Target one-shot pass rates: l1 ≈ 70%, l2 ≈ 50%, l3 ≈ 35%, l4 ≈ 20%. Still: every clause must be unambiguous, and a careful engineer reading only the requirements must be able to pass all hidden tests.
   - Reference solutions up to ~600 lines.
4. **Tests.** Tests call only the specified API, are deterministic, and run fast (< 1 s total). Tests are module contents: they will be placed in a module that does `use super::*;`. Hidden tests use inputs different from public tests and have different function names. Public tests cover the basic happy path of each major API item; hidden tests cover edge cases, error paths, interactions and larger inputs. No `#[should_panic]` unless the spec requires a panic.
5. **Mutants.** 4 per task, each a realistic bug a capable engineer could make (off-by-one, missed spec clause, wrong error variant or position, wrong ordering/tie-break, partial feature). Each mutant must FAIL at least one hidden test. At least 2 of the 4 must PASS all public tests (so hidden tests add discrimination).
6. **Verify everything** with `./check.sh <file.rs> <tests.rs>` (exit 0 = all pass): reference passes public and hidden; every mutant fails hidden; ≥2 mutants pass public. Fix and re-check until true. If `./check.sh` cannot run the compiler (e.g. it hangs > 60 s), say so in REPORT.md and continue writing carefully.
7. Finish with `out/<family>/REPORT.md`: one line per task with the check results (reference public/hidden, each mutant public/hidden), plus anything you could not verify.

Work only inside the current directory.

8. Before finishing, also run `./api_check.sh <task>/reference.rs <task>/public_tests.rs` and the same for hidden_tests.rs: both must print API_OK (tests may use only the public API named in the requirements).
