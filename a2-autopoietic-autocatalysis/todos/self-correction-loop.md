# Self-Correction Loop TODOs

Created: 2026-04-28

Current facts:

- `bench/self_correction.py --fixture fibonacci` is too easy: Minimax N=3 passed on attempt 1 every time.
- `bench/self_correction.py --fixture compound-hidden` exercises the loop: Minimax attempt 1 failed; attempts 2-3 had prior lineage visible; no self-correction occurred.
- Benchmark-only lineage reconciliation was added so later attempts see external verification failures. This should move into core `a2ctl run` lineage handling.

## Next actions

- [x] Make prior external verification failures prominent in `a2_workcell::runtime::render_prior_motif` / prompt rendering. Completed 2026-05-01; `[external verify: FAIL]` notes now render as structured multiline `external_verification` motifs.
- [x] Persist post-apply `verify_and_rebuild` outcomes in `a2ctl run` lineage instead of patching SQLite from the benchmark harness. Completed 2026-05-01; `a2ctl run --apply` now asks the Governor to reconcile the persisted lineage record, and `bench/self_correction.py` no longer writes lineage rows directly.
- [x] Add touched-file / diff-stat fields to `bench/self_correction.py` result JSONL. Completed 2026-05-01; records now include `touched_files`, `touched_file_count`, `diff_added_lines`, and `diff_removed_lines` parsed from the latest lineage patch diff.
- [ ] Re-run `compound-hidden` N≥3 with Minimax after the prompt/lineage fixes. Post-fix runs on 2026-05-03 and 2026-05-08: attempts 1-3 failed; attempts 2-3 had prior lineage; each attempt touched only `a2_core/src/lib.rs` (1 added, 1 removed), not the hidden `a2ctl` regression. After the 2026-05-08 runs, prior motifs gained explicit `failure_focus` extraction and prompt text now makes prior external verification authoritative.
- [ ] Run `compound-hidden` with Kimi and GLM after Minimax loop behavior is understood. Kimi first post-fix run on 2026-05-03 matched Minimax: attempts 1-3 failed, prior lineage present on attempts 2-3, only `a2_core/src/lib.rs` touched. GLM still pending.
- [ ] Add a second hard fixture after `compound-hidden` self-corrects at least once.
