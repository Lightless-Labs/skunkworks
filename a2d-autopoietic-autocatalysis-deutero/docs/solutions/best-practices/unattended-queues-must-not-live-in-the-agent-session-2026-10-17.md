---
title: "Unattended Multi-Day Queues Must Not Live as Agent-Session Background Shells"
date: 2026-10-17
category: best-practices
module: cycle-2 queue automation (queue-runner.sh, qrun.sh, ~/.a2d-queue)
problem_type: operational_reliability
component: unattended-scheduling
symptoms:
  - "Task notification: 'stopped because the system is running low on memory' for the runner, its caffeinate and the watcher"
  - "Runner output ends with [killed] while waiting for the next day's start time"
root_cause: the_agent_harness_reaps_idle_background_shells_under_memory_pressure
---

# Unattended Multi-Day Queues Must Not Live as Agent-Session Background Shells

**Evidence:** A²D LOG, "Queue runner stopped by the Claude Code memory reaper" (2026-10-17). Related: `deprecated-cli-flag-silently-reenables-a-tool-2026-10-17.md` (the same queue's review).

## What happened
- **The setup:** a 5-day queue (confirmation-2 replicates, then dev experiments) ran as a Claude Code background shell, with `caffeinate -w <pid>` and a milestone watcher as two more background shells.
- **The stop:** with the machine low on memory, Claude Code stopped all three while the session was idle. The runner was sleeping until 00:05 UTC, so nothing was half-run.
  - The harness's notice says a stopped job restarts only when the owner asks. The agent cannot simply relaunch it.
- **Near miss:** the same stop during a confirmation replicate would have written its attempt record. The registered policy allows one relaunch, so a second stop would have spent the 144-task confirmation set with no claim.
- **Second, latent bug found on the way:** the replicate-2 line waited for a fixed date. After any slip, replicate 1 and 2 would land on the same UTC day, the controller would answer "deferred", and the runner would walk past replicate 2 forever.

## Lessons
- **Session-bound background jobs share the session's fate.** Use them for minutes-long work the session watches, not for unattended multi-day schedules.
  - Launch long queues detached (`nohup caffeinate -i … &!`, a pid file, a second-copy guard), from the owner's terminal or on the owner's ask. Here: `~/.a2d-queue/start.sh`.
- **Keep the tooling of a long run on a durable path.** The session scratchpad sits under `/private/tmp`, which the OS may clean. Logs, queue file and scripts moved to `~/.a2d-queue/`.
- **Weigh what an interruption costs at each point of the queue.** A stop during a sleep costs nothing, while a stop during a spend-once step can spend the step. Put spend-once steps where interruptions are least likely, and make their recovery path explicit.
- **Dates in a queue are lower bounds, so anything that depends on two dates being distinct must re-check that at run time.** Turn "deferred" into "retry at the next window, bounded", not into "move on".
- **Watchers are optional; the runner is not.** Wrap the runner itself in `caffeinate -i`, so keeping the machine awake does not need a second process that can be stopped on its own.
