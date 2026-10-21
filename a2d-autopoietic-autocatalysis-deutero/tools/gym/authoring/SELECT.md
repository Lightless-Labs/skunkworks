# Task: choose and brief gym families for a dev benchmark

You are choosing task FAMILIES for a development benchmark ("gym v3"). Nobody on the development side will read what you write here. Work only inside the current directory.

## What a family is
Read `SPEC.md` in this directory: it is the authoring spec and frozen rubric each family will later be written to (4 graded levels of a single-file, std-only Rust task with numbered requirement clauses, public tests and hidden tests). A family is one coherent domain (for example a parser, a simulator, a codec, a scheduler, a rules engine) rich enough for 4 levels.

## Rubric classes and quotas (fixed; do not change them)
Each family has one rubric class, its main difficulty source from SPEC.md: `depth`, `state`, `scale`, `surface` or `edges`. Write exactly {PER_CLASS} families of each class, {TOTAL} in all, with ids g3-f001 … in order. Within each class, put your strongest families first: the first ones that pass the overlap check are kept, the rest are spares.

## What to produce
For each id, write `families/<id>/FAMILY.md` containing:
- line 1: `# <short-kebab-case-name>` (2–4 words);
- then ONE paragraph (80–160 words) briefing an author: the domain, the core API shape, what grows from l1 to l4, and how the family's rubric class makes it hard while every tested behaviour stays written in the requirements.

Also write:
- `families/NAMES.tsv`: one line per family, `<id><TAB><short-kebab-case-name>`;
- `families/CLASSES.tsv`: one line per family, `<id><TAB><class>` with class one of the five above.

## Constraints (all mandatory)
1. **Disjoint from every existing family.** `INVENTORY.txt` lists every family name already used in this project. Do not reuse any of them, and avoid near-siblings (same core domain or core algorithm under another name). When in doubt, choose something else.
2. **Disjoint from each other.**
3. **Feasible** as a single std-only Rust file (no network, no filesystem, no threads, no external crates, deterministic), reference solution ≤ ~600 lines at l4.

Do not write any tasks, tests or code. When done, your final message must be a single line: `DONE <number of FAMILY.md files written>`.
