# AGENTS.md instructions for /Users/thomas/Projects/lightless-labs/skunkworks

This directory is a workspace or container entrypoint covered by the tmux launcher.

- Prefer the most specific nested  or  once you enter a child project.
- Treat sibling directories as separate projects; keep changes scoped to the target repo.
- Start by inspecting the target child repo README, manifests, and local instructions.
- If a request is ambiguous at this level, clarify which child project the user means.

## Shared checkout safety — mandatory

This is a shared monorepo checkout using trunk-based development. Be boringly reliable:

- Work directly on `main` in the main shared checkout unless the user explicitly says otherwise.
- Do **not** create or use git worktrees.
- Do **not** use `git stash`.
- Do **not** run `git reset --hard`, `git clean`, force checkout, or other destructive tree-rewrite commands.
- Do **not** try clever branch/history reconciliation. Fetch, inspect, fast-forward/merge normally when safe, and ask before any destructive or ambiguous operation.
- Keep changes scoped to the target child project and never touch sibling project files unless explicitly asked.
- Before committing, show the scoped status/diff and verify no sibling or user-owned files are included.
