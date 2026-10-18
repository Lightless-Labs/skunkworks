# Confirmation-3 blind authoring pipeline

Plan: `docs/plans/2026-10-18-confirmation-3.md` (§0.4, §0.6, §0.10). History: `docs/solutions/best-practices/blind-holdout-authoring-pipeline-2026-10-14.md`. The 10-14 scripts lived in a session scratchpad and were lost, so the pipeline was rebuilt here, in the repo, under tests.

## Rules the code enforces
- **Output gateway (`c3lib.emit`):** scripts print only JSON lines with allowlisted keys and opaque values (numbers, booleans, ids like `c3-f07`). Exceptions print only their class name.
  - Nothing a model wrote is ever printed to the developing agent: family names, briefs, requirements, tests, model text, verdict reasons.
- **Outer sandbox:** every codex job runs under `sandbox-exec` with a profile that denies reads and writes under `$HOME` and the temp roots. It re-allows only:
  - the job's workdir;
  - a dedicated `CODEX_HOME` inside the holdout dir, holding only an `auth.json` symlink;
  - the Rust toolchain;
  - the codex binary's dir.
  - Codex's own sandbox is off inside it, because macOS cannot nest seatbelt profiles. **Network stays open** (codex needs the API), which is disclosed. Reuse of public tasks is checked by `c3_similarity.py`.
- **Launch isolation (`c3lib.isolation_check`), run before every job; any failure means the job does not start:**
  - the workdir is inside the holdout dir, with no `.codex`, `AGENTS.md`, `.agents` or `skills` in its chain;
  - `CODEX_HOME` has no config, plugins, skills, memories, hooks or prompts;
  - the codex version is the pinned one;
  - a fresh canary under `$HOME` cannot be read from inside the profile.
- **Prompts are pointer files** (`INSTRUCTIONS.md`). The argv prompt is always "Read INSTRUCTIONS.md in the current directory and follow it."
- **Features:** every tool feature except shell and file edits is disabled. Each job's `--json` events are counted against an allowlist (agent_message, reasoning, command_execution, file_change, todo_list), and the count of other events is reported.
- **Drain:** every job pid goes to `pids.txt`. `c3_author.py --stop` stops new launches, and `--drained` checks the pids with `kill -0`, never `pgrep -f`.
- **Never read** `codex.jsonl`, `codex.err`, `REPORT.md`, `FAMILY.md`, `NAMES.tsv`, `validate-*.txt` or anything under `$A2D_HOLDOUT_DIR/{authoring/confirmation-3,confirmation-3}` by hand. Never print process command lines while these jobs run.

## Order (each step only after the previous one is complete)
1. `python3 -I tools/holdout/c3_select.py`: Sol writes 40 opaque briefs from the inventory of every existing family. An independent Sol session marks overlaps. `FAMILIES.txt` takes the first 36 OK ids (spares in order).
2. `python3 -I tools/holdout/c3_author.py --parallel 3`: one Sol (high) job per family, resumable.
3. `python3 -I tools/holdout/c3_assemble.py`: copies the contract files only into `$A2D_HOLDOUT_DIR/confirmation-3/`.
4. `python3 -I tools/holdout/c3_similarity.py`: word-5-gram Jaccard against every prior set and between families; a task conflicts at ≥ 0.15.
5. `python3 -I tools/holdout/c3_validate.py`, then `python3 -I tools/holdout/c3_fix.py` (at most 2 rounds), re-assembling after each round.
6. The independent-solve audit, adjudication and hash, on the re-frozen instrument (§0.6 step 5).

## Tests
`python3 -I -m unittest discover -s tools/holdout/tests -v` runs the full flow on fixtures with a fake codex (`tests/fake_codex.sh`), under the real seatbelt profile. It makes no model calls.
