---
title: "Codex --json: Failed Calls Need Their Own Transcript, and the Version Can't Come From the Transcript"
date: 2026-10-19
category: best-practices
module: crates/a2d-providers, crates/a2d-lift, nano/confirm3.py
problem_type: evaluation_integrity
component: provider_audit_trail
symptoms:
  - "Confirmation-2 replicate 2: 11 invocations had a prompt file but no transcript (replicate 1 had 0)"
  - "A rule 'missing transcript voids the tuple' would charge every provider timeout to the arm that makes the most calls"
  - "A per-transcript version check ('OpenAI Codex v0.162.0' banner) found nothing in --json transcripts"
  - "A killed `sh` left its `sleep` child holding the pipes; the reader join never returned"
root_cause: audit_trail_written_only_on_success
---

# Codex --json: Failed Calls Need Their Own Transcript, and the Version Can't Come From the Transcript

## Context
Confirmation-3 voids a `(task, arm, replicate)` tuple when any invocation has a tool event or a missing transcript. A void X tuple then counts as a fail and a void D/B tuple as a pass. The rule is sound, but two instrument facts would have made it charge infrastructure noise to the treatment:

1. **The meter wrote a transcript only for successful calls.** A provider error left a prompt file and an `ERROR:` response, and nothing else. All 34 failed calls in the dev ledgers (0.47%) were provider timeouts.
   - The harness arm makes about 5 calls per tuple, against D's 1.3, so timeouts land disproportionately on X.
   - In confirmation-2's replicate 2, the stalled host produced 11 such calls, 8 of them in X. Worst-case imputation would have cost X up to ~5.6 pp per replicate against a +10 pp bar.
2. **`codex exec --json` prints no version banner**, on stdout or stderr (probe, 1 Luna call). A per-transcript `OpenAI Codex v…` check would have voided *every* tuple once the provider moved to `--json`.

## What we did
- **Capture the failed call's partial transcript.** On timeout, the provider kills Codex and joins the pipe readers. The partial event stream and stderr go into a **thread-local slot**, and the meter takes it right after `invoke` on the same thread. That needs no change to the `Provider` trait or the `ProviderError` enum.
- **Kill the process group, not the child.** Codex is spawned with `process_group(0)` and killed with `/bin/kill -KILL -<pgid>`.
  - A plain `child.kill()` leaves helpers alive, and they hold the stdout/stderr pipes, so `read_to_end` never returns. The unit test reproduced this with `sh -c '…; sleep 30'`.
  - The join is still bounded (5 s, using `JoinHandle::is_finished`). A process that escaped the group means no transcript, which fails closed.
- **Count a failed call's events without the completion penalty.** The allowlist counter treats a missing `turn.completed` as an event, which is right for a call that claims success.
  - A killed call can never complete its turn, and its last line may be cut mid-object.
  - `codex_tool_calls_failed` therefore drops both penalties and keeps everything else: unknown types, non-allowlisted items and unparsable middle lines all still count.
- **Pin the version by bracketing the run, not per transcript.** Record the Codex binary's realpath and sha256 before the replicate and after `generation_complete`. A mismatch voids the replicate's tuples.
  - The scoring code accepts `--json` transcripts (detected by the events/stderr marker) without a banner. It keeps the banner check for legacy text transcripts.

## Gotchas
- **A live probe catches what unit tests don't.** One of three forced 12 s timeouts was killed before Codex emitted `thread.started` (cold start). That is still a valid transcript: an empty stream has zero events. The first version of the live test asserted an event and failed.
- **Test prompts embed libtest's `finished in 0.30s`.** A parity test comparing two runs' prompts flaked under load. Normalise durations, just as the test already masked temp paths, module names and thread ids.
- **Don't let a "missing artifact → void" rule meet a recorder that only writes on success.** Check the recorder's error paths before freezing such a rule.
- **Pre-spawn failures have no transcript at all** (2026-10-20). Codex never started, so there was nothing to capture. In the confirmation-3 audit, 4 such calls voided 4 tuples. They were 2 pairs of calls starting in the same microsecond: macOS clocks have microsecond resolution, and the cwd was named `<pid>-<nanos>` and created with `create_dir_all`, so the pair shared a cwd and a CODEX_HOME. Fix: a per-process sequence in the name, and `create_dir` (fail closed). A thread-barrier regression test reproduces the collision.

## Practice
- For every integrity rule that keys on an artifact, ask **when the artifact is not written**, and make that path either write it or be impossible.
- An audit trail that is complete only on success is biased: failures cluster where the most calls are made.
- When a structured output mode replaces a text one, re-check every check that parsed the text: banners, markers and line formats.
- Never name a per-call scratch directory by clock and pid alone, and never create it with a call that tolerates an existing directory. Under concurrency, uniqueness must come from a counter or a random suffix, and an existing directory must be an error.
- Classify "no information" voids by fixed labels before re-running them. A re-run that succeeds hides an instrument fault that will recur at random in the spend.
