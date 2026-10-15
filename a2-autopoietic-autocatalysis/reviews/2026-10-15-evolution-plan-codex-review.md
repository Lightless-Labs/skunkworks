# Review: harness-evolution loop plan (draft 1)

**Reviewer:** Codex `gpt-6-luna`, xhigh, read-only bundle. **Date:** 2026-10-15. Disposition: plan §11.

**Do not start the unattended loop yet.** The largest issue is that the proposed evaluator runs candidate-controlled code while it can reach evaluator secrets and affect grading. Even after that is fixed, the keep rule and confirmation plan do not support the statistical claims or throughput targets as written.

## Findings

### 1. Blocker — Candidate code can reach secrets and influence grading

**Plan section:** [§2.1–2.2 and §10](<docs/plans/2026-10-15-harness-evolution-loop.md:45>)

**Failure scenario:** The plan reuses E1’s native grader for `/verify` and `/submit`. In E1, the verifier applies the full `test.patch` to a temporary tree under the private directory, then runs `pytest` on that tree. The pytest process is not sandboxed in [`grading.py`](<bench/e1/grading.py:158>) or [`verify.py`](<bench/e1/verify.py:37>). Candidate code imported by tests can inspect its working path, reach neighboring private task files, and read the manifest or gold patch. It can also read hidden tests materialized by the patch. The “whole-id” response matcher does not stop a leak written to the workspace or encoded in verifier output.

There is also a grading-integrity risk: test-path edits are reverted, but candidate-controlled files such as pytest configuration and importable plugins are not necessarily test paths. A candidate could alter test execution or its reports.

**Fix:** Treat code-under-test as adversarial. Run it in a separate, restricted grading boundary with no access to evaluator files, other workspaces, or network. Control pytest configuration and plugin loading from outside the candidate tree, and ensure candidate code cannot modify grading artifacts. Do not rely on output scanning as the secret boundary.

### 2. Major — Oracle workspace paths are not defined as episode-scoped capabilities

**Plan section:** [§2.2](<docs/plans/2026-10-15-harness-evolution-loop.md:59>)

**Failure scenario:** The API returns a filesystem path and accepts `{workspace}` paths for `/agent`, `/verify`, and `/submit`. The contract does not say those paths must have been created for the current episode or belong to that caller. Since the genome can call the localhost API directly, it could submit a different path, including another task’s workspace or an evaluator path. The untrusted harness must not be able to choose where the unsandboxed oracle runs Codex or grading.

**Fix:** Return opaque, server-side workspace IDs. Bind each ID to one episode and endpoint; resolve paths only inside the oracle, reject symlinks and paths outside that episode’s workspace root, and make submit a single atomic close operation.

### 3. Major — The “hard” budget can race or outlive the harness

**Plan section:** [§2.2–2.3](<docs/plans/2026-10-15-harness-evolution-loop.md:63>)

**Failure scenario:** `/agent` allows two concurrent calls. If the oracle checks remaining tokens before starting each call and accounts usage only when each finishes, both can be admitted against the same remaining budget and overshoot it. The 45-minute rule says to kill the harness, but not its subprocess tree or already-running Codex calls. A genome can also run long-lived local commands.

**Fix:** Reserve token capacity atomically before starting each call, including concurrent calls; set per-call ceilings and deadlines; and kill the whole process group when the episode ends. Make budget accounting and endpoint admission server-side, with explicit handling for interrupted calls.

### 4. Major — The keep rule selects winners on noisy evidence and mixes task sets

**Plan section:** [§4–5](<docs/plans/2026-10-15-harness-evolution-loop.md:96>)

**Failure scenario:** Three G0 runs give each task a `p̂` in increments of ⅓. S is then selected from tasks where those same runs happened to produce an intermediate result. That estimate is uncertain, but Δ treats it as known. Stage A is explicitly selected for a positive score (Δ ≥ 1.5), then included in the two-run mean used to keep the candidate. That retains winner’s-curse bias. With 20 candidates, some candidates will look good by chance.

Stage B also adds four hard tasks that were not in Stage A. Since Δ is a sum of solve outcomes minus `p̂`, these tasks can add up to four positive points in Stage B; averaging it with Stage A gives them a promotion effect that ordinary S tasks did not get. The plan does not define whether σ is measured on S or all 44 tasks, nor how its uncertainty or the uncertainty in `p̂` enters the margin.

This matters given E1: a2-full beat stateless retry by just 3 wins to 1 loss, with p = 0.3125, and the post-mortem attributes two of its three wins to single-attempt variance ([decision record](<docs/benchmark-results/external-value/e1/decision.md:21>), [post-mortem](<docs/post-mortems/2026-10-15-a2-first-cycle.md:88>)).

**Fix:** Compare candidate and current champion contemporaneously on the same predeclared tasks and matched budgets. Keep exploratory hard tasks as a separate metric, or include them in every comparison. Use Stage A only to screen; base a keep decision on independent Stage B evidence. Estimate uncertainty from the paired run-level data, including baseline-estimation error, and predefine how repeated candidate selection affects the decision.

### 5. Major — The sealed confirmation is weakly powered and may reuse data adaptively

**Plan section:** [§7](<docs/plans/2026-10-15-harness-evolution-loop.md:135>)

**Failure scenario:** With 48 paired tasks, a +10 pp difference requires at least five net wins. Five wins and no losses gives one-sided McNemar p = 0.03125; six wins and one loss has the same net difference but p = 0.0625, so it fails. The gate therefore has little room for even a few losses. E1’s primary comparison had only four discordant tasks out of 36, so the plan needs a power calculation based on realistic discordance, not just a maximum sample size.

The stated two tests at p ≤ .05 permit up to a 10% family-wise false-promotion rate. Bonferroni does not by itself protect a second confirmation if the same sealed tasks are reused and the next candidate is chosen after seeing the first result.

**Fix:** Precompute power for the expected effect and discordance, stratify the sample by repository, and use more of the available sealed tasks if needed. Decide in advance whether there will be one confirmation or two. For two, use disjoint samples or make both candidates and tests fixed before any results are revealed; choose the family-wise error rate explicitly.

### 6. Major — The confirmation budgets do not match

**Plan section:** [§2.3 and §7](<docs/plans/2026-10-15-harness-evolution-loop.md:73>)

**Failure scenario:** The candidate gets 2.0M tokens and 45 minutes total per episode. E1’s protocol specifies up to three attempts, with a 2.0M token ceiling and 1,500-second timeout per attempt ([protocol](<bench/external-value/protocol-v1.toml:42>)). Those are different caps. The candidate’s reasoning effort is also a genome choice, while E1 used high. “Same budget class” does not settle which resources are actually matched.

**Fix:** Pick one comparison: either give both arms the same total token, wall, model, and effort limits, or give the candidate the E1 per-attempt limits. If matching to the new 2.0M/45-minute budget, call the comparator a budget-matched a2-full variant rather than claiming it is the E1 configuration.

### 7. Major — Cost and throughput use medians where the loop needs totals

**Plan section:** [§4 and §10](<docs/plans/2026-10-15-harness-evolution-loop.md:96>)

**Failure scenario:** The 132-episode noise-floor estimate uses the stateless-retry median of 0.78M tokens, giving about 103M. E1 actually spent 34.2M tokens on 36 stateless-retry tasks, or about 0.95M each; that scales to about 125M for 132 episodes before medium/high comparisons. Ten screens of 12–16 tasks at that average would use roughly 114–152M tokens, without Stage B or champion refreshes.

The wall-time target is similarly tight. At the E1 stateless-retry median of 603 seconds, 16 tasks on four shards take about 40 minutes before setup and verification overhead. Ten screens alone are then close to seven hours; Stage B runs and champion refreshes add more. E1’s actual run took 8.5 hours for 108 mixed rows on three shards, which does not support a 5.5-hour estimate for the larger noise-floor run.

**Fix:** Estimate from E1’s observed means and upper-tail task times, and cost the full schedule: smoke runs, screens, Stage B frequency, champion refreshes, retries, and confirmation. Either lower the nightly experiment target or reduce the required per-experiment workload before promising ten experiments per night.

### 8. Major — The researcher can overfit the reused dev set but gets little candidate-specific evidence

**Plan section:** [§3 and §8–10](<docs/plans/2026-10-15-harness-evolution-loop.md:87>)

**Failure scenario:** The researcher sees problem statements and resolved labels for all 44 tasks already used in E1. The “no task-specific content” rule cannot prevent it from adding rules keyed to those known issues or their repository. The two sealed runs are the only generalization check. At the same time, candidate per-task rows are private; the researcher mostly sees aggregate deltas for its discarded variants, so it has little evidence about which part of a hypothesis helped. With the measured noise and 12–16 tasks per screen, 20 logged experiments could become prompt tweaks selected on noisy aggregate results.

**Fix:** Add a larger, rotating development pool across repositories, keep a researcher-visible training slice separate from an evaluator-hidden screening slice, and report enough paired development evidence to assess hypotheses without exposing the confirmation set. Do not treat the prose scope rule as an enforcement boundary.

## Simpler alternatives

- For the next few experiments, use a human-chosen queue of structural variants with the existing E1 runner and a fixed, matched-budget champion arm. That tests whether anything beyond stateless verifier-feedback retry helps before building the custom oracle, resumable sharding, adaptive S, and two-stage selector.
- If autonomous iteration is required now, keep one small candidate interface and one trusted runner. Use a fixed development task list and a paired champion run for every candidate; remove the adaptive `p̂`-based screen and Stage B task-set changes. Run one preregistered, budget-matched sealed confirmation after selecting the candidate.

## Five changes I would make first

1. **Secure grading before any provider run:** isolate candidate execution from evaluator files and reports; control test discovery, configuration, and plugins.
2. **Make the oracle a real capability boundary:** use episode-scoped workspace IDs, validate every operation, reserve token budget atomically, and kill all child processes at episode end.
3. **Replace the keep rule:** use contemporaneous paired candidate/champion runs on the same tasks; make Stage B independent and use one fixed task set.
4. **Rework confirmation:** match the E1 comparator’s model and budget, calculate power from E1 discordance, and predefine the sample and family-wise error policy.
5. **Replan the loop’s learning and cost model:** separate researcher-visible dev data from screening data, provide useful candidate feedback, and budget from observed means including Stage B and champion refreshes.