# Self-Correction Loop Recovery TODO

Created: 2026-05-10
Updated: 2026-05-28

## Current status

Working:

- Prior lineage is wired into retry attempts.
- `a2ctl run --apply` reconciles post-apply verification truth into lineage.
- Prior motifs render external verification failures, stdout-first details, and `failure_focus`.
- WorktreeCatalyst prompt states prior `external_verification` failures are authoritative.
- Self-correction JSONL records touched files and diff line counts.
- Structured external verification persists typed verifier outcomes in lineage.
- Retry task contracts receive verifier-derived acceptance criteria.
- Retry context includes verifier-derived relevant files.
- Retry context includes `anti_repeat_retry` warnings when failed patch shape misses verifier-derived source paths.
- Candidate-worktree verifier commands run before promotion scoring and remain hidden from the initial prompt.
- `compound-hidden` self-corrected with hidden candidate-worktree verifier wiring for Minimax N=3 and Kimi N=3 on 2026-05-21: both resolved 3/3, pass@1 0/3, loop exercised 3/3, self-corrected 3/3.
- `compound-membrane-hidden` self-corrected with hidden candidate-worktree verifier wiring for Minimax N=3 and Kimi N=3 on 2026-05-21: both resolved 3/3, pass@1 0/3, loop exercised 3/3, self-corrected 3/3. Results: `/tmp/a2-compound-membrane-with-hidden-worktree-verifier-minimax.jsonl` and `/tmp/a2-compound-membrane-with-hidden-worktree-verifier-kimi.jsonl`.
- `compound-archive-hidden` self-corrected with hidden candidate-worktree verifier wiring for Minimax N=3 and Kimi N=3 on 2026-05-22: both resolved 3/3, pass@1 0/3, loop exercised 3/3, self-corrected 3/3. Results: `/tmp/a2-compound-archive-hidden-minimax.jsonl` and `/tmp/a2-compound-archive-hidden-kimi.jsonl`.
- Pi/ZAI GLM provider routing is implemented. 2026-05-22 `pi/zai/glm-5.1` fibonacci calibration passed attempt 1 with token accounting; `compound-hidden` N=3 scored resolved 3/3, pass@1 0/3, loop exercised 3/3, self-corrected 3/3. Results: `/tmp/a2-pi-zai-fibonacci-json-usage.jsonl` and `/tmp/a2-compound-hidden-pi-zai-glm.jsonl`.
- Pi/ZAI GLM on 2026-05-24 scored resolved 3/3, pass@1 0/3, loop exercised 3/3, self-corrected 3/3 on `compound-membrane-hidden` and `compound-archive-hidden`. Results: `/tmp/a2-compound-membrane-pi-zai-glm.jsonl` and `/tmp/a2-compound-archive-pi-zai-glm.jsonl`.
- `compound-sensorium-same-crate-hidden` was added 2026-05-24 as a same-crate multi-bug fixture in `a2_sensorium/src/ingest.rs`; smoke-only injection verified both failures. Pi/ZAI GLM and Minimax both scored resolved 3/3, pass@1 0/3, loop exercised 3/3, self-corrected 3/3. Kimi scored resolved 3/3, pass@1 1/3, loop exercised 2/3, self-corrected 2/3. Results: `/tmp/a2-sensorium-same-crate-pi-zai-glm.jsonl`, `/tmp/a2-sensorium-same-crate-minimax.jsonl`, `/tmp/a2-sensorium-same-crate-kimi.jsonl`.
- Anti-repeat ablation N=3 on `compound-hidden` with Minimax completed 2026-05-28. Enabled cohort: resolved 3/3, pass@1 0/3, loop exercised 3/3, self-corrected 3/3; resolved attempts were 3, 2, 2. Disabled cohort: resolved 3/3, pass@1 0/3, loop exercised 3/3, self-corrected 3/3; all resolved on attempt 2. Result: `/tmp/a2-anti-repeat-ablation-compound-hidden-minimax-20260528T122327Z.jsonl`.
- Anti-repeat ablation N=3 on `compound-sensorium-same-crate-hidden` with Minimax completed 2026-05-28. Enabled and disabled cohorts both scored resolved 3/3, pass@1 0/3, loop exercised 3/3, self-corrected 3/3; all runs resolved on attempt 2. Result: `/tmp/a2-anti-repeat-ablation-sensorium-minimax-20260528T221811Z.jsonl`.
- `compound-raf-same-crate-hidden` was added 2026-05-29 to add same-crate loop diversity in `a2_raf`. It injects visible single-node RAF connectivity and hidden empty-graph repair coverage regressions in `crates/a2_raf/src/graph.rs`; smoke-only injection verified both failures. Result: `/tmp/a2-raf-fixture-smoke.jsonl`.
- `compound-raf-same-crate-hidden` with Minimax on 2026-05-29 resolved 3/3 runs; pass@1 1/3; loop exercised 2/3; self-corrected 2/3. Result: `/tmp/a2-raf-same-crate-minimax-20260529T212431Z.jsonl`.
- Sentinel passed 6/6 after refreshing stale `Cargo.lock` with `cargo generate-lockfile --offline` during Pi/ZAI validation.

Not working / not yet resolved:

- `compound-raf-same-crate-hidden` has not yet been run N≥3 with Kimi or Pi/ZAI GLM.

## Recovery sequence

Implemented in order:

1. `todos/structured-external-verification.md`
2. `todos/retry-task-contract-from-verification.md`
3. `todos/verifier-derived-relevant-files.md`
4. `todos/anti-repeat-retry-strategy.md`
5. `todos/worktree-task-verifier.md`

Remaining recovery work is fixture expansion, provider validation for newer fixtures, and additional ablation coverage rather than missing core plumbing. As of 2026-05-24, Minimax, Kimi, and Pi/ZAI GLM each have N=3 self-correction results on the three original compound fixtures after hidden candidate-worktree verifier wiring. On `compound-sensorium-same-crate-hidden`, Pi/ZAI GLM and Minimax self-corrected 3/3; Kimi resolved 3/3 with self-correction 2/3 because one run passed on attempt 1. Two Minimax anti-repeat ablation cohorts completed 2026-05-28: `compound-hidden` and `compound-sensorium-same-crate-hidden` both had enabled and disabled cohorts resolve/self-correct 3/3. `compound-raf-same-crate-hidden` was smoke-verified on 2026-05-29; Minimax resolved 3/3 with pass@1 1/3 and self-corrected 2/3; Kimi and Pi/ZAI GLM still need N≥3 provider runs.

## Benchmark gate

After each structural change or new fixture, run the changed fixture or `compound-hidden` N=3 with available non-Claude providers:

```bash
bench/self_correction.py --fixture compound-hidden \
  --provider opencode/minimax-coding-plan/MiniMax-M2.7 \
  --attempts 3 \
  --results /tmp/a2-compound-$(date -u +%Y%m%dT%H%M%SZ).jsonl

bench/self_correction.py --fixture compound-membrane-hidden \
  --provider pi/zai/glm-5.1 \
  --attempts 3 \
  --timeout 1800 \
  --max-tokens 100000 \
  --results /tmp/a2-compound-membrane-pi-zai-glm.jsonl
```

Then score:

```bash
bench/self_correction_score.py /tmp/a2-compound-*.jsonl
```

Do not claim broad success from a single run; use these runs as development feedback only. Report exact facts: attempts, prior lineage presence, touched files, and resolved/self-corrected counts.
