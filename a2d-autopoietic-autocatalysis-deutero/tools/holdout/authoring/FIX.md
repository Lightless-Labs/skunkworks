# Task: repair rejected tasks of one benchmark family

Work only inside the current directory. You wrote this family earlier (see `SPEC.md`, `FAMILY.md` and `out/`).

`FIX-VERDICTS.txt` lists validator verdict lines for tasks of this family that were rejected. Each line names a task id and the failed rule. Repair ONLY those tasks so that they satisfy SPEC.md (including rules 6 and 8: re-run `./check.sh` and `./api_check.sh`). Keep each task's intent and difficulty; do not weaken tests to make them pass, and do not remove requirement clauses unless a clause is itself the defect.

Network access is not needed and must not be used. Do not read anything outside this directory.

Your final message must be a single line: `FIXED <number of tasks repaired>`.
