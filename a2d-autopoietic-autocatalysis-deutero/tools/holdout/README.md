# Confirmation-3 blind authoring pipeline

Plan: `docs/plans/2026-10-18-confirmation-3.md` (§0.4, §0.6, §0.10, §0.11). History: `docs/solutions/best-practices/blind-holdout-authoring-pipeline-2026-10-14.md`. The 10-14 scripts lived in a session scratchpad and were lost, so the pipeline was rebuilt here, in the repo, under tests. Codex Sol and a fresh Claude reviewed it on 10-18; their findings and dispositions are in plan §0.11.

## What the code enforces
- **Output gateway (`c3lib.emit`):**
  - Scripts print only JSON lines with allowlisted keys.
  - An `id` must be an opaque confirmation-3 id; other strings must be short code-defined tokens; anything else prints as `redacted`.
  - Exceptions print only their class name.
  - Model text, family names, briefs, requirements, tests and verdict reasons never reach the developing agent.
- **Outer sandbox** (`c3lib.profile`; every codex job and every probe runs under `sandbox-exec`):
  - writes are denied everywhere except the job's workdir, `/dev/null` and xcrun's cache file;
  - reads are denied under `/Users`, the Data-volume twin, `/Volumes`, the holdout dir, the repo and the temp roots. Only the workdir, the Rust toolchain, the codex binary dir and `auth.json` (read-only) are re-allowed;
  - signals and process info stay within the sandbox; `/dev/tty` and Apple Events are denied;
  - broker binaries are denied (`launchctl`, `open`, `osascript`, cron/at, `pbcopy`, `ssh`, `sudo`, …);
  - unix sockets under user dirs and temp roots are denied, which closes the ssh agent;
  - network stays open, because codex needs the API. This is disclosed.
  - Codex's own sandbox is off inside the profile, because macOS cannot nest seatbelt profiles.
- **Per-job `CODEX_HOME`:** a fresh `_codex_home/` in the workdir holding only an `auth.json` symlink, so no plugins, skills, memories, `AGENTS.md`, hooks or config.
- **Driver-owned records:** `authoring/confirmation-3/_records/` holds every job's profile, transcript, stderr and exit code, the attempt counters, `active.json`, `similarity.json`, the validation results, `excluded.json` and `pipeline.json`. Jobs cannot reach it.
- **Host I/O refuses symlinks** (`safe_read_*`, `safe_write_*`, `safe_copy`) and checks each path component.
- **Launch isolation (`c3lib.isolation_check`), run before every job:**
  - the workdir's ancestry is free of agent or codex context files; any under `$HOME` must be unreadable in the profile, which is probed;
  - the codex binary's sha256 prefix and version are pinned;
  - a positive control must be readable inside the profile;
  - denial probes must fail: the canary, the repo, the prior holdout sets, `~/.codex/config.toml`, ambient files, and a write outside the workdir.
- **Gates:**
  - `prereqs_ok()`: confirmation-2's `web-audit-r2.json` is committed at HEAD, and `tools/holdout` and the plan are unmodified relative to HEAD. It records the commit.
  - `DriverLock`: one driver at a time.
  - `STOP`: nothing new launches once it exists.
  - `drained()`: STOP set, no live driver, no live job process group. It fails closed.
- **Process groups:** each job is its own session, killed with `killpg` on timeout, exception or exit.

## Order (each step only after the previous one is complete)
1. `c3_select.py`: inventory of every prior family → 40 opaque briefs → an independent name check → `FAMILIES.txt` (write-once) and `active.json` (36 active plus spares).
2. `c3_author.py --parallel 3`: authoring, at most 2 attempts per family, done only when verified from the files.
3. `c3_assemble.py`: a fresh staging suite of complete families only, atomically swapped into `$A2D_HOLDOUT_DIR/confirmation-3/`.
4. `c3_similarity.py`: Jaccard ≥ 0.15 or containment ≥ 0.40 → re-author once → next spare → drop.
5. `c3_validate.py`, then `c3_fix.py` (at most 2 rounds per family, each bound to a validation run), re-assembling and re-validating between rounds. Exhausted tasks are excluded and their family replaced.
6. The independent-solve audit and adjudication on the re-frozen provider (§0.11; tooling to follow before step 5), then the hash.

All scripts are run as `python3 -I tools/holdout/<script>`. **Never read by hand:** anything under `$A2D_HOLDOUT_DIR/{authoring/confirmation-3,confirmation-3}`. **Never print process command lines** while these jobs run.

## Gotcha: the linker inside the profile
- `cc` goes through the `xcrun` shim, which caches toolchain lookups in `<user temp dir>/xcrun_db`. The dir is resolved via `confstr`, not `$TMPDIR`.
- Without the anchored allow rule for that file (the same rule as the oracle sandbox), every link took about 1 min and `check.sh` timed out.
- With the rule, the first link in a fresh workdir still takes about 1 min; later links take 0.2–0.5 s.
- `test_check_sh_compiles_under_sandbox` guards this.

## Tests
`python3 -I -m unittest discover -s tools/holdout/tests -v`. They run on fixtures with a fake codex (`tests/fake_codex.sh`), under the real seatbelt profile, with no model calls. They cover the gateway, isolation and probes, symlink refusal, the drain, the sandboxed toolchain, the full flow, the similarity re-author and spare path, strict validation parsing, and tool-event fail-closed.
