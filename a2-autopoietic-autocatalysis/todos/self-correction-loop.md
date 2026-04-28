# Self-Correction Loop TODOs

Created: 2026-04-28

Current facts:

- `bench/self_correction.py --fixture fibonacci` is too easy: Minimax N=3 passed on attempt 1 every time.
- `bench/self_correction.py --fixture compound-hidden` exercises the loop: Minimax attempt 1 failed; attempts 2-3 had prior lineage visible; no self-correction occurred.
- Benchmark-only lineage reconciliation was added so later attempts see external verification failures. This should move into core `a2ctl run` lineage handling.

## Next actions

- [ ] Make prior external verification failures prominent in `a2_workcell::runtime::render_prior_motif` / prompt rendering.
- [ ] Persist post-apply `verify_and_rebuild` outcomes in `a2ctl run` lineage instead of patching SQLite from the benchmark harness.
- [ ] Add touched-file / diff-stat fields to `bench/self_correction.py` result JSONL.
- [ ] Re-run `compound-hidden` N≥3 with Minimax after the prompt/lineage fixes.
- [ ] Run `compound-hidden` with Kimi and GLM after Minimax loop behavior is understood.
- [ ] Add a second hard fixture after `compound-hidden` self-corrects at least once.
