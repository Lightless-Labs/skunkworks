# Gym v3 tooling

Pipeline for the gym v3 dev benchmark (plan `docs/plans/2026-10-21-gym-v3.md`). Every driver
prints only gateway lines (counts and opaque ids) and refuses to launch while the
confirmation-3 isolation gate is closed.

## Launch gate

- **(a) window:** `runs/c2-confirm3/decision.json` exists, or both replicates have a non-aborted
  `generation_complete` (each in its first attempt or its `-relaunch1`; a replicate-1 relaunch may
  run after replicate 2) **and** confirmation-3's scoring has not started (no
  `runs/c2-confirm3/manifest-r*.json`). `launch()` re-checks it for every job.
- **(b) frozen:** `git diff --quiet fb5110e7 --` the confirmation-3 paths (`gymlib.GATE_PATHS`),
  with nothing untracked there.

## Runbook

- **Before `confirm3.py manifest`:** run `python3 -I tools/gym/gym_author.py --stop` and wait
  until `python3 -I tools/gym/gym_author.py --drained` prints `"ok": true`. Writing the manifests
  closes gate (a) for new jobs; the stop and drain make sure no gym job is still running (CPU
  load) when confirmation-3's timed hidden tests run. The gym resumes only after
  `decision.json`.

## Selection registry (repo `runs/c2-gym3/`, content-free)

1. `gym_split.py register` (seed derived from the families, never chosen; refused once any
   authoring status exists) → commit `split.json`.
2. `gym_split.py freeze --wave 1` before wave 1's pilot; after wave 1 is finalized,
   `gym_split.py freeze --wave 2 --count N` (N from wave 1's keep rate; 0 allowed). Membership is
   write-once; every slot ends kept, dropped with a reason, or pending.
3. `gym_pilot.py --wave W`: one initial pilot per wave; `--repilot` only for families pending as
   void or invalidated, at most once each. `gym_band.py band --wave W` recomputes the wave from
   the registry: the first complete run that decides a family on its current certified content
   is authoritative.
4. `gym_band.py finalize`: draws the regression stratum once (registered rounding) and checks the
   floors (45 retained dev families = 180 tasks, 25 surviving holdout families); a shortfall is
   `floors_unmet`, which the acceptance replicate and the export refuse.
5. `gym_pilot.py --acceptance`, then `gym_band.py accept --run runs/c2-gym3-accept/r1`, then
   `gym_export.py export`.

## Pilot sealing and Luna accounting

Every raw pilot artifact (a2d's out dir with prompts, submissions, transcripts, hidden outputs and
the full ledger; the suite copy; the harness snapshot; the manifest) stays under the gym root. The
repo receives only `runs/c2-gym3-pilot-w<W>/r<N>/ledger.jsonl` and `summary.json` (or
`runs/c2-gym3-accept/r1/`). The ledger there is an **accounting ledger written incrementally**:
while `a2d lift run` is alive, a thread reads the sealed ledger's complete lines every second and
appends each `invocation` record reduced to content-free fields (`kind`, `provider`, `ts_ms`,
`arm`, `task`, `replicate`, `index`, `ok`, `duration_ms`, `tool_calls`, token counts, `seq`,
`src`). So the daily-cap globs of `nano/eval.py` and `nano/confirm3.py` (`runs/c2-*/*/ledger.jsonl`,
`'"invocation"' in line and LUNA in line`) see in-flight Luna calls; a damaged invocation line is
published as a damaged Luna-side record; a crashed run is swept again by the next driver. Every
record and the summary are scanned for hidden test names before they are written. The pilot also
checks the cap first: today's Luna count plus 6 × tasks must be at most 3000.

## Tests

`python3 -I -m unittest discover -s tools/gym/tests` (about 20 minutes). Test mode is confined:
`GYM_TEST_ROOT` must hold the gym root and every overridden path, `GYM_CODEX` must be a
non-Mach-O fake (in `tools/gym/tests/` or the test root) and `GYM_AUTH` a fixture; otherwise
`gymlib` refuses at import. No test calls a model.
