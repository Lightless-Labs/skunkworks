# SWE-Bench Pro Access Manifest — Operator Template

**Created:** 2026-07-10
**Status:** Operator-filled template only; not a reviewed manifest; does not unblock SWE-Bench Pro readiness.

## Purpose

This document is a non-sensitive intake template for the trusted SWE-Bench Pro evaluator operator. It shows the exact metadata shape A²D can inspect without seeing benchmark-private sources, hidden tests, solutions, credentials, raw evaluator output, or host-local paths in committed artifacts.

The companion JSON skeleton is `docs/plans/swe-bench-pro-access-manifest.template.json`. It is invalid by default because it contains placeholder values only; it is not a reviewed manifest and does not unblock readiness.

A real manifest must be filled and reviewed outside A²D-visible prompts, provider artifacts, and committed repository files. This template is not a manifest and must not be passed to `a2d swe-bench-pro-evaluate` as proof of access.

## Hard rules

- Do not commit a filled manifest.
- Do not commit benchmark sources, hidden tests, reference solutions, credentials, evaluator stdout/stderr, candidate solution patches, or host-local paths.
- Do not paste a filled manifest, sealed evaluator command, evaluator output, or private paths into A²D/coder prompts.
- Do not label Senior SWE-Bench, local-wrapper, synthetic, or scaffold evidence as SWE-Bench Pro.
- The sealed evaluator may return only pass/fail/metrics and opaque IDs/hashes to A²D.
- The manifest below is valid only after an external trusted operator fills it, reviews it, and supplies it through a private path or stdin outside the normal agent-visible artifact path.

## Operator-created public context file

Create a separate **public-only** context file outside the repository or in an ephemeral operator workspace. It may contain only information that can safely be shown to A²D/coding agents, for example:

```json
{
  "schema_version": "a2d.swe-bench-pro-public-context.v1",
  "instance_id": "__OPAQUE_PUBLIC_INSTANCE_ID__",
  "repo": "owner/repo",
  "problem_statement": "__PUBLIC_PROBLEM_STATEMENT_ONLY__",
  "allowed_context_summary": [
    "__PUBLIC_CONTEXT_ITEM__"
  ],
  "forbidden_context": [
    "benchmark-private source files",
    "hidden tests or hidden test names/output",
    "reference or official solution material",
    "credentials, tokens, private URLs, host-local paths"
  ]
}
```

Compute the Git object hash of that public context file:

```bash
git hash-object "$OPERATOR_PUBLIC_CONTEXT_JSON"
```

The resulting hash goes in `public_context_hash` below. The path itself must stay outside committed artifacts; A²D inspection redacts it.

## Operator-filled manifest shape

The trusted operator should create a JSON file with exactly these fields and no private extras. Placeholder values are intentionally not usable as-is.

```json
{
  "schema_version": "a2d.swe-bench-pro-access-manifest.v1",
  "benchmark": "swe-bench-pro",
  "instance_id": "__OPAQUE_PRO_INSTANCE_ID__",
  "repo": "owner/repo",
  "public_context_path": "__OPERATOR_PRIVATE_PUBLIC_CONTEXT_PATH_NOT_COMMITTED__",
  "public_context_hash": "__GIT_OBJECT_HASH_OF_PUBLIC_CONTEXT__",
  "sealed_evaluator_command": [
    "__OPERATOR_PRIVATE_SEALED_EVALUATOR_ENTRYPOINT_NOT_COMMITTED__",
    "__OPAQUE_SAFE_ARG_IF_NEEDED__"
  ],
  "hidden_holdouts": true,
  "github_solution_search_allowed": false,
  "benchmark_sources_visible_to_a2d": false,
  "solution_material_visible_to_a2d": false,
  "evaluator_output_policy": "pass_fail_metrics_only"
}
```

Allowed manifest fields are only:

- `schema_version`
- `benchmark`
- `instance_id`
- `repo`
- `public_context_path`
- `public_context_hash`
- `sealed_evaluator_command`
- `hidden_holdouts`
- `github_solution_search_allowed`
- `benchmark_sources_visible_to_a2d`
- `solution_material_visible_to_a2d`
- `evaluator_output_policy`

Forbidden manifest content includes any benchmark source path, hidden test path, solution/reference patch path, credential path, token, secret, raw evaluator command output, or solution-derived metadata.

## Review checklist for the trusted operator

Before handing the manifest path to A²D:

1. Confirm `benchmark` is exactly `swe-bench-pro`.
2. Confirm `hidden_holdouts` is `true`.
3. Confirm `github_solution_search_allowed` is `false`.
4. Confirm `benchmark_sources_visible_to_a2d` is `false`.
5. Confirm `solution_material_visible_to_a2d` is `false`.
6. Confirm `evaluator_output_policy` is exactly `pass_fail_metrics_only`.
7. Confirm the public context file contains no benchmark-private source, hidden tests/names/output, reference solution, credential, or host-local path.
8. Confirm `public_context_hash` matches `git hash-object` of the public context file.
9. Confirm the sealed evaluator command runs only inside the sealed evaluator boundary and emits only pass/fail/metrics plus opaque IDs/hashes.
10. Confirm the filled manifest is outside git and outside A²D-visible prompts/artifacts.
11. Confirm no filled manifest, private public-context path, sealed evaluator command, raw evaluator output, hidden/source/solution material, credentials, or local checkout paths were staged for commit.

Suggested pre-handoff safety checks from the repository root:

```bash
# Should show no filled SWE-Bench Pro manifest, private public context, evaluator script,
# credential, hidden test, solution, or host-local path staged or untracked in this repo.
git status --porcelain

# Optional targeted scan of tracked docs/artifacts before commit. This should not find
# filled operator paths, credentials, or private benchmark material.
rg -n \
  'benchmark_source_path|benchmark_sources_path|benchmark_repo_path|hidden_tests_path|hidden_holdout_path|solution_patch_path|reference_solution_path|credential_path|credentials_path|token|secret|BEGIN RSA|BEGIN OPENSSH|/Users/' \
  docs runs crates
```

The scan pattern above intentionally names forbidden field names and generic path markers. A hit in this template is expected for the forbidden-field documentation; a hit containing real private values is not acceptable.

## A²D-side inspection commands after operator handoff

Only after the trusted operator supplies the filled manifest path outside A²D-visible prompts/artifacts:

```bash
a2d swe-bench-pro-access-manifest-inspect --manifest "$OPERATOR_FILLED_MANIFEST_JSON"
```

Then, if inspection is clean:

```bash
a2d swe-bench-pro-readiness --official-evaluator-manifest "$OPERATOR_FILLED_MANIFEST_JSON"
```

Only after readiness passes may a sealed evaluation be attempted. Even then, persisted A²D artifacts must contain only redacted evaluation metadata and `a2d.fitness-evidence.v1`, never benchmark-private material.
