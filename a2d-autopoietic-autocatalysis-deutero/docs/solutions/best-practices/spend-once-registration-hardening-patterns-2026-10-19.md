---
title: "Spend-Once Registration Hardening: Patterns From Three Adversarial Review Rounds"
date: 2026-10-19
category: best-practices
module: nano/confirm3.py, docs/experiments/analysis/c3_registration.py, tools/holdout
problem_type: evaluation_integrity
component: confirmation_runner
symptoms:
  - "Reviewer: --registered-at is free operator input; a later commit could change the decision rule between generation and scoring"
  - "Reviewer: killing the Python runner leaves the a2d child alive; a relaunch 'proves' failure by acquiring the lock"
  - "Reviewer: a hidden-test repair can drop or #[ignore] a DETERMINED test through the neighbouring segment"
  - "Reviewer: a failed canary probe on the judge lane passes isolation (no leak, 0 tool events)"
root_cause: operator_and_failure_paths_unbounded
---

# Spend-Once Registration Hardening

## Context
Confirmation-3's runner, registration generator and audit tooling went through three early review rounds (Codex Sol at high effort, plus a fresh Claude, copies only, web off) before any registration existed.
- Sol answered No in all three rounds, with 10, 9 and 7 blockers. Claude answered Yes-with-fixes in rounds 1 and 3.
- Each round moved deeper, from the decision rule to the runner, then the generator, then step-5 tooling. Almost every blocker had the same shape: **an operator choice, or a failure path, that the code did not bound.**

## Patterns that closed the blockers
- **Bind by provenance, not by argument.** The only valid `--registered-at` is *the one commit that added `registration.json`* (`git log --diff-filter=A`). Every later artifact (attempt records, manifests, the score) must carry that sha.
  - A free sha argument lets anyone re-register the rule between generation and scoring.
- **Derive what a human could steer.** The candidate is computed from committed, content-free evidence: exactly one promoted keep; the look run at its registration commit on that keep's snapshot; the gate committed exactly once; the look completed by T. Before T it is "unresolved", not a fallback.
  - The suite must equal the final active family list. A subset that is certified but unfinished is refused.
- **Lock the lifetime, not the process.** A `flock` belongs to the open file description. Pass the descriptor to the child (`pass_fds`), and the lock is held until the child and every descendant that inherited it exit. Verify this live: the parent closes, the lock stays held; the child exits, the lock is free.
  - Close the descriptor without `LOCK_UN`: unlocking acts on the description shared with the child.
- **Reserve before the call, reconcile after the crash.** Write the attempt or round record atomically (`O_EXCL`, fsync) before the first model call.
  - A reservation with no result becomes a *void* round, which spends the allowance, never a fresh call.
  - A probe group recorded as "started" without a "go" holds the UTC day.
- **Preflight through the real path.** Latency and canary probes run as a tiny `a2d lift run` through the real provider, so they get the exact flags, a ledger the daily cap counts, and transcripts that can be audited.
  - Every lane must show *audited evidence*: a successful call with a `--json` transcript. "No leak and no tool events" from a failed call is no evidence.
- **Re-classify, but never override upward.** A stdlib port of the controller's bootstrap (exact to 1e-12 on dev) lets voids be imputed into the CI too.
  - The controller's own classes must still agree. A controller INCONCLUSIVE never becomes a claim through the port.
- **Remove the capability instead of policing it.** The segment check meant to stop hidden-test repairs from touching DETERMINED tests failed three ways: deletion, a helper function, and an attribute leaking across segments. Making repairs requirements-only removed the whole class.
- **Positive controls per item type, in the event stream.** Inventory the exec item types from the binary. Trigger each one live on every lane at its registered effort, and count the *intended* item in the event part only.
  - Record the run (`controls.json`) and make registration require it.
- **Positive drain evidence.** "No live process" is not proof that relocation worked. Require the relocated `CODEX_HOME`s to hold the authoring sessions.

## Process
- Run reviewers on copies of the code, not just the README. Round 2 asked for the pipeline's implementation, because the README's claims cannot establish anything.
- Batch fixes and re-review once. The advisor sequenced quick fixes first, then the generator, pipeline, isolation and controls, with one combined re-review instead of a re-check per item.
- A reviewer's "blocker" can be your own documentation error. Sol's "38 briefs" was a LOG misreading of a gateway count. Check the code before changing a rule.
- **Watcher gotcha:** `pgrep -f "<script>"` inside a shell whose own command line contains `<script>` matches itself forever. Wait on the driver's recorded pid with `kill -0`.
