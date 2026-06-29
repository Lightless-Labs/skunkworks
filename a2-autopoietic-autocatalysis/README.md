# A² — Autopoietic Autocatalysis

A² is an autonomous software-factory prototype that modifies its own source code. It runs AI model CLIs in isolated git worktrees, verifies candidate patches, scores them, records lineage, and can optionally apply promoted patches back to the germline workspace.

For current operational state, read [`docs/HANDOFF.md`](docs/HANDOFF.md) first.

## Quick Verification

```bash
cargo test
cargo run -p a2ctl -- sentinel --workspace .
```

Expected state as of the latest handoff: Rust tests pass and the sentinel suite reports `6/6 PASS`.

## Self-Correction Benchmark Score Output

The self-correction benchmark has two tools:

- [`bench/self_correction.py`](bench/self_correction.py) runs isolated benchmark attempts and appends JSON records to a JSONL log.
- [`bench/self_correction_score.py`](bench/self_correction_score.py) reads that JSONL log and prints the human-readable score table.

So output like this is **scorer output**, not `cargo test` output and not direct model/provider output:

```text
Self-Correction Benchmark
  resolved             100.0% (6/6)
  pass@1               0.0% (0/6)
  loop exercised       100.0% (6/6)
  self-corrected       100.0% (6/6)

Ablation cohorts
  anti-repeat disabled
    resolved             100.0% (3/3)
    pass@1               0.0% (0/3)
    loop exercised       100.0% (3/3)
    self-corrected       100.0% (3/3)
  anti-repeat enabled
    resolved             100.0% (3/3)
    pass@1               0.0% (0/3)
    loop exercised       100.0% (3/3)
    self-corrected       100.0% (3/3)
```

Metric meanings and why they matter:

| Metric | What it means | Why it matters |
|--------|---------------|----------------|
| `resolved` | Independent benchmark runs that eventually passed all fixture verification within the allowed attempts. | This is the broad success rate: did A² get to a correct final workspace at all? It does not distinguish first-pass model skill from loop-driven recovery. |
| `pass@1` | Runs that passed on the first attempt. | This mostly measures raw provider/model capability on the fixture. A high `pass@1` can be good, but it means the A² correction loop was not tested much. |
| `loop exercised` | Runs where attempt 1 failed and at least one retry happened. | A² is supposed to use verification failures, lineage, retry context, and policy to recover. This number tells whether the benchmark actually entered that loop instead of being solved immediately. |
| `self-corrected` | Runs where attempt 1 failed and a later attempt passed with prior lineage/retry context available. | This is the key loop-shaped signal: the system failed, retained evidence about the failure, retried, and then succeeded. It is stronger evidence for A² than `resolved` alone. |
| `Ablation cohorts` | Optional split by fields such as `anti_repeat_retry_enabled`, when enabled/disabled runs are written to the same JSONL log. | Ablations test whether a subsystem contributes measurable value. If enabled and disabled cohorts score the same, that specific fixture/provider pair did not show a benefit from the ablated feature. |

The denominator counts independent benchmark `run_id`s, not individual retry attempts. A single run may contain attempt 1, attempt 2, attempt 3, etc.; the scorer groups those attempts by `run_id` before computing these percentages.

### Reading the example above

The example table says:

- There were 6 independent benchmark runs total.
- All 6 eventually resolved.
- None resolved on attempt 1, so the benchmark measured retry behavior rather than pure first-pass solving.
- All 6 self-corrected after seeing prior failure context.
- In the anti-repeat ablation split, both enabled and disabled cohorts resolved/self-corrected 3/3.

That last point is important: for that fixture/provider pair, the run did **not** show a measurable benefit from the anti-repeat retry motif. It also does **not** prove anti-repeat is useless. Other retry machinery remained enabled — prior lineage, verifier-derived relevant files, retry acceptance criteria, and candidate-worktree verifiers — and different fixtures/providers may separate the cohorts.

See [`bench/README.md`](bench/README.md) for commands and JSONL field details.

## Common Commands

```bash
# Observational benchmark / self-correction harness
bench/self_correction.py --fixture compound-hidden \
  --provider opencode/minimax-coding-plan/MiniMax-M2.7 \
  --attempts 3 \
  --results /tmp/a2-self-correction.jsonl
bench/self_correction_score.py /tmp/a2-self-correction.jsonl
bench/self_correction_score.py --require-demo --trajectories \
  docs/benchmark-results/self-correction/a2-archive-same-crate-opencode-minimax-m3-20260615T165316Z.jsonl

# Autopilot dry-run discovery
cargo run -p a2ctl -- autopilot --dry-run

# Resident autopilot wrapper, bounded smoke mode
cargo run -p a2ctl -- autopilot-resident --max-runs 1 --interval-secs 0 --dry-run
```
