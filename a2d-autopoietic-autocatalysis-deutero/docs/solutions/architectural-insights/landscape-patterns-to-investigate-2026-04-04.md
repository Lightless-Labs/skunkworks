---
module: metabolism, germline, observer, self_sandbox
tags: [landscape, symphony, gas-town, strongdm, schillace, luke-pm, converge-refinery, patterns]
problem_type: design-investigation
---

# Landscape Patterns Worth Investigating or Borrowing

**Sources:** OpenAI Symphony, Gas Town (Yegge), StrongDM Factory/Attractor, Compounding Teams (Schillace), Luke PM, Converge-Refinery
**Context:** A²D has absorbed some patterns from these systems (see `research/01-landscape.md`). This doc focuses on patterns NOT yet implemented that could address current problems — particularly the fitness degradation across cycles and the evolver's ineffectiveness.

## From OpenAI Symphony

### Hot-Reloadable Workflow Definitions
**What it is:** WORKFLOW.md is YAML frontmatter + Liquid-template prompt. Changes hot-reload without restarting the orchestrator — adjust polling intervals, concurrency limits, prompt templates, hooks on the fly.

**Why A²D should care:** The germline is currently serialized JSON, loaded at startup. If the evolver or architect improves an enzyme definition mid-run, the change takes effect next cycle but requires the full run_cycle scheduling loop. Hot-reload would let enzyme prompt changes take effect within the current invocation batch.

**What to borrow:** Make enzyme definitions watchable. When the evolver mutates a prompt_template, the next invocation of that enzyme within the same cycle picks up the change immediately instead of waiting for the next cycle's `sync_germline_artifact()`.

### Continuation Prompts for Long-Running Tasks
**What it is:** After initial full-context prompt, subsequent turns use minimal "keep going from where you left off." Reduces token cost while maintaining continuity.

**Why A²D should care:** The coder currently gets the full prompt every cycle, even when retrying on the same requirements. The failure_report provides context, but the full requirements/design/plan are re-sent redundantly.

**What to borrow:** Detect when the coder is re-running on the same requirements. If failure_report is non-empty and requirements haven't changed, use a continuation prompt: "Your previous attempt failed with these errors: {failure_report}. Fix the errors and resubmit." Instead of the full prompt + failure report appended.

## From Gas Town (Yegge)

Gas Town is well-documented: [github.com/steveyegge/gastown](https://github.com/steveyegge/gastown), Yegge's blog posts, and talks. 7-role architecture (Mayor, Rigs, Polecats, Beads, Refinery, Witness, Deacon, Dogs, Overseer) running 20-30+ parallel Claude Code instances in tmux with git worktrees per agent.

### GUPP Principle ("Gas Town Universal Propulsion Principle")
**What it is:** Each agent has a personal "hook" — a queue of assigned beads. Agents MUST immediately process work on their hook without waiting for confirmation or a complete plan. No blocking, no permission-seeking. Continuous propulsion.

**Why A²D should care:** The metabolism currently fires all ready enzymes in sequence within a cycle. If enzyme A produces an artifact that unblocks enzyme B, B fires in the same cycle but only after A completes. True GUPP would fire B immediately when A's output lands, potentially in parallel with other enzymes.

**What to borrow:** Event-driven scheduling instead of batch scheduling. When an artifact is upserted, immediately check if any enzyme became ready, and fire it. Also: the "no permission-seeking" principle maps directly to the learned helplessness problem documented in `docs/solutions/` — enzymes should act, not ask.

### Beads (Dolt-Backed Institutional Memory)
**What it is:** Not just JSONL files. Beads are atomic work items stored in Dolt — a MySQL-compatible database with git semantics (branch, merge, diff, log on SQL tables). Lifecycle: CREATE → LIVE → CLOSE. SQL-queryable for structured memory. Persistent obligations that survive across sessions. Beads decompose goals into trackable units with provenance.

**Why A²D should care:** The lineage archive is git-backed germline snapshots, but there's no queryable history of cycle reports, fitness trajectories, patch proposals, or failure patterns. The architect operates on a single cycle's failure_report, not on the pattern across 10 cycles. Beads solve exactly this — they give agents institutional memory with query access.

**What to borrow:** Store cycle reports, fitness trajectories, and patch results as structured records in a queryable store (Dolt, SQLite, or even a simple append-only JSONL with grep). Let the architect query: "What failure patterns repeat across the last N cycles?" "Which enzymes consistently produce artifacts that lower fitness?" "What patches were proposed and rejected, and why?" This gives the architect temporal awareness — pattern recognition over time instead of single-cycle reaction.

### Session Handoffs
**What it is:** DB tables (`handoffs` with source/target sessions, consumed_at timestamp) for transferring state and documents between sessions. Compositional for multi-session workflows.

**Why A²D should care:** The metabolism is currently single-session. If the architect modifies the metabolism and the system needs to restart (recompile), the state between sessions is only the germline in the lineage archive + the artifacts on disk. No structured handoff of "what was being attempted, what failed, what the architect was investigating."

**What to borrow:** Persist cycle state to a handoff record before session end. On restart, load the handoff and resume where the cycle left off. This is especially important for the architect — if it proposed a patch that was applied but the fitness impact hasn't been measured yet, the next session needs to know "measure the impact of the last patch" not "start fresh."

### Witness/Deacon Separation (7-Role Architecture)
**What it is:** Specialized roles with structural separation. Witness handles observation/review. Deacon manages execution. The two never collapse into one agent. Part of a broader 7-role system where each role has defined information access and responsibilities. Human remains bottleneck only for PR merge reviews.

**Why A²D should care:** Constitution Invariant 4 (information barriers) is declared but not fully enforced. The evolver can see its own enzyme definitions and evaluate its own mutations. The architect evaluates its own patches (via self-sandbox, which is mechanical, but there's no semantic review). A true Witness/Deacon split would have a separate enzyme (or provider) verify patches independently.

**What to borrow:** After the self-sandbox passes `cargo test`, route the patch to a different model (not the architect's provider) for semantic review. "Does this patch actually address the failure diagnostic? Does it introduce risks that tests don't cover?" This is the inferential control from Fowler's harness engineering, grounded in Gas Town's structural separation. Concretely: architect = Gemini 3 Pro (Deacon), reviewer = Kimi or GLM (Witness). The Witness never writes patches; the Deacon never reviews them.

### Ask Command (RAG Over Transcripts)
**What it is:** `ask` command does RAG over agent transcripts, scoped by session (`--session`, `--sessions last:5`, `--sessions all`). BM25 for fast fragment retrieval, LLM for complex reasoning queries.

**Why A²D should care:** The architect currently sees a snapshot of system code + one cycle's failure report. It has no access to the history of what enzymes said, what the coder attempted, what the evolver proposed. Transcript RAG would let the architect understand not just what failed but how the system behaved across cycles.

**What to investigate:** Would storing enzyme invocation transcripts (the actual LLM output, not just artifacts) and letting the architect RAG over them produce better patches? The risk is information overload; the benefit is behavioral understanding.

## From StrongDM Attractor

Three public NLSpecs: [attractor-spec.md](https://github.com/strongdm/attractor/blob/main/attractor-spec.md), [coding-agent-loop-spec.md](https://github.com/strongdm/attractor/blob/main/coding-agent-loop-spec.md), [unified-llm-spec.md](https://github.com/strongdm/attractor/blob/main/unified-llm-spec.md). Each is a language-agnostic spec designed to be implementable by a coding agent from the spec alone. That's the NLSpec pattern.

### DOT-Based Pipeline Orchestration
**What it is:** Attractor defines multi-stage AI workflows as directed graphs in Graphviz DOT syntax. Nodes are tasks, edges are transitions with conditions. The graph IS the workflow — declarative, visual, version-controllable, diffable in PRs. Pluggable handlers for each node type (LLM call, human gate, parallel fan-out, conditional branch). Checkpoint/resume after each node.

**Why A²D should care:** A²D's enzyme topology is currently hardcoded in Rust structs (`seed_germline()`, `EnzymeDef` in `types.rs`). The evolver can modify it at runtime but there's no human-readable, version-controllable representation of the topology separate from the code. A DOT file would let the architect modify the pipeline declaratively — add/remove/rewire enzymes by editing a graph file, not by producing complete Rust source files.

**What to borrow:** Represent the enzyme topology as a DOT graph. The germline becomes a `.dot` file. The evolver produces `.dot` diffs. The architect can reason about topology visually. `cargo test` validates that the DOT-defined topology achieves RAF closure. This separates topology evolution (graph structure) from behavior evolution (prompt templates) — currently conflated in `EnzymeDef`.

### Coding Agent Loop Spec — Loop Detection
**What it is:** `enable_loop_detection: true`, `loop_detection_window: 10`. The agent tracks consecutive identical tool calls. When the window is exceeded, it injects a steering message to break the loop. Configurable per-session.

**Why A²D should care:** The fitness degradation (100% → 83% → 67%) is a loop. The coder re-generates code that gets progressively worse. The evolver fires 3 times per cycle without producing value. The system detects none of this.

**What to borrow:** Add loop detection to the metabolism. Track artifact hashes across invocations within a cycle. If the same enzyme produces the same (or lower-fitness) output 2+ times, halt it and inject a steering message. If fitness degrades for 2+ consecutive cycles, pause and route the degradation pattern to the architect. The StrongDM spec's `loop_detection_window` is directly applicable — adapt it to enzyme invocations.

### Coding Agent Loop Spec — Steering Queues
**What it is:** `steering_queue: Queue<String>` on each session. Messages injected between tool rounds, before the next LLM call. The host application can redirect the agent mid-task without restarting. Separate from `followup_queue` (messages after current input completes).

**Why A²D should care:** Currently, if the architect detects a problem mid-cycle, it can't intervene until the cycle completes. A steering queue would let the metabolism inject corrective guidance between enzyme invocations: "Stop the evolver, it's producing noise. Focus the coder on the hard sudoku case."

**What to borrow:** Add a `steering_messages: Vec<String>` to the metabolism. After each invocation, check if there are pending steering messages and inject them into the next enzyme's prompt. The architect (or loop detection logic) can push steering messages.

### NLSpec-First (Specification Is the Durable Artifact)
**What it is:** The natural-language specification is the primary artifact. Code is derived, disposable, re-generable. The spec persists; the implementation doesn't. Attractor's own specs ARE NLSpecs — designed to be fed to a coding agent to produce an implementation.

**Why A²D should care:** The challenge `requirements` field is a static string. When the coder fails, the architect can modify the metabolism but nobody improves the specification. If the requirements are ambiguous or incomplete, every coder model will struggle — the bottleneck is spec quality, not code quality.

**What to borrow:** Let the architect (or a new enzyme) refine the requirements based on failure patterns. The spec becomes evolvable, not just the code and the system. Also: could A²D's own architecture be expressed as an NLSpec that the system implements from scratch?

### Checkpoint and Resume
**What it is:** After each node completes, the execution engine saves a serializable checkpoint. Process crashes resume from the last checkpoint, not from scratch.

**Why A²D should care:** A challenge run is expensive (3+ model invocations per cycle × 3 cycles). If the gemini provider times out on cycle 2, the entire run is wasted. Checkpoint/resume would let the system continue from where it left off.

**What to borrow:** Serialize metabolism state (artifacts, lineage, cycle number, fitness) after each cycle. On restart, detect a checkpoint and offer to resume.

### Satisfaction Metrics (LLM-as-Judge, Probabilistic)
**What it is:** Not binary pass/fail. An LLM judges scenario outcomes by asking "Would this satisfy a real user?" Score is probabilistic — drives self-correction feedback loops. This is distinct from mechanical test pass/fail; it evaluates behavioral adequacy.

**Why A²D should care:** The fitness score is mechanical (tests pass/fail), which is good for Constitution Invariant 2 (no self-report). But mechanical tests can't assess "does this sudoku solver feel right to use?" or "is this chess engine's output readable?" Satisfaction fills the gap between "correct" and "good."

**What to borrow:** After mechanical fitness (sandbox tests), run an optional satisfaction evaluation: different LLM (not the coder) reviews the artifact against the requirements and scores satisfaction 0-1. This is an inferential control (Fowler's term). Gate: only use for artifacts that already pass mechanical tests. Don't replace mechanical fitness — layer on top.

**Tension with Constitution:** Invariant 2 says "no agent self-report." Satisfaction metrics ARE agent judgment. Resolution: the judge must be a different model than the producer (Invariant 4, information barriers), and satisfaction scores are advisory, not gating. Mechanical fitness gates; satisfaction informs.

### Digital Twin Universe (DTU)
**What it is:** Behavioral clones of third-party APIs (Okta, Jira, Slack, Google Docs). Replicates APIs, edge cases, failure modes, shared event timelines. Supports rewind/replay/branch/snapshot of the entire "universe." Enables thousands of scenarios/hour without production rate limits or costs.

**Why A²D should care:** The sandbox currently tests code in isolation — `rustc` compile + run tests. For challenges beyond pure algorithms (build a web server, build a CLI that calls APIs), there's no way to test interaction with external systems. DTU would expand what A²D can verify mechanically.

**What to borrow:** Not immediately — current challenges (sudoku, chess, rubiks) are self-contained. But if A²D evolves to produce system software (its stated goal), DTU becomes essential. The pattern: mock the world, not the test.

### Pyramid Summaries (Hierarchical Context Management)
**What it is:** Hierarchical documentation with multiple summary levels. Agents scan short high-level overviews first, drill into details only as needed. Reduces token cost and context overload.

**Why A²D should care:** The architect currently receives ALL modifiable system code as a flat dump. For a small codebase this is fine, but `read_modifiable_files()` will grow. The architect doesn't need line-by-line metabolism.rs to decide "the scheduling logic needs a loop detector" — it needs a summary of what each module does, then detail on the relevant module.

**What to borrow:** Generate pyramid summaries for each system file: one-line purpose, function-level overview, full source. The architect's prompt includes the one-line summaries for all files, but only full source for files relevant to the current failure. This is token-efficient and focus-preserving. Could be generated mechanically (AST extraction of function signatures + doc comments) or by LLM.

### Gene Transfusion (Pattern Extraction and Reuse)
**What it is:** Agents extract architectural patterns ("genes") from existing systems and transplant them into new codebases. Not copy-paste — structural pattern recognition and adaptation.

**Why A²D should care:** If the coder produces 100% fitness in cycle 1, that solution's patterns (constraint propagation, backtracking) could inform subsequent cycles or other challenges. Currently each challenge starts from scratch, each cycle starts from scratch.

**What to borrow:** Store high-fitness artifacts with metadata about what made them work. When starting a new challenge, query: "What patterns from previous 100%-fitness artifacts are relevant?" Inject as a catalyst. The Beads pattern (Gas Town) provides the storage; Gene Transfusion provides the extraction logic.

### Semports (Semantic Porting)
**What it is:** Direct semantic porting of code from one language to another by agents, preserving logic and behavior without manual rewriting.

**Why A²D should care:** Not immediately relevant (Rust-only), but if A²D produces software in multiple languages, semporting proven Rust patterns to Python/Go/TypeScript extends the system's reach. Also relevant if A²D's own codebase ever needs to target a different runtime.

### Holdout Scenarios (Expanded)
**What it is:** End-to-end user stories stored outside the codebase, distinct from unit tests. Agents can't see them, can't game them. LLM-as-judge evaluates whether the implementation would satisfy the scenario's user story.

**Why A²D should care:** A²D already has holdout acceptance tests. But they're code-level tests, not user-story-level scenarios. "User enters a hard sudoku, gets correct solution in under 2 seconds" is a scenario. "assert_eq!(solve(grid), expected)" is a test. The scenario includes UX, performance, and behavioral adequacy — the test only checks correctness.

**What to borrow:** Add scenario-level holdouts alongside code-level acceptance tests. The mechanical tests gate; the scenarios provide richer feedback for the architect and evolver about WHAT to improve.

## From Compounding Teams (Schillace)

### Attention Saturation as Design Constraint
**What it is:** The bottleneck in human+AI teams isn't compute — it's human attention. Every human review point is a queue. Minimize human surface area.

**Why A²D should care:** The Constitution requires human review for certain changes. But if the system generates 10 patches per run, each requiring review, the human becomes the bottleneck. A²D should minimize the number of decisions that require human attention.

**What to borrow:** Risk-score patches (per Fowler's pattern). Low-risk patches (comment changes, formatting, import reordering) auto-apply. Medium-risk patches (logic changes that pass all tests) apply with notification. High-risk patches (structural changes, new control flow) queue for review. The risk score is mechanical (lines changed, functions modified, cyclomatic complexity delta).

### Tooling Self-Improvement Flywheel
**What it is:** Agents improve the tools that agents use. Each improvement compounds — better tools → better output → better tool improvements.

**Why A²D should care:** This is literally what the architect does. But the flywheel isn't yet compounding because the architect has only run once. The question is whether successive architect modifications build on each other or are independent one-shots.

**What to investigate:** Run 10+ cycles. Track whether architect patch N references or builds on architect patch N-1. If patches are independent, the flywheel isn't spinning. If they compound, A²D has achieved what Schillace describes informally.

## From Converge-Refinery

### Controversy Scoring (mean × stddev)
**What it is:** In multi-model brainstorming, score proposals by controversy: `mean_score × stddev_score`. High mean + high disagreement = interesting. Low mean + low disagreement = boring. High disagreement alone = noise.

**Why A²D should care:** The evolver proposes mutations blindly. If multiple models proposed mutations and we scored them by controversy, we'd find mutations that are both promising (high mean) and contentious (high stddev) — exactly the ones worth testing.

**What to borrow:** When the evolver proposes enzyme mutations, dispatch the same prompt to 2-3 models. Score proposals by controversy. Test the most controversial (high mean × high stddev) first. This uses the converge-refinery brainstorm mode directly.

### Score-Only Feedback (No Rationale)
**What it is:** Models see only aggregate scores from previous rounds, not rationale. This prevents herding — models can't converge on a shared narrative and must independently evaluate.

**Why A²D should care:** The evolver currently sees the fitness report ("passed: 5, failed: 1") but also the failure diagnostic. The diagnostic could cause the evolver to over-fit on the specific error rather than addressing the systemic issue.

**What to investigate:** Does the evolver perform better with score-only feedback (just the fitness number) vs. score + diagnostic? Run both and compare.

## Priority Order (What to Build Next)

1. **Loop detection** (StrongDM) — directly addresses the fitness degradation problem. Cheap to implement, high diagnostic value.
2. **Queryable cycle history / Beads** (Gas Town) — gives the architect temporal awareness. Without this, the architect reacts to single-cycle snapshots instead of patterns. Dolt or SQLite.
3. **Session handoffs** (Gas Town) — the architect modifies the metabolism and the system restarts. Without structured handoffs, the next session doesn't know what was attempted.
4. **Continuation prompts** (Symphony) — reduces token cost and noise when retrying on failure.
5. **Witness/Deacon for patches** (Gas Town) — structural separation between the entity that proposes changes and the entity that reviews them. Direct enforcement of Invariant 4.
6. **Confidence intervals on fitness** (StrongDM) — prevents spurious ratchet behavior with small test counts.
7. **Spec evolution** (StrongDM NLSpec) — addresses the possibility that the bottleneck is the requirement, not the code.
8. **Controversy scoring for mutations** (Refinery) — makes the evolver exploratory instead of random.
9. **Transcript RAG** (Gas Town Ask) — lets the architect understand behavioral patterns, not just outcomes.
10. **Event-driven scheduling / GUPP** (Gas Town) — architectural change, high value but high cost.
