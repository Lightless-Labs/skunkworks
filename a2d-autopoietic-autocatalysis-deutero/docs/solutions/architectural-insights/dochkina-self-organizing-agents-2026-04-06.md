---
module: metabolism, germline, escalation
tags: [empirical, self-organization, role-emergence, dochkina, mast-counterpoint]
problem_type: design-validation
---

# Self-Organizing Agents Outperform Designed Structures

**Source:** Dochkina, V. (2026). [Drop the Hierarchy and Roles: How Self-Organizing LLM Agents Outperform Designed Structures](https://arxiv.org/abs/2603.28990). arXiv:2603.28990, March 30 2026.

## What the Paper Shows

25,000-task experiment across 8 models, 4-256 agents, 8 coordination protocols ranging from externally imposed hierarchy to emergent self-organization. Findings:

1. **Sequential protocol outperforms centralized coordination by 14%** (p<0.001, Cohen's d=1.86, p<0.0001). 44% quality spread between protocols.
2. **Agents spontaneously invent specialized roles** without pre-assignment — given minimal structural scaffolding (just fixed ordering), they self-organize.
3. **Agents voluntarily abstain from tasks outside their competence** — emergent humility, no contract required.
4. **5,006 unique roles emerge from just 8 agents** across the experiment.
5. **Sub-linear scaling to 256 agents without quality degradation** (p=0.61).
6. **Open-source models achieve 95% of closed-source quality at 24x lower cost.**
7. **Capability threshold:** strong models self-organize effectively; weaker models still benefit from rigid structure.

## What This Validates About A²D

### Few enzymes, not many
A²D has 4 enzymes. The dominant pattern in the field (Liza's 9 roles, ChatDev's 7+, MetaGPT's many) is more roles, more specialization. Dochkina's 8 agents producing 5,006 unique emergent roles is direct evidence that *fewer fixed roles + emergent specialization > more fixed roles*. A²D is on the right side of this.

### Open-source models are competitive
Kimi k2.5 (coder), GLM 5.1 (evolver) achieving 83% on sudoku is consistent with the paper's "95% of closed-source at 24x cost." The cycle, not the model, is the bottleneck — and the cycle is what A²D fixes.

### Mission + protocol + capable model > role assignment
A²D's enzymes aren't really "roles" in the rigid sense — they're slots in a catalytic graph defined by what they consume and produce. The behavior is still LLM-driven. This is closer to "protocol" than "role assignment." Validated.

### Self-modification is the right axis of evolution
The paper shows emergent specialization happens within a protocol, not by evolving the protocol. A²D's architect evolves the protocol itself (the metabolism). That's a level above what the paper measures, but its findings imply that letting agents do this (rather than human-designed protocol updates) should work.

## The Provocation

> "Give agents a mission, a protocol, and a capable model — not a pre-assigned role."

A²D's enzymes have *names* (coder, tester, evolver, architect). The names imply roles. The names also bias the prompts (the coder's `prompt_template` says "You are a Rust programmer"). Dochkina's evidence suggests this role pre-assignment is suboptimal — agents would specialize better if given a mission and a protocol, not a role.

**What this could mean for A²D:**

A more aggressive design would have N undifferentiated enzyme slots in the catalytic graph, all with the same generic prompt ("here's the protocol, here's the mission, do the next useful thing"). The metabolism's scheduling forces protocol compliance. Specialization emerges from which artifacts each enzyme tends to produce well, not from prompt assignments.

This is provocative because:
- It would break the readable enzyme names that make the system explainable
- It would make the architect's job harder (no targeted prompts to evolve)
- It might not work for weaker models (capability threshold)

It's worth investigating because:
- The paper's effect size is large (Cohen's d=1.86)
- It directly addresses the "evolver produces zero value" problem (the evolver currently mutates fixed-role prompts; emergent specialization would let it mutate a generic protocol prompt)
- It would make the system genuinely more autopoietic — the roles produce themselves

## What Would Be Wrong to Take

### Don't drop the catalytic structure
The paper measures performance on tasks, not on system persistence. A²D's catalytic closure (RAF) is what makes the system self-sustaining over time, not what makes individual tasks succeed. Dropping enzyme topology in favor of pure self-organization would lose that.

The right framing: **the catalytic graph is the protocol; the enzymes within it can self-organize**.

### Don't drop the mechanical verification
The paper's agents self-organize, but the paper doesn't verify *outcomes mechanically*. A²D's sandbox is the oracle that lets self-organization happen safely — without it, emergent roles could converge on confidently-wrong patterns. Mechanical verification + emergent roles is stronger than either alone.

### Don't extrapolate from task quality to system persistence
14% better on a task ≠ 14% better at producing itself. The paper doesn't measure self-modification, deutero-learning, or long-horizon stability. A²D's claims are about persistence, not throughput.

## Counterpoint to Liza/MAST

Liza's 55 failure modes are mapped to 55 prompt-level countermeasures (a behavioral contract). Dochkina's findings suggest most of these failure modes might dissolve with the right protocol — agents that can voluntarily abstain don't need a "Struggle Protocol" rule because they just don't fake competence in the first place.

The MAST taxonomy is still useful as a checklist. But the implication is: **structural changes (protocol design) might prevent more failures than rule-based countermeasures (contract clauses).**

## What to Try

### Experiment 1: Generic enzyme prompts
Replace the coder's `prompt_template` with a generic "you are an enzyme in a self-producing system; here's the protocol; do the next useful thing." Measure if fitness changes. Hypothesis: fitness stays similar or improves on capable models, drops on weaker models (capability threshold).

### Experiment 2: Enzyme slot count
Add 2 unnamed enzyme slots to the germline (e.g., "worker_a", "worker_b") with generic prompts. Let the architect evolve them. See if specialization emerges or if they remain generic. Hypothesis: specialization emerges if and only if the architect has fitness signal to differentiate them.

### Experiment 3: Sequential protocol comparison
Switch the metabolism's scheduling from "fire all ready enzymes in parallel-ish order" to "strict sequential" — coder → tester → evolver → architect → coder → ... — and measure. Hypothesis: sequential is competitive or better, with simpler scheduling.

## Priority

This goes after the escalation ladder. The escalation ladder fixes the immediate degradation problem (rung 0 = loop detection). Self-organization experiments are the next level — they explore whether A²D's enzyme structure should change. Get the system stable before redesigning it.

But the experiments should be cheap (mock-based, like the existing tests) and worth running before any aggressive redesign of enzyme topology.

## Sources

- [Dochkina 2026, arXiv:2603.28990](https://arxiv.org/abs/2603.28990)
- [HTML version](https://arxiv.org/html/2603.28990)
- [alphaXiv overview](https://www.alphaxiv.org/overview/2603.28990)
