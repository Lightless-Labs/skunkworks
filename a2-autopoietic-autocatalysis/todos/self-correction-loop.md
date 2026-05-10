# Self-Correction Loop TODOs

Created: 2026-04-28

Current facts:

- `bench/self_correction.py --fixture fibonacci` is too easy: Minimax N=3 passed on attempt 1 every time.
- `bench/self_correction.py --fixture compound-hidden` exercises the loop: Minimax/Kimi attempts get prior lineage on retries, but observed runs still do not self-correct.
- Core `a2ctl run --apply` now reconciles post-apply verification truth into lineage; the harness no longer patches SQLite directly.
- Prior motifs now preserve external verification output, extract `failure_focus`, and tell models verifier failures are authoritative.
- Observed failing pattern remains: attempts touch only `a2_core/src/lib.rs` and do not fix the hidden `a2ctl` scan-marker regression.

See `todos/self-correction-loop-recovery.md` for the structural recovery sequence.

## Next actions

- [x] Make prior external verification failures prominent in `a2_workcell::runtime::render_prior_motif` / prompt rendering. Completed 2026-05-01; `[external verify: FAIL]` notes now render as structured multiline `external_verification` motifs.
- [x] Persist post-apply `verify_and_rebuild` outcomes in `a2ctl run` lineage instead of patching SQLite from the benchmark harness. Completed 2026-05-01; `a2ctl run --apply` now asks the Governor to reconcile the persisted lineage record, and `bench/self_correction.py` no longer writes lineage rows directly.
- [x] Add touched-file / diff-stat fields to `bench/self_correction.py` result JSONL. Completed 2026-05-01; records now include `touched_files`, `touched_file_count`, `diff_added_lines`, and `diff_removed_lines` parsed from the latest lineage patch diff.
- [ ] Re-run `compound-hidden` N≥3 with Minimax after the prompt/lineage fixes. Post-fix runs on 2026-05-03 and 2026-05-08: attempts 1-3 failed; attempts 2-3 had prior lineage; each attempt touched only `a2_core/src/lib.rs` (1 added, 1 removed), not the hidden `a2ctl` regression. 2026-05-08 reruns after stdout-first, `failure_focus`, and authoritative-verification prompt changes still scored self-corrected 0/1.
- [ ] Run `compound-hidden` with Kimi and GLM after Minimax loop behavior is understood. Kimi first post-fix run on 2026-05-03 matched Minimax: attempts 1-3 failed, prior lineage present on attempts 2-3, only `a2_core/src/lib.rs` touched. GLM still pending.
- [ ] Add a second hard fixture after `compound-hidden` self-corrects at least once.
