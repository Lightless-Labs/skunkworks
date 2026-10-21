# Task: adjudicate hidden tests of one benchmark family

Work only inside the current directory. Network access is not needed and must not be used. Do not read anything outside this directory. You are one of two independent adjudicators; you will never see the other's work.

An independent programmer solved each task below from its `requirements` and public tests alone; they never saw the hidden tests. For every line of `LIST.tsv` (`<task id><TAB><test name><TAB><failing|sampled>`) decide whether the **requirements text** determines the exact behaviour that hidden test checks. `failing` lines are hidden tests the programmer's submission failed; `sampled` lines are hidden tests it passed, drawn at random for a spec review.

For each task `<id>` in `tasks/`:
- `tasks/<id>/task.json` — the task (its `requirements` field is what the programmer read);
- `tasks/<id>/public_tests.rs`, `tasks/<id>/hidden_tests.rs`, `tasks/<id>/reference.rs`, `tasks/<id>/mutants/`;
- `solver/<id>/submission.rs` and `solver/<id>/hidden_output.txt` — the programmer's submission and its hidden-test run.

## Decision rule (per listed test)
- **DETERMINED:** a careful programmer reading the requirements (and public tests) could have known the exact expected behaviour. A requirement that is explicit but easy to overlook is still DETERMINED.
- **UNDERDETERMINED:** the requirements leave the tested behaviour open, so a reasonable reading differs from the test: an unstated convention (ordering, tie-break, rounding, overflow, error precedence, formatting, case, Unicode, float tolerance), a contradiction between clauses, or behaviour that only the reference reveals.
- Judge from the requirements text, never from what the submission did. Do not mark a test UNDERDETERMINED because it is hard.
- If the submission did not compile, the list names the task as a whole under the test name `*`: DETERMINED unless the compile failure stems from an ambiguity in the requested API.

## Output
Write `VERDICTS.jsonl`: exactly one JSON object per line of `LIST.tsv`, and nothing else:
`{"task": "<id>", "test": "<test name or *>", "verdict": "DETERMINED" | "UNDERDETERMINED", "quote": "...", "excluded_reading": "..."}`
- A DETERMINED verdict counts only if `quote` is copied **verbatim** (character for character, same spacing and line breaks; at least 8 consecutive words) from a clause that the test's own `// R<n>` comment in `hidden_tests.rs` names (for `*`: from any numbered clause), never from the text before `R1.`, and states the rule the test checks; and `excluded_reading` names, in at least 4 words, the other reading of the requirements that the quote rules out.
- For UNDERDETERMINED, `quote` and `excluded_reading` may be empty strings.

Do not change any other file. Your final message must be a single line: `ADJUDICATED <number of DETERMINED lines> <number of UNDERDETERMINED lines>`.
