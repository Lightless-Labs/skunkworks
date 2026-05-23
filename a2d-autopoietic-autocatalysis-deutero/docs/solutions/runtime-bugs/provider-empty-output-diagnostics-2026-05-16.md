---
title: "Provider Empty Output Diagnostics"
date: 2026-05-16
category: runtime-bugs
module: metabolism
problem_type: observability_gap
component: provider-output-routing
symptoms:
  - "Provider subprocess succeeds but no artifact is materialized"
  - "Architect invocations fail with no usable system_patch and little evidence about what the provider emitted"
  - "OpenCode/Gemini/Kimi parser gaps are hard to distinguish from genuinely empty model output"
root_cause: parsed_provider_text_was_the_only_failure_evidence
resolution_type: code_fix
severity: medium
tags:
  - provider
  - diagnostics
  - empty-output
  - architect
  - observability
---

# Provider Empty Output Diagnostics

## Problem

A successful provider process can still produce no materialized A²D artifact. This is especially costly for the architect enzyme: a response may be empty after provider-specific parsing, malformed as `SystemPatch`, or present only in a provider's raw CLI event stream.

Before this fix, the failure surfaced as a generic no-materialized-output or parse error. The system knew that no artifact was routed, but not what the provider actually emitted. That made parser bugs, generic acknowledgements, malformed JSON, and truly empty output look the same.

## Root cause

`InvocationResponse` only carried parsed `text`. CLI provider raw stdout was discarded immediately after parsing. When `materialize_outputs()` produced an empty artifact set, the metabolism had no raw evidence to attach to the workcell failure or provider cooldown record.

For `SystemPatch` parse failures, the rejection reason also omitted a preview of the malformed artifact, so architect contract failures were under-instrumented.

## Solution

Add an optional `raw_output` field to `InvocationResponse`:

- CLI providers populate it with raw stdout.
- SDK providers may leave it empty until they have a useful raw body/event stream to expose.
- The metabolism never routes raw output as an artifact; it only uses a sanitized, trimmed preview in diagnostics.

No-materialized-output failures now include:

- expected product set
- parsed text preview, including explicit `<empty>`
- raw stdout preview when it differs from parsed text

Malformed `SystemPatch` rejections now include a sanitized parsed artifact preview.

## Safety properties

- Raw output previews are bounded to 800 characters.
- Control characters are stripped and whitespace is collapsed.
- The preview is diagnostic only; artifact routing still uses the parsed provider text and existing gates.
- Architect source changes still require `SystemPatch` parsing plus self-sandbox validation.

## Tests

Updated `empty_provider_output_fails_invocation` to assert the failed workcell records parsed-empty output and raw stdout preview content.

`cargo test` passes: 137 tests passing, 2 ignored.

## Related

- `docs/solutions/runtime-bugs/opencode-write-tool-output-recovery-2026-05-01.md`
- `docs/solutions/runtime-bugs/provider-invocations-need-timeouts-and-output-format-handling-2026-04-04.md`
- `docs/solutions/runtime-bugs/provider-circuit-breaker-temporary-cooldown-2026-04-23.md`
