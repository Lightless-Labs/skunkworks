---
module: a2d-providers
tags: [senior-swe-bench, provider-policy, benchmark-integrity]
problem_type: best-practice
---

# CLI provider subprocesses must receive the no-public-solution-search policy env

Senior SWE-Bench coding agents must not search GitHub or other public solution sources. Prompt text and checkout/tool boundaries are not enough observability for subprocess-based providers: the launched CLI should also receive explicit policy flags in its environment.

A²D now injects these no-search flags into every `CliProvider::invoke` child process:

- `A2D_GITHUB_SOLUTION_SEARCH_ALLOWED=false`
- `A2D_PUBLIC_SOLUTION_SEARCH_FORBIDDEN=true`
- `A2D_SENIOR_SWE_BENCH_GITHUB_SOLUTION_SEARCH_ALLOWED=false`
- `A2D_SENIOR_SWE_BENCH_PUBLIC_SOLUTION_SEARCH_FORBIDDEN=true`

Important scope: this is policy propagation and observability, not OS/network isolation. It does not prove that a provider had no network egress; separate sandboxing would be needed for enforcement.

Regression coverage should include a fake CLI subprocess that prints these variables, plus a provider-injected sentinel (`A2D_PROVIDER_POLICY_ENV_SOURCE=a2d-cli-provider`), so the test fails if `.envs(provider_no_public_solution_search_env())` is removed even though the helper still returns the right values or the parent shell happens to export similarly named policy variables.
