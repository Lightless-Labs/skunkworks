---
title: "Seatbelt: Path Denies Don't Stop Copied Brokers — Allowlist Mach Services, and Reap Jobs by Sandbox Membership"
date: 2026-10-18
category: best-practices
module: tools/holdout
problem_type: evaluation_integrity
component: authoring_sandbox
symptoms:
  - "A copy of /usr/bin/open in the job's workdir launched TextEdit outside the sandbox, although (deny process-exec (literal \"/usr/bin/open\")) held for the original"
  - "setsid/double-forked job descendants survived killpg and could mutate artifacts while the host read them"
  - "With (deny mach-lookup) and no allowlist, xcrun fell back to xcodebuild and a sandboxed compile took 8+ minutes"
  - "codex exec failed with 'Connection failed: error sending request' while curl in the same profile reached the API"
root_cause: exec_path_denies_and_process_groups_are_not_identity
---

# Seatbelt: Path Denies Don't Stop Copied Brokers; Allowlist Mach Services and Reap by Sandbox Membership

## Context
The confirmation-3 authoring pipeline (`tools/holdout/`) runs Codex jobs that must never reach the user's files, other processes or the developing agent. The outer `sandbox-exec` profile denied the paths of broker binaries (`open`, `launchctl`, `osascript`, …) and killed each job's process group at the end. A reviewer (Codex Sol) flagged both as insufficient, and a live probe confirmed the first one. A copied `open` in the workdir asked LaunchServices to launch TextEdit, and LaunchServices, which runs outside the sandbox, did so. `open -a Terminal x.command` would have been unsandboxed code execution.

## What we did
- **Mach services are an allowlist.** The profile has `(deny mach-lookup)` followed by `(allow mach-lookup (global-name …))` for 9 services. It also has `(deny network-outbound (remote ip "localhost:*"))`, which covers both v4 and v6.
  - The allowed services are logd, the notification center, opendirectoryd (libinfo and membership), `bsd.dirhelper`, trustd and its agent, configd, and SecurityServer.
  - LaunchServices (`lsd.*`, `coreservices.launchservicesd`), Spotlight (`mds`), the pasteboard, cfprefsd and the data-protection keychain stay closed, and copies of brokers fail.
- **The list is harvested, not guessed.** Run the real workload once under `(allow mach-lookup (with report))`, then extract the services from the log:
  ```
  /usr/bin/log show --start … --predicate 'subsystem == "com.apple.sandbox.reporting"'
  ```
  `(with report)` is accepted only on `allow`; on `deny` it is a profile syntax error. Harvest from a **cold** run (fresh workdir), not a warm one.
- **Bisect what breaks, using the cheapest client.** `curl` reached the API on the minimal list, but codex did not. Codex loads TLS roots through SecurityServer.
  - After adding it, a copied `security` can still list only the system keychain's metadata, which is world-readable anyway. The login keychain *file* is unreadable, so its items stay out of reach.
  - Disclose it rather than pretend.
- **Probes before every job, each with its own failure test:**
  - copied `open` (by name and by path), `launchctl`, `pbpaste` and `mdfind` must fail;
  - `curl` to a fresh localhost listener must give rc 7, and the listener must see no connection.

  An rc ≠ 0 alone is not enough: a listener that never accepts makes `curl` time out (rc 28) even when it connected. Verify the probes fail open on a weakened profile (the negative control).
- **Job identity is the sandbox, not the process group.**
  - Each profile alone may read its own empty tag file under the driver's records. Every other profile denies it, because it sits in the holdout dir.
  - The host calls `sandbox_check(pid, "file-read-data", SANDBOX_FILTER_PATH|NO_REPORT, tag)` on every pid from `proc_listallpids`. A pid is in the job if it is sandboxed, may read the tag, and may not read `tags/_none`.
  - A process can drop its session, group, environment and cwd, but not its sandbox.
  - Reap: SIGSTOP every member to a fixpoint (stopped processes cannot fork), re-check membership (pid reuse), SIGKILL, repeat until empty. Only then read artifacts. If any survive, raise.
  - The driver lock reaps every unretired tag on start, which recovers from a dead driver.

- **Close the launch window too.** Between `Popen` and the moment `sandbox-exec` applies the profile, the launcher is unsandboxed, so membership reaping cannot see it. Every job *and probe* therefore starts in its own session as `sh -c 'read go && exec sandbox-exec … </dev/null'` with stdin = a driver pipe.
  - The driver fsyncs `pid start_time` (libproc `PROC_PIDTBSDINFO`) before writing "go". If the driver dies first, the read gets EOF and the launcher exits.
  - Recovery kills recorded live groups, but only when the live leader's start time matches the record (otherwise the id was reused), and only then reaps by sandbox. Two review rounds were needed: the first fix registered jobs but not probes.

## Gotchas
- **`log` in zsh is a builtin.** `log show …` silently prints nothing, so use `/usr/bin/log`. Half an hour of "the sandbox logs nothing" was this.
- **ctypes on arm64 macOS:** `sandbox_check` is variadic. Without `argtypes` for the 3 fixed arguments, the path arrives in a register instead of on the stack, and every check returns -1.
- **`sandbox_check` with a path filter returns 1 for a nonexistent path,** even for an unsandboxed process. Probe existing files only.
- **`(deny mach-lookup)` without `bsd.dirhelper`** makes `confstr(DARWIN_USER_TEMP_DIR)` fail inside xcrun. xcrun then misses its cache and runs `xcodebuild`, which hangs on LaunchServices and FSEvents.
- **Time the unsandboxed baseline before bisecting a slowdown.** During this work a `cargo-sweep` from another session flooded `fseventsd` (2.4 GB, "stuck"). Every file-creating process sat in `U` state, and an *unsandboxed* hello-world compile took 3.5 min. That looked exactly like a profile regression.
  - Check `top -stats command,cpu,mem,state` for a stuck `fseventsd`.
  - Check `ps -axo stat,comm | awk '$1 ~ /^U/'` for processes in uninterruptible wait.
- **`subprocess.Popen(pass_fds=(r,))` keeps the fd's own number.** It does not move it to 3. A handshake written as `read go <&3` read from whatever held fd 3 (a listening socket in the broker probe), and every probe failed closed. Pass the handshake on stdin instead.
- **Harvest the allowlist from an authoring-shaped job, not only "reply OK".** A Sol job that wrote and tested Rust under the profile passed. Its transcript also exposed a command-scan false positive (`"$HOME/.rustup/…/rustc"`) that would have spent an authoring attempt.
- **Live probes can leave side effects.** The escaping `open` really launched TextEdit, and the weakened-profile control launched Calculator. Close them afterwards; use a harmless target (`-g -j`) in recurring probes.

## Practice
- **Assume any binary can be copied.** Exec path denies are defense in depth only. The boundary is the services and sockets a process can reach: Mach lookups, unix sockets, localhost TCP and Apple Events.
- **Process groups and sessions are not identity.** Use something the job cannot shed, here its sandbox.
- **Every probe needs a negative control** that makes it fail, or a passing probe proves nothing. `(deny signal (target others))` and the path-only broker deny both looked right and were inert.
