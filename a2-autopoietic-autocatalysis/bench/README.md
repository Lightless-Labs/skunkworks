# Benchmark Harness

## Self-Correction Benchmark

`bench/self_correction.py` is the first loop-shaped A² benchmark. It creates an isolated git worktree, injects a deterministic fixture regression, commits that bug only in the isolated branch, and runs repeated `a2ctl run --apply` attempts with the same JSONL `task_id`.

Smoke the fixture without a model:

```bash
bench/self_correction.py --smoke-only --results /tmp/a2-self-correction-smoke.jsonl
```

Run real A² attempts with a cheap provider:

```bash
bench/self_correction.py --provider gemini --attempts 3 \
  --results bench/self-correction-results.jsonl
```

Run multiple independent trajectories in one command by adding `--runs`. Each run starts from a fresh isolated worktree with the fixture newly injected, gets a distinct `run_id`, and restarts attempt numbering at 1:

```bash
bench/self_correction.py --fixture compound-hidden \
  --provider opencode/minimax-coding-plan/MiniMax-M2.7 \
  --runs 3 \
  --attempts 3 \
  --results bench/self-correction-compound-results.jsonl
```

Run the harder compound fixture that usually exercises the loop instead of pass@1:

```bash
bench/self_correction.py --fixture compound-hidden \
  --provider opencode/minimax-coding-plan/MiniMax-M2.7 \
  --attempts 3 \
  --results bench/self-correction-compound-results.jsonl
```

Current loop-shaped fixtures:

- `fibonacci` — visible one-crate `a2_core` regression; usually too easy.
- `compound-core-same-crate-hidden` — visible and hidden regressions in `a2_core`.
- `compound-hidden` — visible `a2_core`, hidden `a2ctl` TODO scanner regression.
- `compound-membrane-hidden` — visible `a2_core`, hidden `a2_membrane` deny-overrides-allow regression.
- `compound-archive-hidden` — visible `a2_core`, hidden `a2_archive` lineage ordering regression.
- `compound-archive-same-crate-hidden` — visible and hidden regressions in `a2_archive` promotion journal ordering and legacy schema migration.
- `compound-archive-index-hidden` — visible `a2_archive` promotion journal ordering plus hidden schema index direction assertion.
- `compound-sensorium-same-crate-hidden` — visible and hidden regressions in `a2_sensorium`.
- `compound-raf-same-crate-hidden` — visible and hidden regressions in `a2_raf` graph edge-case behavior.
- `compound-eval-same-crate-hidden` — visible and hidden regressions in `a2_eval` seed scoring behavior.
- `compound-broker-same-crate-hidden` — visible and hidden regressions in `a2_broker` provider usage parsing.
- `compound-constitution-same-crate-hidden` — visible and hidden regressions in `a2_constitution` bootstrap profile behavior.
- `compound-workcell-same-crate-hidden` — visible and hidden regressions in `a2_workcell` catalyst response parsing and prompt context truncation.
- `compound-workcell-provider-hidden` — visible Pi JSONL usage parsing plus hidden candidate-worktree `PWD` propagation in `a2_workcell` provider execution.
- `compound-a2d-same-crate-hidden` — visible and hidden regressions in `a2d` stagnation detection and verifier-backstop promotion.

Smoke a specific fixture without a model:

```bash
bench/self_correction.py --fixture compound-raf-same-crate-hidden \
  --smoke-only \
  --results /tmp/a2-raf-fixture-smoke.jsonl
```

For reproducible archived evidence, add `--require-clean-source` from a clean checkout. The guard checks the project-scoped git status before creating benchmark worktrees or result files, so write generated demo artifacts outside the checked source tree (for example under `/tmp`) unless the destination is already ignored or pre-created outside the audited project path.

```bash
bench/self_correction.py --fixture fibonacci \
  --smoke-only \
  --require-clean-source \
  --results /tmp/a2-smoke-clean-source.jsonl
```

Each JSONL result includes:

- `task_id`, `run_id`, `attempt`, `category` (`--runs N` emits one distinct `run_id` per independent trajectory)
- `provider` / `model`
- `source_head`, `source_head_short`, `source_branch`, and `source_dirty` for auditing which source revision produced the benchmark record; use `--require-clean-source` when the run should fail rather than produce `source_dirty=true` evidence
- `max_tokens` and `timeout_secs`, recording the per-attempt budget used to produce newly generated rows
- `resolved`
- `prior_lineage_present`
- `lineage_records_before` / `lineage_records_after`
- `lineage_reconciled_by_core`
- `verifier_failure_evidence_present`, `promotion_evidence_present`, and nested `promotion` fields (`verifier_gated`, `evidence_present`, `lineage_reconciled_by_core`, `verify_returncode`) on newly generated rows. For structured rows, `--require-demo` treats the nested `promotion` object as authoritative and requires explicit `verifier_failure_evidence_present=true` on the failed first attempt plus `verifier_gated=true`, `evidence_present=true`, `lineage_reconciled_by_core=true`, and `verify_returncode=0` on the lineage-tied passing attempt; older archived rows without nested `promotion` remain scoreable through legacy stdout/stderr promotion markers
- `anti_repeat_retry_enabled` / `ablation`
- `touched_files`, `touched_file_count`, `diff_added_lines`, `diff_removed_lines`
- verification command, return code, duration, stdout, stderr

Score self-correction specifically:

```bash
bench/self_correction_score.py bench/self-correction-results.jsonl
```

Add per-run attempt trajectories when investigating retry shape or verifier failures:

```bash
bench/self_correction_score.py --trajectories bench/self-correction-results.jsonl
```

Machine-check that a log contains at least one complete reproducible demo trajectory and optionally refresh the machine-readable causal-chain evidence map:

```bash
bench/self_correction_score.py --require-demo --trajectories \
  --demo-evidence-json docs/benchmark-results/self-correction/a2-archive-same-crate-opencode-minimax-m3-20260615T165316Z.demo-evidence.json \
  docs/benchmark-results/self-correction/a2-archive-same-crate-opencode-minimax-m3-20260615T165316Z.jsonl
```

Or use the demo wrapper, which separates archived-proof verification from fresh provider regeneration:

```bash
# Fast, deterministic: re-score the durable archived demo artifact and refresh the evidence map.
bench/self_correction_demo.py verify-archive

# Slow/provider-backed: regenerate a fresh artifact, then run the same --require-demo gate.
RUN_ID=fresh-demo-$(date -u +%Y%m%dT%H%M%SZ)
bench/self_correction_demo.py fresh \
  --fixture compound-archive-same-crate-hidden \
  --provider opencode/minimax-coding-plan/MiniMax-M3 \
  --runs 3 \
  --attempts 3 \
  --run-id "$RUN_ID" \
  --results "docs/benchmark-results/self-correction/a2-${RUN_ID}.jsonl" \
  --confirm-provider-run
```

Fresh mode requires `--run-id` and, for the non-preflight/non-print execution path that can spend provider quota, `--confirm-provider-run`; it refuses to write to a non-empty results file or non-empty evidence JSON file, so the post-run `--require-demo` gate cannot pass because of older rows already present in the target JSONL or because an old proof was left at the target evidence path. Use a unique `RUN_ID`/`--results` path for auditable fresh regeneration. Unless `--evidence-json` is supplied, fresh mode writes the scorer proof next to the results as `<results-stem>.demo-evidence.json`, so a successful fresh regeneration has both JSONL rows and a durable causal-chain evidence map to archive. Clean-source readiness is checked before those fresh output files are created, and the harness records that pre-run source state in each row; if the outputs live under tracked `docs/benchmark-results/`, they will make the checkout dirty after the run and must be reviewed/committed deliberately rather than mistaken for pre-run dirtiness. After a successful fresh score, the wrapper first confirms the new evidence map points at the requested results path with a matching `artifact_sha256`, then runs `verify-evidence-contract --fresh-run-id "$RUN_ID" --max-tokens <budget> --timeout <seconds>` against that map, so stale/mismatched rows, substituted evidence maps, post-score JSONL mutations, and JSONL rows containing host-specific path markers are rejected before the artifact is treated as fresh loop proof. `fresh --preflight-only` checks empty output/evidence paths, provider CLI presence, local provider config where supported, and clean source before output creation unless `--allow-dirty-source`, then prints the harness/validation/scorer/fresh-provenance-contract commands without running the provider-backed benchmark. Add `--preflight-report-json <tmp-or-unique-path>` to write the same no-network readiness result as machine-readable JSON; the report path must be empty/nonexistent, distinct from the results/evidence paths, and should not be committed as loop proof. The report records that benchmark task payloads request `network_policy=Isolated` and that current restricted provider-backed runs fail closed until an audited sandbox/provider allowlist exists. It is a readiness check only: live auth, quota, model availability, and sandbox/provider-allowlist execution remain unverified until the fresh run executes. `fresh --print-only` is a lighter command preview that also prints the post-score provenance-contract command. That preview includes the underlying `bench/self_correction.py` harness command, but the wrapper-only `--confirm-provider-run` safety gate is not part of that printed harness line; use the wrapper `bench/self_correction_demo.py fresh ... --confirm-provider-run` command for real provider-backed regeneration instead of copy/pasting only the printed harness line. Neither preflight nor print-only creates verifier/failure/lineage artifacts or counts as loop evidence.

After any archived or fresh run writes a `.demo-evidence.json`, validate that JSON against the archived six-step contract before treating it as a rerunnable demo artifact:

```bash
bench/self_correction_demo.py verify-evidence-contract \
  --evidence-json "docs/benchmark-results/self-correction/a2-${RUN_ID}.demo-evidence.json" \
  --reference-evidence-json docs/benchmark-results/self-correction/a2-archive-same-crate-opencode-minimax-m3-20260615T165316Z.demo-evidence.json \
  --fresh-run-id "${RUN_ID}" \
  --max-tokens 100000 \
  --timeout 1800
```

Omit `--fresh-run-id` for deterministic archived contract verification. Include it for fresh artifacts: the referenced JSONL must then also pass run-id/prefix membership, provenance-field, budget, and clean-source checks, so stale/cached rows with another run ID fail before the artifact is archived as fresh loop evidence.

Audit that documented test-count claims still match the local Rust and Python test inventory:

```bash
bench/self_correction_demo.py verify-documented-counts
```

Run the updater only after intentionally adding or removing tests, then review the documentation diff it produces:

```bash
bench/self_correction_demo.py verify-documented-counts --update
```

This count audit shells out to `cargo test -- --list`; run it as an explicit operator check, not from `cargo test`, sentinel, or Python self-test paths.

This contract check is local artifact validation, not a provider run. It rejects `complete=false`/pass@1-only evidence and requires all six proof steps in order, retry context linked to archived verifier/failure evidence, advancing lineage, verifier-gated promotion evidence, and embedded row snapshots that match the referenced source JSONL artifact. Older raw JSONL artifacts preserve provider stdout/stderr and historical temporary workspace strings; the durable `.demo-evidence.json` map deliberately embeds only schema-bounded row snapshots and rejects host-specific path markers.

`--require-demo` exits non-zero unless at least one grouped run contains: a failed first attempt with verifier failure archived into lineage (`lineage_records_after > lineage_records_before`), a later verified passing attempt whose `lineage_records_before` reaches the failed row's `lineage_records_after`, core lineage reconciliation, and verifier-gated promotion/apply evidence. Passing output includes a deterministic `[proved]` checklist mapping each demo requirement to the JSONL artifact, run/task ID, verifier command/status, lineage counters, retry attempts, later pass, and verifier-gated promotion/apply evidence. `--demo-evidence-json` writes the same proof as JSON with one `causal_chain` entry per proved trajectory step, schema-bounded normalized `evidence_row` / `evidence_rows` snapshots for the rows used by each proof step, and the source JSONL `artifact_sha256`; it remains `complete=false` for pass@1-only or incomplete lineage logs. This is loop evidence only when the chain is failed attempt → archived verifier/failure evidence → retry context from that failed lineage row → later pass → lineage trajectory → verifier-gated promotion; pass@1-only logs should fail `--require-demo`. Archived-proof verification is deterministic evidence that the checked artifact contains the chain; fresh regeneration is provider-backed and may consume quota. The known 2026-06-15 archived demo predates explicit `max_tokens`/`timeout_secs` fields, though its captured `a2ctl run` command records `--max-tokens 100000 --timeout 1800`; newly generated rows record those budget values as structured fields.

This scorer reports `pass@1` separately from `self-corrected`. A first-attempt pass is useful model capability data, but it does not exercise prior-lineage self-correction; when no run retries, `self-corrected` renders as N/A instead of a failed recovery rate. A JSONL file can have more rows than independent trials because each retry attempt is one row; score and document runs by unique `(run_id, task_id)`, not raw line count. Benchmark success keys off `resolved` / verification status; `a2_returncode=0` only means the agent command exited cleanly.

Run the anti-repeat retry ablation by keeping the same fixture/provider/attempt budget and writing enabled/disabled cohorts to one log or to paired logs:

```bash
: > /tmp/a2-anti-repeat-ablation.jsonl
bench/self_correction.py --fixture compound-hidden \
  --provider opencode/minimax-coding-plan/MiniMax-M2.7 \
  --attempts 3 \
  --results /tmp/a2-anti-repeat-ablation.jsonl
bench/self_correction.py --fixture compound-hidden \
  --provider opencode/minimax-coding-plan/MiniMax-M2.7 \
  --attempts 3 \
  --disable-anti-repeat \
  --results /tmp/a2-anti-repeat-ablation.jsonl
bench/self_correction_score.py /tmp/a2-anti-repeat-ablation.jsonl
```

`--disable-anti-repeat` passes `--disable-anti-repeat-retry` to `a2ctl run`. Candidate-worktree verification commands, prior lineage, verifier-derived relevant files, and retry acceptance criteria remain enabled.

After each attempt, `a2ctl run --apply` reconciles the newest lineage row with the core post-apply rebuild result. The harness records whether that core reconciliation path ran, but does not patch lineage itself.

The benchmark removes its isolated worktree by default. Use `--keep-workspace` to inspect a run.

## BigCodeBench / Legacy Harness

The old `bench/tasks/*.toml` suite is still in-tree, but the BigCodeBench evaluation path lives in three small Python CLIs:

- `bench/bigcodebench_runner.py` emits BigCodeBench Hard tasks as JSONL.
- `bench/eval.py` sets up a task workspace, runs the verification command, and emits one JSON result object.
- `bench/score.py` aggregates a JSONL result log into pass-rate metrics.

## Task Format

`bench/eval.py` expects a JSON object on stdin with these required fields:

```json
{
  "problem_statement": "Prompt shown to A²",
  "setup_script": "shell script to prepare the task workspace",
  "test_command": "command that decides pass/fail",
  "repo_path": "/absolute/or/relative/workspace/path"
}
```

Extra fields like `task_id`, `category`, `run_id`, or `attempt` are preserved in the evaluator output so they can be scored later.

## Generate BigCodeBench Tasks

The generator defaults to:

- dataset: `bigcode/bigcodebench-hard`
- split: `v0.1.4`
- first `20` tasks

It uses the Hugging Face `datasets` package unless you pass `--dataset-path` with a local JSON or JSONL export.

```bash
python3 -m pip install datasets
python3 bench/bigcodebench_runner.py > bench/bigcodebench-hard-20.jsonl
```

Each line includes:

- `problem_statement` for `a2ctl run`
- `setup_script` for the evaluator
- `test_command`
- `repo_path`
- metadata such as `task_id`, `category`, and `difficulty`

## Run A² Against Generated Tasks

`a2ctl run` now accepts JSONL task input and will use `problem_statement` when present. For external benchmarks such as Senior SWE Bench (`https://senior-swe-bench.snorkel.ai/tasks`) or generated BigCodeBench/SWE-bench-style tasks, include `"no_external_solution_search": true` and `"network_policy": "Isolated"` (or pass `--network-policy isolated` for plain-text task streams). A² currently treats restricted network policies as a fail-closed launch gate: coding-agent runs are blocked unless/until an audited sandbox/provider allowlist can enforce the requested policy at the process boundary. Do not count Senior SWE Bench evidence from prompt-only/no-policy runs.

```bash
python3 bench/bigcodebench_runner.py \
  | cargo run -p a2ctl -- run --provider codex --network-policy isolated --apply
```

Local network-policy smokes (not benchmark evidence):

```bash
# Host primitive only: emits command/returncode/stdout/stderr for the denied TCP probe.
python3 bench/network_policy_smoke.py --self-test
python3 bench/network_policy_smoke.py --json

# Real a2ctl launch-gate path: proves restricted policy currently blocks provider launch
# with a nonzero exit instead of counting a no-candidate discard as success.
# Requires the selected provider binary on PATH; default provider is opencode.
python3 bench/network_policy_smoke.py --a2ctl-run-smoke --self-test
```

The generated prompts target per-task workspaces under `bench/workspaces/`.
The generated setup scripts only seed `solution.py` when it does not already exist, so you can run evaluation after `a2ctl run --apply` without clobbering the agent's output.

## Evaluate Results

The evaluator runs one task at a time. It enforces:

- 60 second timeout per command
- CPU, memory, file-size, and open-file resource limits on Unix

Example for a single task:

```bash
head -n 1 bench/bigcodebench-hard-20.jsonl | python3 bench/eval.py
```

To run the full loop and build a JSONL log:

```bash
: > bench/results.jsonl
while IFS= read -r task; do
  printf '%s\n' "$task" | cargo run -p a2ctl -- run --provider codex --apply
  printf '%s\n' "$task" | python3 bench/eval.py >> bench/results.jsonl
done < bench/bigcodebench-hard-20.jsonl
```

This keeps generation, solving, and evaluation loosely coupled:

- `bigcodebench_runner.py` decides what the task is
- `a2ctl run --apply` writes the candidate code into the repository workspace
- `eval.py` prepares any missing files and runs the benchmark test command

## Score a Log

`bench/score.py` reads the JSONL log and prints:

- overall pass rate
- `pass@1`
- `pass@3`
- category breakdown
- trend over time when multiple runs are present in the log

```bash
python3 bench/score.py bench/results.jsonl
```

If you want trend reporting across repeated runs, include `run_id` in the JSON lines before evaluation.
