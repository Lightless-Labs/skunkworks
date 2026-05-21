# Self-Correction Loop Recovery TODO

Created: 2026-05-10
Updated: 2026-05-21

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

Not working / not yet resolved:

- GLM at the current 1800s attempt timeout produced no patches across 7 observed attempts on 2026-05-21. Recalibrate budget/timeout before drawing model-capability conclusions.
- Broader cross-fixture validation after hidden candidate-worktree verifier wiring remains limited.

## Recovery sequence

Implemented in order:

1. `todos/structured-external-verification.md`
2. `todos/retry-task-contract-from-verification.md`
3. `todos/verifier-derived-relevant-files.md`
4. `todos/anti-repeat-retry-strategy.md`
5. `todos/worktree-task-verifier.md`

Remaining recovery work is validation/calibration rather than missing core plumbing.

## Benchmark gate

After each structural change, run `compound-hidden` N=3 with available non-Claude providers:

```bash
bench/self_correction.py --fixture compound-hidden \
  --provider opencode/minimax-coding-plan/MiniMax-M2.7 \
  --attempts 3 \
  --results /tmp/a2-compound-$(date -u +%Y%m%dT%H%M%SZ).jsonl
```

Then score:

```bash
bench/self_correction_score.py /tmp/a2-compound-*.jsonl
```

Do not claim broad success from a single run; use these runs as development feedback only. Report exact facts: attempts, prior lineage presence, touched files, and resolved/self-corrected counts.
