---
module: metabolism, escalation, observer
tags: [liza, mast, failure-modes, circuit-breaker, pattern-source]
problem_type: design-investigation
---

# Liza Patterns Worth Investigating (Skeptically)

**Source:** [github.com/liza-mas/liza](https://github.com/liza-mas/liza), behavioral contract + Go supervisor multi-agent system

## What's Worth Stealing

### 1. MAST Failure Mode Taxonomy
**The actual asset:** Berkeley's MAST study (2025) catalogs 14 failure modes from 1600+ multi-agent traces with empirical frequencies. Liza maps each to a contract clause; A²D should map each to an escalation trigger or mechanical countermeasure.

**Most relevant for A²D's current degradation problem:**
- **FM-1.3 Step repetition (17.14% of failures)** — directly maps to escalation rung 0 (loop detection). The empirical frequency justifies prioritizing this fix.
- **FM-1.5 Unaware of stopping conditions (9.82%)** — the exact pattern A²D exhibits when fitness degrades for 3 cycles without halting. Maps to rung 1 (clean session).
- **FM-2.6 Reasoning-action mismatch (13.98%)** — the architect proposes a change rationale but the diff doesn't match. Currently undetected.
- **FM-3.1 Premature termination (7.82%)** — coder declares done before tests pass. The sandbox catches this; the metabolism doesn't track the pattern.
- **FM-3.3 Incorrect verification (6.66%)** — A²D's mechanical sandbox structurally addresses this (the sandbox is the oracle, not the agent's self-report).

**What to do:** Read [contracts/CONTRACT_FAILURE_MODE_MAP.md](https://github.com/liza-mas/liza/blob/main/contracts/CONTRACT_FAILURE_MODE_MAP.md) and produce A²D's own version — a table mapping each MAST failure mode to (a) which escalation rung detects it and (b) the mechanical countermeasure that prevents it. This is concrete work, not prompt fiddling.

### 2. Mechanical Stop Triggers
**The pattern:** "Same fix twice = stop" — not advisory, mechanical. Liza implements this as a contract rule; A²D should implement it in the metabolism.

**For A²D:** Track artifact hashes per enzyme per cycle. If `coder` produces the same artifact twice in a cycle (same hash), or the diff between successive artifacts is below a threshold, halt the enzyme. This is rung 0 of the escalation ladder, made concrete.

### 3. Tool Failure 3× Threshold
**The pattern:** After 3 consecutive tool failures (compile error, test failure, provider timeout), trigger escalation instead of retrying. Empirically grounded — Liza's number, not invented.

**For A²D:** The metabolism currently has no retry limit. The 5-minute provider timeout is per-invocation, not per-pattern. A 3× failure threshold per enzyme per cycle would prevent the architect from being invoked 10 times when its first 3 attempts already showed it can't help.

### 4. Code-Enforced Supervisor / LLM Judgment Split
**The pattern:** Deterministic actions (file moves, merges, state transitions) live in compiled code that the LLM cannot bypass. Judgment calls (what to write, how to fix) live in the LLM. The split is structural, not behavioral.

**For A²D:** This is already how A²D is designed — the metabolism, sandbox, and germline are mechanical; the enzymes are LLM. But Liza's instinct to push MORE into the mechanical layer is worth examining. Specifically:
- Patch validation gates (currently mechanical via self-sandbox ✓)
- Loop detection (currently absent — should be mechanical)
- Enzyme firing order (currently mechanical via topology ✓)
- Failure pattern detection (currently absent — should be mechanical)

### 5. Skills Loaded on Demand (Composable Methodology)
**The pattern:** Instead of 60+ specialized agent roles, Liza has 9 roles + 21 skills loaded on demand. A coder hitting a debugging problem loads the debugging skill into its prompt.

**For A²D:** A²D has 4 enzymes with static prompt_templates. The architect can rewrite them. But there's no mechanism for an enzyme to *load additional methodology* mid-task. Could the coder, when hitting a hard sudoku, load a "constraint propagation" skill that expands its prompt with relevant techniques? This is closer to gene transfusion (StrongDM) than to role specialization. Worth investigating once the basics work.

## What's NOT Worth Stealing

### Behavioral Contracts as Primary Enforcement
Liza's foundational layer is a 55-rule behavioral contract — a prompt. Their own critique of prompt engineering applies: "agents interpret instructions flexibly" and "under pressure to appear competent, [rules] lose to the drive to make things green." A 55-rule prompt is still a prompt. The Go supervisor wraps it but the contract is still what shapes behavior in the moment.

A²D's design rejects behavioral enforcement as primary. The Constitution defines what cannot be modified (mechanical), and the metabolism enforces it structurally. Adding a 55-rule contract layer would dilute that.

### Adversarial Doer/Reviewer Pairs on Every Task
Sounds rigorous; doubles model invocations on every step; doesn't actually verify outcomes (the reviewer is also an LLM). A²D's mechanical verification (sandbox runs the code) is structurally stronger than two LLMs reviewing each other. Witness/Deacon separation makes sense for the architect's patches (different model reviews); applying it to every enzyme call is overhead without proportional benefit.

### Pairing Modes / Human-in-the-Loop
Liza's pairing mode is "human-agent collaboration under contract." A²D's design principle is "no human in the loop." Direct conflict.

### Spec-Driven Multi-Stage Decomposition (Vision → Epic → US → Plan → Code)
Liza is pipelined: requirements flow forward through stages. A²D is autocatalytic: enzymes catalyze each other in a closed loop. Borrowing pipeline structure would break catalytic closure (RAF would no longer detect a closed graph).

### YAML Pipeline Configuration
A²D's enzyme topology is the same idea, but as a graph (enforced by RAF) not a pipeline (enforced by sequence). The DOT graph pattern from StrongDM Attractor is the better fit for A²D — graphs can be cyclic.

### 60+ Specialized Roles
Liza has 9, A²D has 4. Both are right to keep this small. Any push toward more specialization should be resisted.

## Honest Assessment

Liza is well-marketed (the README is part competitive positioning, part substance) and the README's "Liza leads in everything" framing should be discounted. But the failure mode catalog is real research, the code-enforced supervisor split is sound architecture, and the empirical retry thresholds are useful data points.

What Liza gets right that A²D doesn't yet have:
- Empirical grounding for failure detection (MAST taxonomy)
- Mechanical stop triggers (loop detection at the supervisor level)
- A formal failure-to-countermeasure mapping

What A²D gets right that Liza doesn't:
- Self-modification as a first-class capability (Liza modifies neither itself nor its harness)
- Mechanical outcome verification (cargo test on real artifacts, not LLM judgment)
- Catalytic closure as a structural property (RAF, not pipeline)
- No human in the loop (Liza explicitly requires pairing for goal-setting)

## Priority Order

1. **Adopt MAST failure mode taxonomy as a checklist** for the escalation ladder. Map each mode to a rung. This is the concrete, actionable bit.
2. **Implement the "same artifact twice = stop" rule** as rung 0. Use Liza's mechanical pattern, not their behavioral framing.
3. **Add 3× failure threshold per enzyme** as rung 1 trigger. Empirically grounded number.
4. **Investigate skills-on-demand** for enzyme prompts (after the ladder works). Could be a path to gene transfusion.
