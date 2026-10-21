# Task: requirements-only repair of benchmark tasks

Work only inside the current directory. Network access is not needed and must not be used. Do not read anything outside this directory.

The hidden tests listed in `LIST.tsv` (`<task id><TAB><test name>`) were not found to be determined by their task's requirements by two independent adjudicators. For each task `<id>` in `tasks/` (`task.json`, `public_tests.rs`, `hidden_tests.rs`, `reference.rs`, `mutants/`, and the `SPEC.md` rubric), write `out/<id>/task.json`: the same task with **only its `requirements` text changed**, so that it states the behaviour each listed test checks.

- Do not write any other file. Hidden tests, public tests, the reference and the mutants are never changed.
- Keep exactly the same numbered clauses `R1.`, `R2.`, … `Rn.`: **reword or extend existing clauses only, and never add a new clause id** (every clause must stay covered by a hidden test's `// R<n>` comment, and hidden tests are never edited). Write the missing behaviour into the clause that the listed test's `// R<n>` comment names. State each rule affirmatively, and an intended misreading as the rule plus the rejected reading (SPEC.md "Forbidden").
- The repaired text must still satisfy SPEC.md's mechanical checks (M1, M2's `refuted_by` quotes in `rubric.json`, M5, the clause floor and every difficulty source's evidence); a repair that breaks one is refused and the task is excluded.
- Never word a requirement to match a submission's output, and keep the task as difficult as it was.

Your final message must be a single line: `REPAIRED <number of tasks written>`.
