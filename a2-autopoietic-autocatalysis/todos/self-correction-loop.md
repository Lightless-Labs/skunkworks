# Self-Correction Loop TODOs

Created: 2026-04-28
Updated: 2026-05-24

Current facts:

- `bench/self_correction.py --fixture fibonacci` is too easy: Minimax N=3 passed on attempt 1 every time.
- `bench/self_correction.py --fixture compound-hidden` exercises the loop: the visible task is the `a2_core` Fibonacci regression and the hidden verifier also checks the `a2ctl` scan-marker regression.
- Core `a2ctl run --apply` reconciles post-apply verification truth into lineage; the harness no longer patches SQLite directly.
- Prior motifs preserve external verification output, extract `failure_focus`, and tell models verifier failures are authoritative.
- Structured external verification, verifier-derived retry acceptance criteria, verifier-derived relevant files, anti-repeat retry motifs, and hidden candidate-worktree verifier execution are implemented.
- `compound-hidden` with hidden candidate-worktree verifier wiring self-corrected with Minimax N=3 and Kimi N=3 on 2026-05-21: both resolved 3/3, pass@1 0/3, loop exercised 3/3, self-corrected 3/3. Results: `/tmp/a2-compound-with-hidden-worktree-verifier-minimax.jsonl` and `/tmp/a2-compound-with-hidden-worktree-verifier-kimi.jsonl`.
- `compound-membrane-hidden` with hidden candidate-worktree verifier wiring self-corrected with Minimax N=3 and Kimi N=3 on 2026-05-21: both resolved 3/3, pass@1 0/3, loop exercised 3/3, self-corrected 3/3. Results: `/tmp/a2-compound-membrane-with-hidden-worktree-verifier-minimax.jsonl` and `/tmp/a2-compound-membrane-with-hidden-worktree-verifier-kimi.jsonl`.
- `compound-archive-hidden` with hidden candidate-worktree verifier wiring self-corrected with Minimax N=3 and Kimi N=3 on 2026-05-22: both resolved 3/3, pass@1 0/3, loop exercised 3/3, self-corrected 3/3. Results: `/tmp/a2-compound-archive-hidden-minimax.jsonl` and `/tmp/a2-compound-archive-hidden-kimi.jsonl`.
- OpenCode GLM at the 1800s attempt timeout produced no patches across 7 observed attempts on 2026-05-21; 2026-05-22 fibonacci calibration at 200k/3600s also timed out with tokens=0/no patch. Direct `opencode --print-logs` smoke exposed upstream ZAI 429 `Insufficient balance or no resource package`. After subscription restore, `pi/zai/glm-5.1` worked through Pi with the existing Pi `zai` API key.
- A² now supports `pi` / `pi/<model_id>` provider routing. Pi/ZAI GLM fibonacci calibration passed attempt 1 with token accounting in `/tmp/a2-pi-zai-fibonacci-json-usage.jsonl`. Pi/ZAI GLM scored resolved 3/3, pass@1 0/3, loop exercised 3/3, self-corrected 3/3 on the three original compound fixtures: `/tmp/a2-compound-hidden-pi-zai-glm.jsonl`, `/tmp/a2-compound-membrane-pi-zai-glm.jsonl`, and `/tmp/a2-compound-archive-pi-zai-glm.jsonl`.
- `compound-sensorium-same-crate-hidden` was added 2026-05-24 to move beyond visible-core-plus-hidden-second-crate fixtures. It injects visible high-risk priority and hidden title truncation regressions in `crates/a2_sensorium/src/ingest.rs`; smoke-only injection verified both failures. Pi/ZAI GLM and Minimax N=3 both scored resolved 3/3, pass@1 0/3, loop exercised 3/3, self-corrected 3/3. Kimi N=3 scored resolved 3/3, pass@1 1/3, loop exercised 2/3, self-corrected 2/3. Results: `/tmp/a2-sensorium-same-crate-pi-zai-glm.jsonl`, `/tmp/a2-sensorium-same-crate-minimax.jsonl`, `/tmp/a2-sensorium-same-crate-kimi.jsonl`.

See `todos/self-correction-loop-recovery.md` for the structural recovery sequence.

## Next actions

- [x] Make prior external verification failures prominent in `a2_workcell::runtime::render_prior_motif` / prompt rendering. Completed 2026-05-01; `[external verify: FAIL]` notes now render as structured multiline `external_verification` motifs.
- [x] Persist post-apply `verify_and_rebuild` outcomes in `a2ctl run` lineage instead of patching SQLite from the benchmark harness. Completed 2026-05-01; `a2ctl run --apply` now asks the Governor to reconcile the persisted lineage record, and `bench/self_correction.py` no longer writes lineage rows directly.
- [x] Add touched-file / diff-stat fields to `bench/self_correction.py` result JSONL. Completed 2026-05-01; records now include `touched_files`, `touched_file_count`, `diff_added_lines`, and `diff_removed_lines` parsed from the latest lineage patch diff.
- [x] Re-run `compound-hidden` N≥3 with Minimax after the prompt/lineage fixes. Completed 2026-05-21 after hidden candidate-worktree verifier wiring: resolved 3/3, pass@1 0/3, loop exercised 3/3, self-corrected 3/3.
- [x] Run `compound-hidden` with Kimi after Minimax loop behavior is understood. Completed 2026-05-21 after hidden candidate-worktree verifier wiring: resolved 3/3, pass@1 0/3, loop exercised 3/3, self-corrected 3/3.
- [x] Recheck GLM provider availability before rerunning `compound-hidden`; completed 2026-05-22 by adding/using `pi/zai/glm-5.1`. Pi/ZAI fibonacci passed attempt 1 and `compound-hidden` N=3 scored resolved 3/3, pass@1 0/3, loop exercised 3/3, self-corrected 3/3. Results: `/tmp/a2-pi-zai-fibonacci-json-usage.jsonl` and `/tmp/a2-compound-hidden-pi-zai-glm.jsonl`.
- [x] Run Pi/ZAI GLM N≥3 on `compound-membrane-hidden` and score the JSONL result. Completed 2026-05-24: resolved 3/3, pass@1 0/3, loop exercised 3/3, self-corrected 3/3. Result: `/tmp/a2-compound-membrane-pi-zai-glm.jsonl`.
- [x] Run Pi/ZAI GLM N≥3 on `compound-archive-hidden` and score the JSONL result. Completed 2026-05-24: resolved 3/3, pass@1 0/3, loop exercised 3/3, self-corrected 3/3. Result: `/tmp/a2-compound-archive-pi-zai-glm.jsonl`.
- [x] Add a broader loop-shaped fixture beyond visible-core-plus-hidden-second-crate regressions. Completed 2026-05-24 with `compound-sensorium-same-crate-hidden`; Pi/ZAI GLM and Minimax N=3 scored resolved/self-corrected 3/3; Kimi N=3 scored resolved 3/3 and self-corrected 2/3.
- [x] Run `compound-sensorium-same-crate-hidden` N≥3 with Minimax and Kimi. Completed 2026-05-24; results: `/tmp/a2-sensorium-same-crate-minimax.jsonl`, `/tmp/a2-sensorium-same-crate-kimi.jsonl`.
- [ ] Design anti-repeat ablation benchmark: candidate verifier enabled + anti-repeat enabled vs candidate verifier enabled + anti-repeat disabled.
- [x] Run `compound-archive-hidden` N≥3 with Minimax and Kimi after smoke-only injection success. Completed 2026-05-22; both scored resolved 3/3, pass@1 0/3, loop exercised 3/3, self-corrected 3/3.
- [x] Refresh stale `Cargo.lock` observed by sentinel during Pi/ZAI validation. Completed 2026-05-22 with `cargo generate-lockfile --offline`; sentinel then passed 6/6.
- [x] Add a second hard fixture after `compound-hidden` self-corrects at least once. Completed 2026-05-18 with `compound-membrane-hidden`; after hidden candidate-worktree verifier wiring, Minimax N=3 and Kimi N=3 on 2026-05-21 both scored resolved 3/3, pass@1 0/3, loop exercised 3/3, self-corrected 3/3.
