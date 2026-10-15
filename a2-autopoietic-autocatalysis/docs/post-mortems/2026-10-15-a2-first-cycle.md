# Post-mortem: A²'s first cycle (2026-04-01 → 2026-10-15)

**Date:** 2026-10-15
**Covers:**
- the original build, April–July 2026 (426 commits);
- the three-month idle period;
- the E1 value test, 2026-10-14/15 (56 commits).

**Sources:**
- `reviews/2026-10-14-direction-audit.md`;
- `docs/benchmark-results/external-value/e1/decision.md`;
- `docs/plans/2026-10-14-value-test-first.md` and its addenda;
- `docs/blockers/`.

**Not a verdict.** A² exists to be an agent that iterates on coding-agent harnesses and workflows, in the spirit of autoresearch. This post-mortem asks why the first cycle produced almost no measured learning, and what the next loop must do differently. It does not ask whether A² should exist.

## 1. Summary

For six months, A² changed itself without ever measuring whether a change helped. It had no fitness function on external work and no baseline arm. It could also edit its own evaluator, and on day 1 it did, loosening its acceptance rule. Every "self-improvement" round was therefore unfalsifiable. When the path to a real experiment was blocked, effort went into validating artifacts and the documentation about them.

The first real fitness measurement, E1, ran two days after the direction audit. It showed:
- retrying with visible-test feedback roughly doubles a raw model's resolve rate;
- A²'s lineage-bearing loop adds nothing measurable over a plain stateless retry. The variant is discarded.

The durable output of the cycle is the apparatus that took the measurement. That apparatus is the missing piece every earlier "improvement" lacked.

## 2. Timeline

| When | What happened | What it should have triggered |
|---|---|---|
| Apr 1–2 | Eleven crates were scaffolded and "Stage 0" was claimed on public cargo checks. A² relaxed its own acceptance rule (`74bf6c1`) and "promoted" that change. | The evaluator must be outside the system's write scope. |
| Apr 2–4 | 61 "autonomous round" commits, about 45 lines each. The log says "14 overnight rounds with zero capability improvement". | No fitness signal, so rounds are noise. Stop and build one. |
| Apr 5–6 | Raw vs A² scored 5/5 vs 5/5: "baseline proves bench tests the wrong thing". The response was to raise budgets. | Pivot to an external value question with a control arm. |
| Apr 24 – Jun 30 | A self-correction harness built on A²'s own code with injected bugs. The prompt named one failing test and the verifier checked two, so attempt 1 was built to fail. Fixtures were hardened whenever a model passed first time. | That measures the fixture, not the loop. It needs a stateless-retry arm. |
| Jun 29 – Jul 8 | The demo evidence contract and sandbox attestation: 123 commits of evidence about evidence, including a 10k-line validator for one archived JSONL file. | Two attempts without movement means change the design, not add another guard. |
| Jul 10–11 | An audit, a 40-item remediation roadmap, then idle. The roadmap put the value test last. | Order work by real dependency, not by perceived safety. |
| Oct 14 | The direction audit froze the roadmap. E1 harness built: native grading, read boundary, egress proxy, three arms, pre-registration. | — |
| Oct 15 | E1 complete: raw 5/36, stateless retry 10/36, a2-full 12/36. NO GO for the lineage loop. | Discard the variant; keep the apparatus. |

## 3. Root causes

### 3.1 No fitness function on external work

This is the root failure, and the others follow from it. Self-modification without a held-out external score can only optimise things the system itself defines: test counts, green checks, evidence completeness. The DESIGN.md rule was right: improvement only counts on externally useful work. Nothing enforced it, because nothing measured it.

### 3.2 The evaluator was part of the genome

A² could write its own acceptance rule, and its second self-modification loosened it. That is the Goodhart / Fontana Level-0 failure DESIGN.md names as the primary threat. It happened on day 1 and stayed live for six months. The only reliable defence is structural: the system under evolution cannot write to the grader, the verifier, the task pool or the boundary.

### 3.3 No control arm, ever

Thirty-five self-correction JSONL files contain no stateless-retry baseline. Every attribution to lineage, ContextPack or anti-repeat was unfalsifiable. E1 added that arm and the attribution disappeared at once: most of the gain belongs to plain retry with feedback.

### 3.4 Measuring on its own source, with validity holes

The fixtures were bugs injected into A²'s own crates. The injected regression was committed into history the model could read with `git log`. The prompts were built so that attempt 1 would fail. None of that is held-out work.

### 3.5 A blocked path plus a no-pausing rule produced validator recursion

When the next real step was blocked (no sandbox, provider quota, paid-run confirmation), agents told never to pause did the only always-green work available: hardening validators, then validators of validators. The rule to change the design after two failed attempts existed, but nothing enforced it.

### 3.6 Safety gates ordered before the value test

The roadmap made a candidate-only experiment wait on germline heredity, constitution integration and Bazel parity. None of these is a dependency of an experiment that admits nothing. The cheapest falsifier ran last, and in practice never.

## 4. What went right

- **The audit-to-decision path took two days.** Freezing the roadmap and pre-registering a single falsifiable experiment worked.
- **Blockers became design changes, not waits:**
  - No container runtime on this host: grade natively in history-free checkouts.
  - No funded cheap provider: Codex `gpt-6-luna` in an isolated `CODEX_HOME`.
  - The Seatbelt hostname filter was invalid: a loopback CONNECT-allowlist proxy.
- **Calibration caught a ceiling before any sample spend.** With the official instructions, raw solved 5/5 at every reasoning effort. Switching to issue-only instructions under a pre-stated rule brought raw down to the target band.
- **Pre-registration held.** The protocol and analysis were committed before outcomes, and every later change was a dated addendum.
- **The apparatus works.** It produced 108 graded rows with 0 infrastructure exclusions and a decision derived mechanically from the committed rule.

## 5. What went wrong in E1 (my own execution)

- **The leak guard stopped the run twice, both times on false positives.**
  - First, it treated the private directory's *path* as a secret, but pytest tracebacks name the venv there.
  - Second, it matched hidden test ids as raw substrings, but some are prefixes of visible ids.

  I wrote the guard without testing it against real verifier output, though calibration runs had produced exactly that output. The run cost was three restarts, plus a small re-draw bias in five stateless-retry arms, which is disclosed in the record.
- **A watcher that could never fire.** A `pgrep -f` pattern matched its own shell's command line.
- **Wall-clock time was misread as a stall.** The VM had been paused, so 182 minutes on the session clock were about 2 minutes on the host.
- **A framing error after the result.** I treated the audit's re-decision table ("E1 NO GO → archive") as a project verdict. In the owner's frame, E1 is one outer-loop experiment and its NO GO discards one variant. Decision tables written inside one frame are inputs, not mandates.

## 6. Findings to carry forward

1. **The big lever is retry with verifier feedback.** It gave +13.9 pp over raw, against a2-full's +5.6 pp over stateless retry. Lineage, used as cross-attempt memory, is not worth its keep in this loop.
2. **The noise floor is large.** Single-attempt sampling variance alone produced 2 of a2-full's 3 wins. Detecting a +5 pp harness improvement at n ≈ 36 needs repeated evaluations or many more tasks. A single paired run cannot do it.
3. **Difficulty must be calibrated.** Full SWE-Bench Pro instructions put this model at its ceiling. Issue-only instructions put raw at 14%, so the task framing is itself a parameter of the fitness function.
4. **Evaluation is slow and expensive.** It took about 4.7 minutes per (task, arm) per shard, about 8.5 hours of wall time for 108 rows, and 83 M tokens. An evolution loop needs evaluations that are an order of magnitude cheaper, or far fewer of them, chosen well.
5. **The task supply is used up.** All 44 gated ansible tasks have now been seen. They can serve as a dev pool, but confirmations need new sealed tasks from other repos.

## 7. Design rules for the next loop

1. **The evaluator is frozen and outside the genome.** The researcher agent cannot write to the grader, verifier, task pool, sandbox or analysis. This is the one rule that, had it existed on day 1, would have saved the first cycle.
2. **Every change gets a fitness score against the current champion** on external held-out work, paired by task, at a fixed budget. A change with no score is not a change.
3. **Keep, discard, log.** Every experiment records its parent, diff, cost, score and decision. Discards are as valuable as keeps, and lineage belongs here, across harness variants.
4. **Measure the noise floor first.** Re-run the champion several times before trusting any delta. Promotions need a margin above that noise and confirmation on a sealed set.
5. **When blocked, change the design.** That means a short blocker note and a different path. Never a guard on an existing artifact.
6. **Test every guard against real outputs before a long run.** A guard that has never seen production data is a crash waiting to happen.
7. **Time-box, don't gate.** A safety mechanism is a dependency only if the experiment cannot run soundly without it.
