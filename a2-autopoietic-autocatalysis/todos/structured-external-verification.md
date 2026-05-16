# Structured External Verification TODO

Created: 2026-05-10
Completed: 2026-05-12

## Problem

External verification truth is still encoded as text inside `LineageRecord.patch_rationale` with `[external verify: ...]` markers. The loop can render this text, but downstream code must parse fragile prose to recover failing tests, commands, stdout/stderr, and pass/fail state.

## Goal

Promote external verification from rationale text into typed protocol data persisted with lineage.

## Proposed shape

Add a typed record, likely in `a2_core::protocol`:

```rust
pub struct ExternalVerification {
    pub passed: bool,
    pub command: String,
    pub exit_code: Option<i32>,
    pub failing_tests: Vec<String>,
    pub failure_focus: Vec<String>,
    pub stdout_excerpt: String,
    pub stderr_excerpt: String,
    pub verified_at: chrono::DateTime<chrono::Utc>,
}
```

Then attach it to `LineageRecord`, with SQLite migration support in `a2_archive`.

## Acceptance criteria

- [x] `LineageRecord` has structured external verification data with serde defaults for legacy records.
- [x] `a2_archive::SqliteLineageStore` persists and reads the new data.
- [x] `a2ctl run --apply` writes post-apply verification into the structured field.
- [x] Prior motif rendering consumes the structured field first and only falls back to legacy rationale markers.
- [x] Existing legacy lineage rows still migrate/read successfully.
- [x] Tests cover failed verification with stdout + stderr + failing test extraction.

## Verification

```bash
cargo test -p a2_core -p a2_archive -p a2d -p a2_workcell -p a2ctl
cargo run -p a2ctl -- sentinel --workspace .
```
