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
  - broker binaries are denied (`launchctl`, `open`, `osascript`, cron/at, `pbcopy`, `ssh`, `sudo`, …). That alone does not hold: a **copied** `open` launched an app through LaunchServices on 10-18. So Mach services are an **allowlist** (`MACH_ALLOW`: logd, notifications, opendirectory, dirhelper, trustd, configd and SecurityServer, which codex needs for TLS roots). That list was harvested with `(allow mach-lookup (with report))` from a sandboxed compile and a live codex call. LaunchServices, Spotlight, the pasteboard, cfprefsd, launchd services and the data-protection keychain stay unreachable;
  - unix sockets under user dirs and temp roots are denied, which closes the ssh agent; TCP to `localhost` (v4 and v6) is denied, which closes local dev servers, browser debug ports and local MCP or HTTP brokers;
  - network to remote hosts stays open, because codex needs the API. This is disclosed.
  - Codex's own sandbox is off inside the profile, because macOS cannot nest seatbelt profiles.
- **Per-job `CODEX_HOME`:** a fresh `_codex_home/` in the workdir holding only an `auth.json` symlink, so no plugins, skills, memories, `AGENTS.md`, hooks or config.
- **Driver-owned records:** `authoring/confirmation-3/_records/` holds every job's profile, transcript, stderr and exit code, the attempt counters, `active.json`, `similarity.json`, the validation results, `excluded.json` and `pipeline.json`. Jobs cannot reach it.
- **Host I/O refuses symlinks.** Reads are descriptor-relative: each component is opened with `O_NOFOLLOW` against its parent's descriptor, and `tree_hashes` uses `os.fwalk`. Writes check each component and replace atomically.
- **Job quiescence (`c3lib.reap`):** a job's processes are identified by their **sandbox**, not by process group, session, environment or ancestry, which a job can shed. Each profile alone may read its empty tag file (`_records/tags/<record>.tag`). `sandbox_check(pid, "file-read-data", tag)` is allowed only for that job's processes, while `_records/tags/_none` is denied. After a job exits or times out, the driver stops every member to a fixpoint (stopped processes cannot fork), kills them, and repeats until none is left. Only then does it read any artifact. If anything survives, it raises. `DriverLock` reaps every unretired tag first (driver-death recovery), and `drained()` requires no live member of any tag. Probes are tagged and reaped too.
- **Launch handshake (`c3lib._handshake_popen`):** between `Popen` and sandbox entry, a launcher is unsandboxed, so tag reaping cannot see it.
  - Every job **and every probe** is started in its own session as `/bin/sh -c 'read go && exec sandbox-exec … </dev/null'`, reading from a driver pipe. The driver says "go" only after it has fsync'd the launcher's pid and start time (libproc) to `pids.txt`, under the launch lock. A job launch is also refused once STOP exists; a probe launch is not.
  - If the driver dies before that, the pipe hits EOF and the launcher exits without running anything.
  - On start, `DriverLock` first kills every recorded launcher group that is still alive, then reaps all tagged members, then starts a fresh `pids.txt`. A recorded id whose live leader has a different start time has been reused, and is never signalled.
- **Launch isolation (`c3lib.isolation_check`), run before every job:**
  - the workdir's ancestry is free of agent or codex context files; any under `$HOME` must be unreadable in the profile, which is probed;
  - the codex binary's sha256 prefix and version are pinned;
  - a positive control must be readable inside the profile;
  - denial probes must fail: the canary, the repo, the prior holdout sets, `~/.codex/config.toml`, ambient files, and a write outside the workdir;
  - broker probes must fail, each with its own expected-failure test: copies of `open`, `launchctl`, `pbpaste` and `mdfind` run from the workdir, and `curl` to a fresh localhost listener (which must see no connection).
- **Gates:**
  - `prereqs_ok()`: confirmation-2's `web-audit-r2.json` is committed at HEAD, and `tools/holdout` and the plan are unmodified relative to HEAD. It records the commit.
  - `DriverLock`: one driver at a time.
  - `STOP` and the **launch gate**: `request_stop()` (`c3_author.py --stop`) and every `Popen` of a job (`c3lib.launch`) hold the same lock (`_records/launch.lock`). Once STOP exists, no job of any kind starts, select and namecheck included. A launch refused at the gate releases its reserved attempt or round; for selection, it removes the unlaunched workdir.
  - `drained()`: STOP set, no live driver, no live job process group, and no live member of any job sandbox. It fails closed.
- **Process groups:** each job is its own session, killed with `killpg` on timeout, exception or exit, and then reaped by sandbox membership (above).
- **Inventory completeness:** traversal errors raise (`os.walk(onerror=...)`), so a partial inventory or prior manifest fails selection instead of silently omitting families.
- **Selection provenance:** `select.json` carries the selection generation; `--reset` retires it with the failed briefs and job records, so only the current generation's successful job can be accepted.
- **Terminal states:** a family whose 2 authoring attempts are used up without verified output is replaced at assembly (`assemble-exhausted`). A re-authored family is decided only by a similarity check of a suite that holds the re-authored version (`assembled.json`); before that it stays pending.

## Order (each step only after the previous one is complete)
1. `c3_select.py`: inventory of every prior family → 40 opaque briefs → an independent name check → `FAMILIES.txt` (write-once) and `active.json` (36 active plus spares).
2. `c3_author.py --parallel 3`: authoring, at most 2 attempts per family, done only when verified from the files.
3. `c3_assemble.py`: a fresh staging suite of complete families only, atomically swapped into `$A2D_HOLDOUT_DIR/confirmation-3/`.
4. `c3_similarity.py`: Jaccard ≥ 0.15 or containment ≥ 0.40 → re-author once → next spare → drop.
5. `c3_validate.py`, then `c3_fix.py` (at most 2 rounds per family, each bound to a validation run), re-assembling and re-validating between rounds. Exhausted tasks are excluded and their family replaced.
6. The independent-solve audit and adjudication on the re-frozen provider (§0.11; tooling to follow before step 5), then the hash.

All scripts are run as `python3 -I tools/holdout/<script>`. **The long drivers (`c3_select.py`, `c3_author.py`, `c3_fix.py`) are launched only through `tools/holdout/detached.sh <script> [args]`.** It uses its own session, `caffeinate` and no terminal. A Claude Code background shell can be reaped under memory pressure, and the next driver's recovery would then kill the in-flight jobs, spending their attempts. The detached driver's gateway output goes to `~/.a2d-c3-drivers/`, outside the holdout dir. **Never read by hand:** anything under `$A2D_HOLDOUT_DIR/{authoring/confirmation-3,confirmation-3}`. **Never print process command lines** while these jobs run.

## Gotcha: the linker inside the profile
- `cc` goes through the `xcrun` shim, which caches toolchain lookups in `<user temp dir>/xcrun_db`. The dir is resolved via `confstr`, not `$TMPDIR`.
- Without the anchored allow rule for that file (the same rule as the oracle sandbox), every link took about 1 min and `check.sh` timed out.
- With the rule, the first link in a fresh workdir still takes about 1 min; later links take 0.2–0.5 s.
- `test_check_sh_compiles_under_sandbox` guards this.

## Tests
`python3 -I -m unittest discover -s tools/holdout/tests -v`. They run on fixtures with a fake codex (`tests/fake_codex.sh`), under the real seatbelt profile, with no model calls. They cover the gateway, isolation and probes, broker probes, symlink refusal, the drain, the launch gate, inventory traversal errors, reset provenance, the reaping of a detached (setsid, double-forked, env-scrubbed) job process, exhausted-family replacement, the sandboxed toolchain, the full flow, the similarity re-author and spare path (including a stale suite), strict validation parsing, and tool-event fail-closed.
