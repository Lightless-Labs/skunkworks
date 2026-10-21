# Gym v3 tooling

Pipeline for the gym v3 dev benchmark (plan `docs/plans/2026-10-21-gym-v3.md`). Every driver
prints only gateway lines (counts and opaque ids) and refuses to launch while the
confirmation-3 isolation gate is closed.

## Launch gate

- **(a) window:** `runs/c2-confirm3/decision.json` exists, or replicate 2 has a non-aborted
  `generation_complete` (first attempt or `-relaunch1`) **and** confirmation-3's scoring has not
  started (no `runs/c2-confirm3/manifest-r*.json`). `launch()` re-checks it for every job.
- **(b) frozen:** `git diff --quiet fb5110e7 --` the confirmation-3 paths (`gymlib.GATE_PATHS`),
  with nothing untracked there.

## Runbook

- **Before `confirm3.py manifest`:** run `python3 -I tools/gym/gym_author.py --stop` and wait
  until `python3 -I tools/gym/gym_author.py --drained` prints `"ok": true`. Writing the manifests
  closes gate (a) for new jobs; the stop and drain make sure no gym job is still running (CPU
  load) when confirmation-3's timed hidden tests run. The gym resumes only after
  `decision.json`.

## Tests

`python3 -I -m unittest discover -s tools/gym/tests` (about 20 minutes). Test mode is confined:
`GYM_TEST_ROOT` must hold the gym root and every overridden path, `GYM_CODEX` must be a
non-Mach-O fake (in `tools/gym/tests/` or the test root) and `GYM_AUTH` a fixture; otherwise
`gymlib` refuses at import. No test calls a model.
