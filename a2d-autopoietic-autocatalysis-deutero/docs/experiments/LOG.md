# A²D Experiments Log

Pre-registration and results for `docs/plans/2026-10-14-lift-first-roadmap.md`. An entry is written **before** a live run; its Result is filled in after. Never edit a pre-registration after the run starts — add a dated Deviation line instead. Keep entries short; artifacts carry the detail.

## Entry template

```
### E<NN> — <short name>  [phase P<n>]
- Registered: YYYY-MM-DD (commit <sha>; controller/oracle paths frozen at this sha)
- Hypothesis:
- Arms / configuration:
- Task set: <calibration | development | confirmation-1 | confirmation-2>, suite id + hash, n =, families =
- Model(s) (pinned version/settings):     k =      replicates =
- Budget cap: <invocations>, <tokens>, <wall-clock>
- Infrastructure exceptions (predefined):
- Result-class rule: SUPPORTED / RULED OUT / INCONCLUSIVE thresholds (default: roadmap § Metric)
- Result: <pending | class, lift with paired cluster-bootstrap 95% CI, exact McNemar p>
- Artifacts: runs/<date>-<id>/ (ledger: <path>)
- Deviations: <dated lines, never edits above>
- Decision:
```

## Entries

### E01 — tiny real arm-A run  [phase P0]
- Registered: 2026-10-14 (commit fd3a5b7; controller paths frozen at this sha)
- Hypothesis: none about lift. P0 exit check: the controller drives a live Codex lane end to end (invocation, code extraction, timeout path, hidden scoring, ledger). Secondary observation: Luna one-shot hidden pass on the legacy tasks, as a saturation hint for P1.
- Arms / configuration: A (one-shot), k = 1
- Task set: development, `tasks/dev` (suite hash 7782021a79ff08ace611cfa755d96302fc2dcc6366c27b1d368ff827074a924f), n = 3, families = 3 (legacy chess, rubiks, sudoku; hidden tests only)
- Model(s): `codex/gpt-6-luna`, reasoning effort low, provider timeout 300 s (default)     k = 1     replicates = 1
- Budget cap: 3 invocations, 1800 s wall-clock
- Infrastructure exceptions (predefined): none. A provider error or timeout counts as a task failure.
- Result-class rule: not applicable (single arm, no comparison). Report hidden pass rate per task.
- Result: pending
- Artifacts: runs/2026-10-14-e01-tiny-arm-a/ (ledger: runs/2026-10-14-e01-tiny-arm-a/ledger.jsonl)
- Deviations:
- Decision:
