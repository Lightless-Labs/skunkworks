# Task: author one task family for a dev benchmark

Work only inside the current directory.

1. Read `SPEC.md` (the authoring spec and frozen rubric; follow it exactly) and `FAMILY.md` (your family brief).
2. **Your family id is `{FAMILY_ID}` and its rubric class is `{FAMILY_CLASS}`.** Use the id everywhere SPEC.md says `<family>`: the output directory is `out/{FAMILY_ID}/{FAMILY_ID}-l1/` … `out/{FAMILY_ID}/{FAMILY_ID}-l4/`, and each `task.json` has `"id": "{FAMILY_ID}-lN"` and `"family": "{FAMILY_ID}"`. Every task's `rubric.json` lists `{FAMILY_CLASS}` among its difficulty sources. Do not put the descriptive family name from FAMILY.md into ids, directory names or file names.
3. Verify with `./check.sh` and `./api_check.sh` exactly as SPEC.md rules 6 and 8 require, and write `out/{FAMILY_ID}/REPORT.md` as rule 7 requires.
4. Network access is not needed and must not be used. Do not read anything outside this directory.

Your final message must be a single line: `DONE {FAMILY_ID} <number of tasks whose reference passes public and hidden tests>`.
