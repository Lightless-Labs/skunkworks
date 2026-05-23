---
title: "OpenCode Write Tool Output Recovery"
date: 2026-05-01
category: runtime-bugs
module: providers
problem_type: bug_fix
component: cli-provider
symptoms:
  - "OpenCode returns successful invocations with no useful parsed artifact"
  - "Model writes the requested artifact to an isolated temp file, then final text is only 'Done.'"
  - "A²D wastes an invocation/cooldown even though the artifact existed in OpenCode's NDJSON stream"
root_cause: stdout_parser_ignored_tool_write_payloads
resolution_type: code_fix
severity: medium
tags:
  - opencode
  - cli-provider
  - ndjson
  - output-parsing
  - isolated-cwd
  - empty-output
---

# OpenCode Write Tool Output Recovery

## Problem

After CLI providers were moved into an isolated temp cwd, OpenCode could still satisfy a prompt by using its `write` tool instead of placing the artifact in final assistant text. The repository stayed protected, but A²D only consumed parsed text events. When the final assistant text was empty or a generic acknowledgement such as `Done.`, the actual artifact was ignored.

This caused wasted invocations: the provider process succeeded and the artifact existed in the NDJSON tool event, but the metabolism saw no materialized product or saw only `Done.`.

## Root cause

`CliProvider::opencode` parsed only current text events shaped like:

```json
{"type":"text","part":{"text":"..."}}
```

It did not handle:

- older/top-level text events: `{"type":"text","text":"..."}`
- OpenCode write tool payloads: `part.state.input.content`
- generic final acknowledgements after file writes

The isolated cwd fix made filesystem side effects safe, but the parser still discarded safe artifact content already present in stdout.

## Solution

Refactor OpenCode parsing into `parse_opencode_output()` and collect three channels:

1. current `/part/text` assistant text
2. legacy top-level `/text` assistant text
3. `write` tool `input.content` payloads

If assistant text is empty or a generic completion acknowledgement (`Done.`, `Complete.`, etc.) and a write payload exists, return the last written content as the provider text. If the final text is substantive, keep preferring it over scratch-file writes.

## Why this is safe

The provider still runs in an isolated temp cwd, so OpenCode cannot mutate the A²D repo directly. Recovering `write` payloads does not bypass the architect gate: architect output still has to parse as `SystemPatch` and pass self-sandbox validation before source changes are applied.

## Tests

Added provider unit tests covering:

- current `/part/text` NDJSON shape
- legacy top-level `/text` shape
- fallback from `Done.` to `write` content
- preference for substantive final text over scratch writes

`cargo test` passes: 132 tests passing, 2 ignored.

## Related

- `docs/solutions/runtime-bugs/cli-providers-must-run-in-isolated-cwd-2026-04-28.md`
- `docs/solutions/runtime-bugs/provider-invocations-need-timeouts-and-output-format-handling-2026-04-04.md`
