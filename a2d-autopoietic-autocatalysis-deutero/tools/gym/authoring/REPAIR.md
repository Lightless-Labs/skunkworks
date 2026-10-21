# Task: requirements-only repair of benchmark tasks

Work only inside the current directory. Network access is not needed and must not be used. Do not read anything outside this directory.

The hidden tests listed in `LIST.tsv` (`<task id><TAB><test name>`) were not found to be determined by their task's requirements by two independent adjudicators. For each task `<id>` in `tasks/` (`task.json`, `public_tests.rs`, `hidden_tests.rs`, `reference.rs`, `mutants/`, and the `SPEC.md` rubric), write `out/<id>/task.json`: the same task with **only its `requirements` text changed**, so that it states the behaviour each listed test checks.

- Do not write any other file. Hidden tests, public tests, the reference and the mutants are never changed.
- Keep the numbered clauses `R1.`, `R2.`, … in order with no gaps, and keep every existing clause id; add a new clause at the end, or reword an existing one, so that the behaviour is written down. State each rule affirmatively, and an intended misreading as the rule plus the rejected reading (SPEC.md "Forbidden").
- Never word a requirement to match a submission's output, and keep the task as difficult as it was.

Your final message must be a single line: `REPAIRED <number of tasks written>`.
