# Task: adjudicate failing hidden tests of one benchmark family

Work only inside the current directory. Network access is not needed and must not be used. Do not read anything outside this directory.

An independent programmer solved each task below from its `requirements` and public tests alone; they never saw the hidden tests. Their submission failed some hidden tests. Your job is to decide, for every failing hidden test, whether the **requirements text** determined the behaviour the test checks.

For each task `<id>` in `tasks/`:
- `tasks/<id>/task.json` — the task (its `requirements` field is what the programmer read);
- `tasks/<id>/public_tests.rs` — the tests the programmer saw;
- `tasks/<id>/hidden_tests.rs` — the hidden tests;
- `tasks/<id>/reference.rs` — the reference solution;
- `solver/<id>/submission.rs` — the programmer's submission;
- `solver/<id>/hidden_output.txt` — the hidden-test run of the submission (the `FAILED` lines name the failing tests).

## Decision rule (per failing hidden test)
- **DETERMINED:** a careful programmer reading the requirements (and public tests) could have known the exact expected behaviour. A requirement that is explicit but easy to overlook is still DETERMINED. Missing an explicitly specified order of checks, boundary, format, error case or tie rule is the programmer's mistake, not the task's.
- **UNDERDETERMINED:** the requirements leave the tested behaviour open, so a reasonable reading differs from the test. This covers an unstated tie-break, an unspecified format detail, a contradiction between clauses, or behaviour that only the reference implementation reveals.
- Judge from the requirements text, never from what the submission did or from which answer seems more natural. Do not mark a test UNDERDETERMINED because it is hard.
- If the submission did not compile, so no test lines exist, judge the task as a whole under the name `*`: DETERMINED unless the compile failure stems from an ambiguity in the requested API.

## Output
Write `VERDICTS.tsv`: exactly one line per failing hidden test, tab-separated: `<task id>\t<test name as printed in hidden_output.txt, or *>\t<DETERMINED|UNDERDETERMINED>`. No header, no other lines.

{MODE}

Your final message must be a single line: `ADJUDICATED <number of DETERMINED lines> <number of UNDERDETERMINED lines>`.
