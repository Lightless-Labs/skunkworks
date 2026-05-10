# Anti-Repeat Retry Strategy TODO

Created: 2026-05-10

## Problem

The loop can remember a failed patch shape, but it does not yet prevent or penalize repeating that shape. In `compound-hidden`, multiple retry attempts produced the same behavioral shape: one-line change in `a2_core/src/lib.rs`, no change in `a2ctl`, verifier still fails.

## Goal

Make repeated failed patch shapes visible and actionable as retry strategy.

## Proposed approach

Use lineage patch diffs and verifier failures to detect repeated partial fixes:

- prior touched files
- current/previous diff stats
- unresolved failing tests
- whether the same file-only patch pattern recurs

Then render or encode a constraint for the next attempt:

```text
A prior failed attempt touched only `a2_core/src/lib.rs`.
The unresolved verifier failure is in/near `crates/a2ctl/src/main.rs`.
Do not repeat the previous patch shape alone; inspect and address the unresolved verifier failure.
```

Longer-term, make this a typed strategy rather than text, e.g.:

```rust
StrategyChange::AddressUnfixedVerifierFailures {
    failing_tests: Vec<String>,
    prior_touched_files: Vec<PathBuf>,
}
```

## Acceptance criteria

- [ ] A repeated failed touched-file set can be detected from prior lineage.
- [ ] Retry context includes prior touched files and unresolved verifier failures.
- [ ] The next prompt explicitly warns when the previous patch shape did not address all failures.
- [ ] Unit test covers `prior_touched_files = [a2_core]` plus unresolved `a2ctl` failure.
- [ ] No warning is emitted when prior failure already touched files associated with unresolved verifier output.

## Verification

```bash
cargo test -p a2d -p a2_workcell
cargo run -p a2ctl -- sentinel --workspace .
```

Then rerun `compound-hidden` and compare `touched_files` across attempts.
