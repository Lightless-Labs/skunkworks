# AGENTS.md instructions for /Users/thomas/Projects/lightless-labs/skunkworks/a2-autopoietic-autocatalysis

This project currently has no . Use this file as the cross-agent entrypoint.

- Inspect the repo before editing: start with the top-level README and build/test manifests.
- Prefer any more specific nested  or  files if you move into a subdirectory that has them.
- Keep changes scoped to this project unless the user explicitly asks for cross-project work.
- Run the smallest relevant verification command for the files you touch before finishing.

## Shared checkout safety — mandatory

This is a shared monorepo checkout using trunk-based development. Be boringly reliable:

- Work directly on `main` in the main shared checkout unless the user explicitly says otherwise.
- Do **not** create or use git worktrees for assistant/operator git workflow in this shared checkout.
- This workflow rule does **not** prohibit A² product/runtime support for isolated candidate worktree execution; do not remove or rewrite runtime worktree code/docs solely because of this instruction.
- Do **not** use `git stash`.
- Do **not** run `git reset --hard`, `git clean`, force checkout, or other destructive tree-rewrite commands.
- Do **not** try clever branch/history reconciliation. Fetch, inspect, fast-forward/merge normally when safe, and ask before any destructive or ambiguous operation.
- Keep changes scoped to this A² project and never touch sibling project files unless explicitly asked.
- Before committing, show the scoped status/diff and verify no sibling or user-owned files are included.
