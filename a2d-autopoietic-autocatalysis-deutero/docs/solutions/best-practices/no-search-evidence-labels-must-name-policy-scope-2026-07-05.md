---
module: a2d-cli
tags: [senior-swe-bench, fitness-evidence, benchmark-integrity]
problem_type: best-practice
---

# No-search evidence labels must name policy scope

Senior SWE-Bench evidence used a result label named `has_no_solution_search`. That label was too strong: A²D had validated task/manifest policy and propagated no-search env flags, but it had not performed OS/network forensics proving that a provider or evaluator had no egress.

Use an explicit policy-scope label instead:

- `has_no_solution_search_policy_declared`

This preserves the useful evidence that the accepted task/evaluator path forbids public solution search, while avoiding an overclaim that the runtime enforced network isolation or proved no external lookup occurred.

Regression coverage should assert both positive and failed local-evaluator reports use the scoped label and do not emit the ambiguous `has_no_solution_search` label. Failed local-evaluator reports must remain regressing/non-acceptable even though the no-search policy was declared.
