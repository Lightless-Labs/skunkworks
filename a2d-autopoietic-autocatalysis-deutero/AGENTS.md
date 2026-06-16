# A²D — Autopoietic Autocatalysis Deutero

A parallel experiment to A². What emerges when a self-producing system design has access to empirical data about how agents actually behave at scale?

## Session Protocol

1. **Read `docs/HANDOFF.md` first** — current state, score card, what works/doesn't, what to do next
2. Run `cargo test` to verify everything passes before changing anything
3. **Update `docs/HANDOFF.md`** before context compaction or session end

## Lineage

- **A²** (`~/Projects/lightless-labs/skunkworks/a2-autopoietic-autocatalysis/`) — Theoretical foundations: autopoiesis, autocatalytic sets (RAF theory), closure to efficient causation, von Neumann self-reproduction. Two architectural proposals, zero implementation.
- **Agentic Engineering Sawdust** (`~/Projects/Banade-a-Bonnot/agentic-engineering-sawdust/`) — 253 empirical learnings extracted from 4.5GB of real agent sessions. Multi-model refinery consensus. Key insight: systems engineering > prompt engineering.
- **Third Thoughts** (`~/Projects/lightless-labs/third-thoughts/`) — Analytical backbone. Middens (Rust CLI) + 26 Python analysis scripts. 23 cross-disciplinary statistical techniques. Rigorous methodology.
- **Foundry** (`~/Projects/lightless-labs/public/foundry/`) — Adversarial blind process for multi-agent verification. Red/green teams with structural information barriers (typed contexts, filesystem isolation, outcome filtering). The immune system primitive A² theorized but didn't design.

## What A²D Is

An emergence space. Not a sequel to A², but a sibling experiment running in parallel with a richer food set: A²'s theory + Sawdust/Third Thoughts' empirics + distributed Middens data.

"Deutero" (Bateson): learning to learn. A system that learns how agentic systems learn.

## Positioning

Level 6 on Shapiro's maturity scale (beyond Dark Factory → self-producing factory).
Every existing system (Symphony, Gas Town, StrongDM/Attractor) tops out at Level 5.
A²D adds: catalytic closure, deutero-learning, mechanical verification, the 85% correction factor.

## Additional Lineage

- **Converge-Refinery** (`~/Projects/Banade-a-Bonnot/converge-refinery/`) — Multi-model convergence engine. Three modes: converge (Lamarckian), brainstorm (controversy scoring), evolve (Darwinian). Use for key design decisions, not single-pass Claude.
- External references: OpenAI Symphony, Gas Town (Yegge), StrongDM Factory/Attractor, Compounding Teams (Schillace). See `research/01-landscape.md`.

## Design Principles

- **Outcomes, not correctness.** A²D converges on outcomes (does the artifact work?), not correctness (is the code right?). The sandbox is the oracle; models are the search strategy. See `docs/solutions/architectural-insights/outcomes-not-correctness-2026-04-04.md`.
- **No human in the loop.** Degradation triggers mechanical escalation (clean session → model swap → multi-model proposals → Darwinian isolation), not human intervention. See `todos/escalation-ladder.md`.
- **No unnecessary dependencies.** Borrow patterns from refinery, Gas Town, StrongDM, etc. but implement directly. SQLite or append-only JSONL for persistence, not Dolt. Refinery patterns in the metabolism, not as a crate dependency.
- **Tests are part of self-modification.** Tests are not protected physics. When the architect changes production semantics, it must evolve the internal tests that encode those semantics in the same atomic `SystemPatch` batch; the self-sandbox validates the combined state, while hidden challenge holdouts remain the behavioral backstop.

## Process

- **Delegate to other providers** for heavy tasks and design decisions. Codex, Gemini, OpenCode (GLM 5.1/5.2, Minimax 2.7/3, Kimi k2.5/k2.7) are available.
- **Use `/refinery`** or independent multi-model proposals for architectural choices. Single-model design is the monoculture this project warns against.
- **Run `/ce:compound`** after significant findings or implementation milestones.

## Technical Stack

- **Language**: Rust (2024 edition)
- **Build**: Bazel (MODULE.bazel + rules_rust), Cargo for local dev
- Follows Lightless Labs conventions (see parent CLAUDE.md)

## Benchmark Methodology

The system's value proposition: produce functional software more reliably than a single model, with fewer iterations and less human feedback.

### What "better" means

Not fitness scores. Not "does it compile." **Does it do what it's supposed to do?** The artifact works when given to something that doesn't know how it was made.

### Acceptance tests

Hidden holdout tests appended to the coder's output before sandbox compilation. The coder never sees them. They verify the artifact actually works — feed a real puzzle, check the answer.

Design rules:
- One acceptance test is not testing; use diverse inputs, edge cases, and negative cases
- Tests must be **API-agnostic** — verify behavior, not implementation details. The chess acceptance test assumed rank 1 = white pawns, but Codex used rank 6. Fixed to: "white has >= 16 legal moves from start" which is orientation-agnostic.
- See `docs/solutions/best-practices/acceptance-test-coverage-*.md`

### Baseline comparison

Always compare against a single frontier model one-shot on the same task with the same acceptance tests. If the cycle can't beat that, it's overhead.

Current baselines (2026-04-04):
- **Gemini 3 Pro**: sudoku 100% (7/7), chess 100% (9/9) — one shot
- **Codex gpt-5.4**: sudoku 100% (6/6), chess 100% (11/11) — one shot

### Current state (2026-04-04)

A²D produces 83% (5/6) on sudoku regardless of coder model (Codex or Kimi k2.5). The bottleneck is in the cycle, not the model. Two critical architectural findings:

1. **The evolver produces no value.** Across all runs, zero mutations improved fitness. It modifies enzyme definitions blindly without knowing which test failed or why. Pure overhead.
2. **The feedback loop is broken.** The coder never sees why it failed. Acceptance test failures and compilation errors go to the fitness score, not back to the coder. The thing that can fix the problem never learns what the problem is. The missing edge: sandbox failure output → coder input in subsequent cycles.

See `docs/solutions/architectural-insights/` for full analysis.

### Challenges

Run via `a2d challenge <name> <cycles>`. Available: chess, sudoku, rubiks. Each has requirements, string checks, sandbox compilation, and acceptance tests. See `crates/a2d-core/src/challenges.rs`.

### Examples

Documented runs with configuration, per-cycle results, baseline comparison, artifacts, and learnings in `examples/runs/`.

## Provider Configuration

### Model IDs (OpenCode)

OpenCode model IDs are not intuitive. Always verify with `opencode models` when possible; some newly available lanes may not list immediately:
- Kimi k2.5: `kimi-for-coding/k2p5`
- Kimi k2.7 code: `kimi-k2.7-code` (A²D provider name: `opencode/kimi-k2.7-code`)
- GLM 5.1: `zai-coding-plan/glm-5.1`
- GLM 5.2: `zai-coding-plan/glm-5.2` (A²D provider name: `opencode/zai-coding-plan/glm-5.2`)
- Minimax M2.7: `minimax-coding-plan/MiniMax-M2.7`
- Minimax 3: exact OpenCode alias may vary; A²D currently recognizes opt-in provisional aliases `opencode/minimax-coding-plan/MiniMax-3`, `opencode/minimax-coding-plan/Minimax-3`, and `opencode/minimax-coding-plan/MiniMax-M3`

A²D keeps Kimi k2.7 / GLM 5.2 / Minimax 3 lanes opt-in: they are registered when named by runtime overrides, loaded/provider-comparison policies, or direct role-provider comparisons, not added to the default coder portfolio. Verified Pi-backed opt-in provider names are also available for experiments without changing defaults: `pi/kimi-coding/k2p7`, `pi/minimax/MiniMax-M3`, and `pi/zai/glm-5.2`.

### Provider resilience

CLI providers have bounded subprocess timeouts. GLM 5.x gets a 15-minute default window; other CLI providers default to 5 minutes. OpenCode uses `--format json` with NDJSON parsing to avoid ANSI escape code contamination. Failures are reported, not silently swallowed.

## Key Directories

- `research/` — Synthesis documents, collision analyses, landscape research
- `docs/plans/` — Implementation plans and NLSpecs
- `docs/solutions/` — Documented learnings organized by category with YAML frontmatter (`module`, `tags`, `problem_type`). Search before implementing or debugging in documented areas.
- `todos/` — Pending work items
- `crates/a2d-core/` — Core library: types, raf, observer, germline, workcell, provider, sandbox, benchmark, challenges
- `crates/a2d-providers/` — CLI provider implementations (Codex, Gemini, OpenCode)
- `crates/a2d-cli/` — CLI binary: cycle, challenge, status, enzymes, lineage
- `examples/runs/` — Documented challenge runs with results and learnings
- `CONSTITUTION.md` — 6 organizational invariants (immutable by automated actors)
