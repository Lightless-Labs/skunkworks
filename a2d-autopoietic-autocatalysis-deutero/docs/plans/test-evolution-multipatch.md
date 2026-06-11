# Test Evolution Multipatch SystemPatch

**Created:** 2026-06-11

## Goal

Allow the architect to evolve production code and tests atomically by returning multiple `SystemPatch` objects in one `system_patch` artifact.

## Requirements

- Preserve backward compatibility for legacy single patch JSON and `{"action":"noop"}`.
- Accept JSON arrays for patch batches, including fenced arrays and `{"system_patch":[...]}` artifacts after materialization.
- Validate a batch against one temp project copy with all patches applied before `cargo test`.
- Reject empty batches, duplicate target paths, protected files, ineligible files, missing files, and failing combined `cargo test`.
- Never queue partial patches: if any member fails gates or combined validation, `pending_patches()` is unchanged and lineage records a rejection.
- Make existing standalone test files eligible for architect modification where needed for current internal coverage.

## Implementation

1. Add `self_sandbox::validate_patches(project_root, patches)` and make `validate_patch` delegate to it.
2. Extend `Metabolism::apply_system_patch` to parse noop, single patch, and patch arrays; append to `pending_patches` only after successful batch validation.
3. Extend JSON extraction to recognize raw/fenced arrays.
4. Update the architect prompt to document test evolution and multi-patch array output.
5. Add mock/minimal-fixture tests proving combined acceptance and no partial queue on failure.

## Validation

- Focused self-sandbox and metabolism multipatch tests.
- Full `cargo test`.
