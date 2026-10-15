# Genome G0

A genome is the policy the frozen evaluator runs for each task. G0 implements stateless retry: it starts each agent call from the original code, includes the previous visible verifier failures in the next prompt, and submits the first visibly passing candidate. If none passes within the call budget, it submits the candidate with the most passing visible tests, with later candidates winning ties.

## Action protocol

The driver sends one JSON state object on standard input. The policy prints exactly one JSON object:

- `{"agent":{"base":"fresh","prompt":"...","effort":"high"}}` asks the driver for one fresh agent call. The base may also be a prior candidate index.
- `{"submit":{"candidate":0}}` selects a previous candidate for grading.

The driver validates every field and owns the budgets. Policies have at most 8 steps, 3 agent calls, and 1500 seconds per agent call. Effort is limited to low, medium, or high. Policy execution is sandboxed without network or file writes and can read only this genome, the Python runtime, and a pristine base snapshot. The policy receives no opaque task id, private files, hidden test ids, or gold patch.

Genome changes must be task and repository agnostic. Do not include task specific content or copy an eight word sequence from a development problem statement or digest. Prefer structural hypotheses such as verifier selected best of n, plan then execute, test writing first, review and repair, continue versus restart, or effort allocation.
