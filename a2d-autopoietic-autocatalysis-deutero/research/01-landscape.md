# 01 — Software Factory Landscape: Where A²D Sits

**Created:** 2026-04-01
**Sources:** OpenAI Symphony, Gas Town (Yegge), StrongDM Factory/Attractor, Luke PM, Sam Schillace (Microsoft), Dan Shapiro

---

## The Maturity Scale (Shapiro's Levels + Level 6)

| Level | Metaphor | Human Role | Example Systems |
|-------|----------|------------|-----------------|
| 0-2 | Spicy autocomplete → Junior dev | Coder | Copilot, Cursor |
| 3 | Waymo with safety driver | Reviewer | Claude Code, Codex |
| 4 | Robotaxi | Spec writer / PM | Symphony, Gas Town |
| 5 | Dark Factory | Vision setter | StrongDM Attractor |
| **6** | **Self-producing factory** | **Constitution author** | **A²D (goal)** |

Every existing system tops out at Level 5. A²D adds:
- Catalytic closure (the factory sustains itself)
- Deutero-learning (the factory learns how to learn)
- Mechanical verification (the factory proves its own correctness)

---

## System Inventory

### OpenAI Symphony (Level 4)
- Elixir/BEAM, issue-to-PR orchestrator, Codex-only
- **Relevant patterns:** Workspace isolation (→ workcells), WORKFLOW.md config-as-code (→ germline), proof-of-work verification (→ Invariant 2), exponential backoff retry
- **Gap:** No self-improvement, no cross-model verification, no catalytic closure

### Gas Town / Steve Yegge (Level 4)
- Go, 20-30+ parallel Claude Code agents, 17-day proof-of-concept
- **Relevant patterns:** Beads (git-backed JSONL + SQLite external memory → germline/lineage), topological task dispatch (→ metabolism scheduling), GUPP principle ("execute immediately on available work" → enzyme activation), Witness/Deacon separation (→ information barriers), git worktrees per agent (→ workcells)
- **Gap:** No formal verification (Witness is LLM-as-judge), no self-improvement, no multi-model

### StrongDM Factory / Attractor (Level 5)
- 3 engineers producing enterprise security software, $1000+/day in tokens
- **Relevant patterns:** Holdout scenarios as external validation (agents can't see their own tests → Invariant 4), satisfaction as probabilistic metric (→ beyond binary fitness), Digital Twin Universe for hermetic testing, NLSpec-first (specification IS the durable artifact → germline), Gene Transfusion (reusing proven patterns → catalytic motifs), Pyramid Summaries for context management, coding agent loop spec (steering queues, loop detection → metabolism)
- **Gap:** Factory doesn't produce itself. Autopoietic factory ≠ dark factory.

### Compounding Teams / Schillace (Empirical)
- 2-3 elite Microsoft teams, 2-3 humans directing AI swarms = 30+ person output
- **Relevant insight:** Attention saturation — human bandwidth is the bottleneck, not compute. A²D must minimize human attention requirements.
- **Relevant insight:** Self-improvement of tooling (agents improve the tools agents use) is primitive autocatalysis, not yet formalized.

### Luke PM (Synthesis)
- Architecture-code matching as CI gate (→ mechanical verification)
- State separation: current / target / questions (→ germline state model)

---

## What the Field Converges On

1. **Specification > Implementation** — bottleneck shifts to spec quality at every level
2. **Externalized state** — Beads, WORKFLOW.md, NLSpecs. The durable artifact is the spec, not the code
3. **Ephemeral execution** — workcells/polecats/sandboxes are universally disposable
4. **Mechanical verification** — proof-of-work, holdout scenarios, CI gates. Reject self-reports.
5. **Git as coordination substrate** — worktrees, git-backed state, git as audit trail

## What A²D Uniquely Contributes

1. **Catalytic Closure** — No existing system checks whether it can sustain itself. RAF detection is novel.
2. **Self-production** — Every system produces SOFTWARE. A²D produces ITSELF.
3. **Deutero-learning** — No system learns how to learn. Compounding is ad-hoc; A²D formalizes it.
4. **Structural information barriers** — Gas Town's Witness is a step; Foundry's typed contexts are rigorous.
5. **Constitutional invariants** — No system has immutable laws agents cannot modify.
6. **The 85% correction factor** — No system accounts for the quantified reliability degradation of agent self-report.

## Techniques to Absorb

| Source | Technique | A²D Module |
|--------|-----------|------------|
| Symphony | Workspace isolation, proof-of-work | Workcell, Invariant 2 |
| Gas Town | Beads (git/JSONL/SQLite), GUPP, topological dispatch | Germline, Metabolism |
| StrongDM | Holdout scenarios, probabilistic satisfaction, DTU, NLSpec-first | Observer, Germline |
| StrongDM | Coding agent loop spec (steering queues, loop detection) | Metabolism |
| Gas Town | Witness/Deacon separation | Observer, Information barriers |
| Schillace | Attention saturation as design constraint | Metabolism (minimize human surface) |
| Shapiro | Level 6 positioning | A²D's unique contribution |
| Luke PM | Architecture-code matching as CI | Mechanical verification |
| Converge-refinery | Controversy scoring (mean × stddev), Darwinian mode | Mutation generation |
| Yegge ZFC | Transport makes no judgment calls; intelligence is separate | Metabolism = transport, enzymes = intelligence |
| Converge-refinery | Controversy scoring (mean × stddev), Darwinian mode | Mutation generation |
| Converge-refinery | Score-only feedback (no rationale) | Anti-monoculture |
| Converge-refinery | Self-evaluation exclusion | Invariant 4 |
