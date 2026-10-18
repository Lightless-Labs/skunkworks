# Task: choose and brief 40 new task families for a blind benchmark set

You are choosing task FAMILIES for a confidential benchmark ("confirmation-3"). Nobody on the development side will read what you write here until the benchmark has been used. Work only inside the current directory.

## What a family is
Read `SPEC.md` in this directory: it is the authoring spec each family will later be written to (4 graded levels of a single-file, std-only Rust task with numbered requirement clauses, public tests and hidden tests). A family is one coherent domain (for example a parser, a simulator, a codec, a scheduler, a rules engine) rich enough for 4 levels with many precise, interacting, non-standard clauses.

## What to produce
For i = 01 … 40, write `families/c3-fNN/FAMILY.md` (NN = two digits) containing:
- line 1: `# <short-kebab-case-name>` (2–4 words, e.g. `# tariff-calculator`);
- then ONE paragraph (80–160 words) briefing an author: the domain, the core API shape, what grows from l1 to l4, and which custom (non-textbook) rules make it hard to recall from training data.

Also write `families/NAMES.tsv`: one line per family, `c3-fNN<TAB><short-kebab-case-name>`.

## Constraints (all mandatory)
1. **Disjoint from every existing family.** `INVENTORY.txt` lists every family name already used anywhere in this project (training, development, validation and earlier confirmation sets). Do not reuse any of them, and avoid near-siblings: no family whose core domain or core algorithm substantially overlaps one in the inventory (e.g. if `csv-quote` exists, no CSV/TSV/delimited-text parser; if `huffman-codec` exists, no other entropy coder; if `cron-next` exists, no other cron/schedule-expression evaluator). When in doubt, choose something else.
2. **Disjoint from each other.** No two of your 40 families may overlap in core domain or algorithm.
3. **Diverse.** Spread across parsing/formats, data structures, simulations/state machines, numeric/units/money rules, text processing, games/puzzles rules, scheduling/allocation, protocols/encodings, and business rules. At most 6 families per category.
4. **Feasible** as a single std-only Rust file (no network, no filesystem, no threads, no external crates, deterministic, tests < 1 s), reference solution ≤ ~600 lines at l4.
5. **Order matters:** families c3-f01 … c3-f36 are used first; c3-f37 … c3-f40 are spares used only to replace a family that fails a later check. Put your strongest 36 first.

Do not write any tasks, tests or code. When done, your final message must be a single line: `DONE <number of FAMILY.md files written>`.
