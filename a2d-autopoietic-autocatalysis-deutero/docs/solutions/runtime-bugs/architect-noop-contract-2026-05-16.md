---
title: "Architect No-op Contract"
date: 2026-05-16
category: runtime-bugs
module: metabolism
problem_type: protocol_fix
component: architect
symptoms:
  - "Architect has no valid way to say no source change is warranted"
  - "No-change decisions collapse into empty output, prose, or malformed SystemPatch failures"
  - "Provider/parser failures are hard to distinguish from legitimate abstention"
root_cause: architect_output_schema_only_admitted_patches
resolution_type: code_fix
severity: medium
tags:
  - architect
  - autopoiesis
  - noop
  - system-patch
  - protocol
---

# Architect No-op Contract

## Problem

The architect enzyme had one valid product shape: a `SystemPatch` containing a target file and complete replacement content. That is appropriate when the system needs source modification, but brittle when the correct architectural decision is to abstain.

Without an explicit no-op contract, models that concluded "no source change is warranted" tended to emit prose, markdown, an empty answer, or malformed JSON. The metabolism treated those as failed patch attempts. This conflated two different events:

1. The provider failed to produce a valid artifact.
2. The architect made a valid no-change decision.

## Root cause

The architect protocol encoded self-modification as "always produce a patch." It had no typed representation for abstention. In A²D terms, the autopoietic mechanism lacked an immune-compatible null operation: a way to preserve the system state intentionally rather than by failing validation.

## Solution

The architect prompt now admits two JSON schemas:

```json
{"action":"patch","file_path":"crates/...","new_content":"...complete file content..."}
```

or:

```json
{"action":"noop","reason":"why no source change is warranted"}
```

`apply_system_patch()` now parses the action field before self-sandbox validation:

- `action: "noop"` records a successful no-op patch record with a reason and does not call the self-sandbox.
- `action: "patch"` proceeds through the existing `SystemPatch` parse and self-sandbox path.
- Legacy bare `SystemPatch` JSON remains accepted for backwards compatibility.
- Unknown actions are rejected with a preview of the malformed artifact.

Patch summaries now include `NOOP: <reason>` so the architect's abstention remains visible as an artifact rather than disappearing.

## Safety properties

- No-op does not modify source files.
- No-op does not bypass validation for actual patches.
- Protected-file and automated-modifiable gates remain unchanged.
- Legacy patch producers continue to work.

## Tests

Added `architect_noop_output_is_successful_patch_record` covering the no-op path and patch summary.

`cargo test` passes: 138 tests passing, 2 ignored.

## Related

- `docs/solutions/runtime-bugs/provider-empty-output-diagnostics-2026-05-16.md`
- `docs/solutions/runtime-bugs/architect-patches-need-relevance-gate-2026-05-01.md`
