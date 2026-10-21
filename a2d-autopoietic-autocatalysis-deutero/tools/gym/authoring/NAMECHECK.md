# Task: independent overlap check of proposed benchmark families

Work only inside the current directory. Nobody on the development side will read your notes.

`INVENTORY.txt` lists every task-family name already used in this project. `families/<id>/FAMILY.md` are {TOTAL} proposed new families (name on line 1, then a brief).

For each proposed family decide:
- `CONFLICT` if its core domain or core algorithm substantially overlaps ANY inventory family (same kind of parser/codec/simulator/rules engine, even under a different name), or substantially overlaps another proposed family with a LOWER id;
- `OK` otherwise.

Write `VERDICTS.tsv` with exactly {TOTAL} lines: `<id><TAB>OK` or `<id><TAB>CONFLICT`. Be strict: when unsure, mark CONFLICT.

Your final message must be a single line: `DONE <number of CONFLICT verdicts>`.
