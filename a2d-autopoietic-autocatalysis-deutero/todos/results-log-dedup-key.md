# results.tsv: log_result's dedup key omits the proposer

**Found:** 2026-10-22 (phase B close-out). **Priority:** medium. **Blocked until:** confirmation-3's
decision.json (`nano/eval.py` is frozen at fb5110e7 until then).

`nano/eval.py:log_result` skips a row whose `(id, stage, decision)` is already logged, so that a
resumed experiment does not log the same decision twice. The key has no `proposer`. Phase B logs one row per
arm, both `PC-<r>` / `round`, and in all four rounds both arms reached `screen-fail`. Every
uninformed row was dropped silently, and results.tsv held the informed arm only. No error, no
count. `c2_phaseb_backfill.py` restored the four rows, and the informed rows were reproduced field for field
as a check. The experiment itself was unaffected: the informed brief filters out pc-uninformed
rows, and the endpoints come from scores.json.

Fix, after the freeze (TDD):
- Add `proposer` to the key (`r[2]`); a test logs two rows that differ only in proposer and expects both.
- Keep the resume case: the same row logged twice is still logged once.
- Consider returning or printing "skipped (duplicate)", so that a dropped row is visible to the
  caller instead of silent.

`nano/confirm3.py` does not call `log_result`, so this cannot affect confirmation-3's rows.
