# Verifier-Derived Relevant Files TODO

Created: 2026-05-10
Completed: 2026-05-12

## Problem

`ContextPack.relevant_files` is empty in the self-correction path. When external verification names a failing test in `crates/a2ctl/src/main.rs`, the next attempt is not structurally pointed at that file.

Observed fact: repeated `compound-hidden` attempts touched only `a2_core/src/lib.rs`, even when prior verification output named an `a2ctl` test and assertion.

## Goal

Derive relevant files from failed verification output and include them in the next `ContextPack` / prompt.

## Proposed approach

Add a small failure-to-file resolver that can use:

- explicit paths in panic output, e.g. `crates/a2ctl/src/main.rs:1556:9`
- failing test names mapped via source search
- package names from commands like `cargo test -p a2ctl ...`

For `compound-hidden`, the next attempt should include:

```text
crates/a2ctl/src/main.rs
```

as a relevant file when the prior failure focus names `tests::ignores_non_task_mentions_inside_comments_and_strings` or `crates/a2ctl/src/main.rs`.

## Acceptance criteria

- [x] Failed verification output with a source path adds that path to `ContextPack.relevant_files`.
- [x] Relevant files are deduplicated and bounded.
- [x] A unit/integration test proves prior lineage containing `crates/a2ctl/src/main.rs:...` reaches WorktreeCatalyst as a relevant file.
- [x] Prompt rendering includes the verifier-derived file path.
- [x] First attempts without prior verification failures keep current behavior.

## Verification

```bash
cargo test -p a2d -p a2_workcell
cargo run -p a2ctl -- sentinel --workspace .
```

Then run `compound-hidden` and inspect JSONL `touched_files` for whether attempts after failure inspect/touch `crates/a2ctl/src/main.rs`.
