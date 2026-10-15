You are researching structural changes to the A² coding-agent genome. Read this file and the repository README first. The genome is `genome/policy.py`, `genome/prompts/`, and `genome/genome.toml`. The policy uses the action protocol described in `genome/README.md`: it may make at most 3 agent calls, each with a timeout of at most 1500 seconds; choose effort only from low, medium, or high; use at most 8 policy steps; and remain compatible with Python 3.9 and its standard library. Policy execution has a 60 second CPU limit per step, no network, and no file access beyond the genome and pristine base snapshot.

The objective is to raise the resolved rate on held-out coding tasks graded by hidden tests. Visible verifier results are the only test feedback available to you. The hidden grader and its state are outside your reach.

The noise floor is substantial: a change must plausibly gain several tasks out of about 20 to be detectable. Prompt polish is below that noise floor. Make one structural hypothesis per experiment. Possible structures include verifier-selected best-of-n, plan-then-execute, continuing from a candidate for repair versus restarting, reproduce-then-fix, review-and-repair, effort allocation across calls, or a different way to summarize verifier failures.

Read the experiment log before proposing a change, including discarded hypotheses and their measured deltas. Do not repeat a discarded idea unless you can state a concrete reason the new experiment differs.

Hard constraints:

- Do not add task-specific or repository-specific content. Do not copy names, paths, or strings from task digests into the genome.
- Never try to read evaluator state, detect the grader, or alter tests or test configuration.
- Keep every candidate valid Python 3.9 and within the action protocol and resource limits above.

Edit files under `genome/` only. Write exactly one line describing the experiment hypothesis to `genome/HYPOTHESIS.txt`.
