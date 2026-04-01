---
title: Multi-Model Autoresearch Dispatch Patterns
date: 2026-04-01
category: best-practices
module: multi-model-orchestration
problem_type: best_practice
component: tooling
severity: medium
applies_when:
  - Dispatching parallel tasks to multiple AI model CLIs (Claude, Gemini, Codex, OpenCode)
  - Running autoresearch or refinery-style diverge-evaluate-converge workflows
  - Orchestrating background agents alongside external CLI processes
tags: [multi-model, autoresearch, cli-dispatch, gemini, codex, opencode, diverge-converge]
---

# Multi-Model Autoresearch Dispatch Patterns

## Context

While building the A² (Autopoietic Autocatalysis) autoresearch system, we needed to dispatch the same prompt to 4 AI models in parallel (Claude, Gemini, Codex, OpenCode), collect their independent outputs, then run cross-evaluation where each model reviews all others. This revealed several non-obvious patterns and failure modes when orchestrating multiple AI CLIs from a Claude Code session.

## Guidance

### 1. Background shell processes vs. sub-agents

Claude sub-agents write to files reliably and report completion. External CLI tools backgrounded with `&` in a single Bash call cause the **parent shell to exit immediately** while children continue running. The `run_in_background` Bash parameter exits when the shell exits, not when the children finish.

**Pattern**: Use `wait` at the end of the Bash command to block until all children complete, OR poll file sizes separately.

```bash
# WRONG — shell exits, you get notified "done" while models are still running
gemini -p "..." > out1.md &
codex exec "..." -o out2.md &
echo "dispatched"

# RIGHT — shell waits for all children
gemini -p "..." > out1.md &
codex exec "..." -o out2.md &
wait
echo "all done"
```

### 2. CLI-specific invocation patterns

| Model | Non-interactive command | Output capture | Gotcha |
|-------|----------------------|----------------|--------|
| Claude | Sub-agent with `run_in_background=true` | Writes files directly | Most reliable; can read local files without bundling |
| Gemini | `gemini -p "..." --sandbox --approval-mode yolo -o text` | stdout redirect | System prompt via `GEMINI_SYSTEM_MD` env var only, no `--json-schema` |
| Codex | `codex exec "..." --full-auto --skip-git-repo-check -o file.md` | `-o` writes final message at completion | `-o` file is empty until process finishes; need `--skip-git-repo-check` outside git repos |
| OpenCode | `opencode run "..." --model provider/model --format default` | stdout redirect | Model format is always `provider/model`; no sandbox mode |

### 3. File bundling for external models

Claude sub-agents can read files by path. External CLIs need content piped via stdin or embedded in the prompt. Bundle research files into a single temp file:

```bash
cat research/*.md > /tmp/bundle.md
gemini -p "$PROMPT" < /tmp/bundle.md > output.md
```

For OpenCode, stdin piping is unreliable — embed content directly in the prompt string instead.

### 4. Context window management

When orchestrating multi-phase research, **never read agent output files back into the main context**. Instead:
- Let agents write to well-known file paths
- Pass those paths to next-phase agents
- Or bundle files for external CLIs

This keeps the orchestrator's context lean across phases.

### 5. The diverge-evaluate-converge pattern

The three-phase autoresearch workflow:

1. **Diverge**: Same prompt, all models independently, no cross-contamination. Maximize diversity.
2. **Evaluate**: Every model reads all proposals and scores them. Include self-criticism instructions — models tend to favor their own output otherwise.
3. **Converge**: Synthesize based on cross-evaluation. Convergent themes are load-bearing truths; contradictions are real design decisions; blind spots are what to research next.

Key insight: having 5+ evaluators (using multiple OpenCode model backends) produces richer signal than 3. The marginal cost is near-zero when running in parallel.

## Why This Matters

Multi-model consensus is more rigorous than single-model output, but the orchestration is fragile. Shell process lifecycle mismatches, CLI-specific quirks, and context window pressure can silently degrade the workflow. These patterns prevent the most common failures.

## When to Apply

- Any refinery-style multi-model workflow
- Autoresearch loops (research → propose → evaluate → synthesize)
- Multi-agent code review or analysis tasks
- Any time you're dispatching to >2 external AI CLIs in parallel

## Examples

### Before: Naive dispatch (broken)
```bash
# All three background, shell exits, notification fires prematurely
gemini -p "analyze X" > g.md &
codex exec "analyze X" -o c.md &
opencode run "analyze X" > o.md &
echo "done"  # This triggers the "done" notification
```

### After: Reliable dispatch
```bash
# Shell waits for all children before exiting
gemini -p "analyze X" --sandbox -y -o text > g.md 2>/dev/null &
codex exec "analyze X" --full-auto --skip-git-repo-check -o c.md 2>/dev/null &
opencode run "analyze X" --format default > o.md 2>/dev/null &
wait
echo "all models complete"
```

### 6. Correct OpenCode model names

OpenCode model names are NOT intuitive. Always run `opencode models` to verify. Common mappings:

| What you want | Actual model ID |
|---------------|----------------|
| GLM 5.1 | `zai-coding-plan/glm-5.1` |
| Minimax M2.7 | `minimax-coding-plan/MiniMax-M2.7` |
| Kimi K2.5 | `kimi-for-coding/k2p5` |
| Minimax free | `opencode/minimax-m2.5-free` |

Wrong model names produce a JSONL error event in ~1.5s but if stderr is redirected, you get silent 0-byte output.

### 7. Delegate heavy tasks to non-Claude providers

Codex `exec` with `-o` is reliable for "read input, produce output" tasks. Reserve Claude sub-agents for tasks needing local file path reading, MCP tools, or multi-step codebase navigation. This conserves Claude subscription quota.

## Related

- `/Users/thomas/.claude/skills/gemini-cli/SKILL.md` — Gemini CLI skill reference
- `/Users/thomas/.claude/skills/codex-cli/SKILL.md` — Codex CLI skill reference
- `/Users/thomas/.claude/skills/opencode-cli/SKILL.md` — OpenCode CLI skill reference
- `/Users/thomas/.claude/skills/refinery/SKILL.md` — Multi-model refinery skill
