# Cross-Evaluation of A² Architecture Proposals

**Evaluator:** Claude Opus 4.6
**Date:** 2026-04-01
**Inputs:** Claude proposal, Gemini proposal, Codex proposal, foundational research (01-04)

---

## 1. Claude Proposal: "The Metabolic Compiler"

### 1.1 Theoretical Grounding

**Rating: Strong, with one significant inflation**

The Claude proposal is the most theoretically detailed of the three. It correctly identifies RAF theory as constructive (polynomial-time detection algorithm) and autopoiesis as a recognition criterion rather than a construction manual. The Rosen (M,R)-system mapping is explicit: metabolism = f, evolver = Phi, cortex = beta, and the closed causal loop f -> beta -> Phi -> f is named and traced through concrete components. The three-level closure hierarchy (catalytic closure, organizational closure, closure to efficient causation) is precisely the hierarchy established in the research (Cornish-Bowden & Cardenas 2020). The von Neumann constructor/description duality is correctly applied to the genome's dual role (interpreted by the ribosome, copied by git).

The RAF formalization in Section 3.2 is concrete: X, R, C, F are defined with specific artifact types, and the catalysis relation is given operational meaning (Section 3.3). The distinction between catalysis and mere input is a genuine theoretical contribution that goes beyond the research summaries.

**The inflation:** The proposal claims "closure to efficient causation" as an aspired property and then proceeds to describe it as though it is architecturally achievable through the evolver-improving-evolver loop. But the proposal's own Section 10 ("The Hard Problem") correctly identifies three fundamental obstacles (Vingean reflection, evaluation recursion, coherence maintenance) that make this closure deeply problematic. The architecture *describes* closure to efficient causation but does not *solve* it. The honest conclusion in Section 10 -- that full organizational autonomy may be asymptotic -- partially redeems this, but the proposal still oversells the cortex and evolver as achieving something the analysis itself shows they cannot reliably do.

### 1.2 Architectural Concreteness

**Rating: High -- the most buildable of the three**

The proposal provides a Rust trait definition for the Enzyme interface, an ASCII data flow diagram, a tech stack table with rationale, and a detailed RAF detection loop algorithm. The seven named components (genome, ribosome, membrane, enzymes, metabolism, immune system, cortex) each have specific, implementable responsibilities. The Bazel dependency graph as the machine-readable reaction graph is an insight with real engineering value.

The bootstrap sequence (Stages 0-3) is credible: hand-written seed, first self-modification of a prompt template, RAF closure check, and regeneration-from-scratch as the fixed-point test. The minimal irrRAF suggestion (3-4 enzymes in a tight cycle) is grounded in the RAF theory result that linear catalytic connectivity suffices.

**Gap:** The proposal never specifies how the evolver actually works. It says the evolver "reads enzyme performance data and produces improved enzyme definitions," but this is the entire hard problem compressed into a single sentence. What does the evolver's prompt look like? How does it decide what to change? How does it avoid the degenerate attractor of trivially gaming its own evaluation? The coder, tester, and reviewer enzymes are straightforward (they map to well-understood agentic SE patterns from the research). The evolver is where all the novelty lives, and it is the least specified component.

### 1.3 Autocatalytic Closure

**Rating: Partial -- honest about the gap but does not close it**

The inner loop (code production) is explicitly acknowledged as not autocatalytic. The self-production loop (evolver -> improved enzymes -> better performance data -> evolver) is genuinely autocatalytic *if* it works. The meta-loop (cortex -> evolver -> improved cortex -> better bottleneck identification) is where closure to efficient causation would live.

The proposal correctly identifies that the food set (base models, compute, human objectives) is never produced by the system. This is honest. But it then somewhat glosses over the degree to which the system depends on LLM capabilities that it cannot improve or replace. The system can improve its prompts, its orchestration, its evaluation criteria -- but it cannot improve the underlying models. If model capability is the binding constraint on enzyme quality (and the research strongly suggests it is -- scaffold matters, but model capability is the floor), then the system's self-improvement has a ceiling determined by an external factor it cannot influence.

The genome-as-component argument is sound: the genome is both produced (every commit is an enzyme output) and consumed (the ribosome reads it). This is genuine constitutive circularity for the artifact layer.

**Where closure breaks:** The proposal smuggles in human intervention at Level 5 of the fitness signal stack ("Initially, this level requires human judgment") and in the organizational invariants ("human-specified and frozen"). These are load-bearing concessions. Without human judgment at the meta-level, there is no ground truth for whether the system is actually improving versus Goodharting. The proposal acknowledges this but does not grapple with the implication: A² is not fully autopoietic. It is a human-supervised autocatalytic system with aspirations toward organizational closure.

### 1.4 Novelty

- The RAF detection loop as a runtime health metric (Section 8.4) -- computing catalytic closure coverage as an operational signal, not just a theoretical property. No other proposal does this.
- The explicit distinction between catalysis and mere input (Section 3.3), grounding the RAF formalism in concrete software engineering operations.
- The three-layer self-production model (artifact, component, process) mapping to allopoietic, autopoietic, and Rosennean closure respectively.
- The organizational invariants as the computational analogue of autopoietic organization (Section 6.3) -- catalytic closure invariant, boundary integrity invariant, closure-to-efficient-causation invariant, performance monotonicity invariant.
- The comparison table (Appendix) positioning A² against Karpathy autoresearch, AlphaEvolve, and Darwin Godel Machine.

### 1.5 Weaknesses

1. **The evolver is a black box.** The most critical component in the entire architecture -- the one that makes it autocatalytic rather than merely agentic -- is the least specified. "Reads performance data, produces improved enzyme definitions" is a wish, not a design.

2. **Overlong and somewhat self-congratulatory.** At 553 lines, the proposal is nearly 7x the length of the Gemini proposal and over 2x the Codex proposal. Much of this length is devoted to theoretical exposition that, while accurate, sometimes reads as demonstrating knowledge rather than applying it. The comparison table in the Appendix is especially suspect: it conveniently gives A² a "Yes" on every property where competitors get "No."

3. **The membrane is underspecified for a supposedly autopoietic system.** The proposal claims the membrane is "self-produced" and "not a static configuration imposed by an operator," but the concrete description (ingress policies, egress policies, access controls, API surface) reads like standard infrastructure-as-code. What makes A²'s membrane self-produced in a way that a well-configured Kubernetes cluster is not? The proposal does not answer this convincingly.

4. **Model dependency is underexamined.** The food set includes "base models (Claude/Gemini/Codex)" but the proposal never seriously considers what happens when these models change, degrade, or become unavailable. The system's entire autocatalytic capacity depends on external model APIs it does not control.

5. **The cortex is aspirational.** Maintaining a "map of the current RAF structure" and doing "bottleneck identification" and "performance attribution" requires significant reasoning capability that is hand-waved. How large is the RAF graph for a real system? Can an LLM actually reason over it effectively? The proposal assumes yes without evidence.

6. **No cost analysis.** The proposal specifies model routing by complexity tier but provides no estimate of what it costs to run a single autocatalytic cycle. If each cycle requires dozens of LLM invocations (architect + coder + tester + reviewer + evolver + cortex + immune system), the cost per improvement may be prohibitive.

---

## 2. Gemini Proposal: "The Digital RAF Set"

### 2.1 Theoretical Grounding

**Rating: Adequate but shallow**

The proposal correctly names RAF theory and Rosen (M,R) systems and provides the CRS tuple (X, R, C, F). The nested closure model (metabolic loop f, repair loop Phi_f, replication loop beta_b) directly mirrors Rosen's formalism. The mapping of molecules to code modules, reactions to build processes, and catalysts to agentic personas is stated clearly.

However, the theoretical engagement is surface-level. The proposal asserts "RAF theory because its hierarchical structure (irrRAFs) allows for modular evolvability" but never explains *how* irrRAFs would be identified or used in the system. The autopoiesis connection is limited to a single sentence in Section 6 about rejecting perturbations that break catalytic closure. There is no engagement with the research on where the biological analogy breaks (the physicality problem, the boundary problem, the closure problem from research doc 01). The von Neumann duality is never mentioned.

The opening line -- "I will read the remaining research files to ensure the architecture proposal is grounded in the full breadth of the provided foundational research" -- is literally a leaked chain-of-thought artifact, which undermines confidence in the rigor of the rest.

### 2.2 Architectural Concreteness

**Rating: Low -- too abstract to build**

The proposal is 77 lines long. The implementation sketch (Section 8) consists of four bullet points: "Rust (Actix-based agent actor system) + Bazel," "MCP (Model Context Protocol)," "Gemini 1.5 Pro and Claude 3.5 Sonnet," and "a persistent Vector-embedded Knowledge Graph." There is no data flow diagram, no interface definition, no bootstrap sequence detail, no crate structure, no protocol specification.

The component list in Section 2 is essentially the RAF tuple restated in words. "Substrate (X)" is "Rust source code, Bazel build rules, MCP server definitions." This tells you the types of artifacts but nothing about how they are organized, stored, accessed, or transformed.

The bootstrap sequence (Section 4) uses biological metaphors (Egg, Larva, Imago) that are evocative but not informative. Stage 0 is "a minimal Python script that utilizes the Food Set (LLM APIs) to generate the Universal Constructor in Rust." This is not a design -- it is a hope. How does a Python script generate a Rust universal constructor? What does the constructor's interface look like? What subset of the system does it build first?

The "Vector-embedded Knowledge Graph" in Section 8 is the most concrete novel element, but it is not explained. What is in the graph? How is it queried? How does it relate to the RAF structure?

### 2.3 Autocatalytic Closure

**Rating: Claimed but not demonstrated**

The proposal claims "every catalyst in A² is a product of a reaction catalyzed by another member of A²" and gives one example: "the Architect catalyst produces the Linter catalyst's source code, while the Linter catalyzes the Architect's refinement." This is a plausible sketch of mutual catalysis, but it is a single example, not a systematic analysis. The proposal does not enumerate the catalytic relationships, does not describe how closure would be verified, and does not identify which components might be orphaned.

The closure condition in Section 3 (nested closure across three loops) is structurally correct per Rosen, but the proposal provides no mechanism for *detecting* whether closure holds at runtime. Compare this to the Claude proposal's RAF detection loop, which provides an algorithm for exactly this purpose.

The proposal's "Bootstrap Test" in Section 7 ("Can the system reconstruct its current version from the Food Set?") is a strong criterion for closure, but it is stated as a fitness signal rather than as something the system actually performs.

### 2.4 Novelty

- **MCP as intercellular communication.** Using Model Context Protocol as the "cytoplasm" through which components interact is a concrete design choice that neither other proposal makes. MCP provides a standard protocol for tool invocation, which could genuinely serve as the substrate for catalytic interactions.
- **Vector-embedded Knowledge Graph.** The idea of maintaining the system's organizational self-model as a vector-embedded graph is different from both the Claude proposal's SQLite-based cortex and the Codex proposal's filesystem-based lineage archive. It is underspecified but potentially powerful for similarity-based retrieval of past organizational states.
- **The "Model Collapse" failure mode** (Section 9): the system becoming an allopoietic extension of a single LLM rather than an autonomous unity. This is a genuinely important risk that neither other proposal identifies in these terms.

### 2.5 Weaknesses

1. **Critically underspecified.** At 77 lines, this is less an architecture proposal and more an architecture sketch. Almost every section needs 3-5x more detail to be actionable.

2. **Leaked chain-of-thought.** The opening line is a process artifact, not content. This suggests the proposal was generated in a single pass without revision.

3. **Outdated model references.** "Gemini 1.5 Pro" and "Claude 3.5 Sonnet" are several generations behind current capabilities. This is a minor point but suggests the proposal was not grounded in the state-of-the-art research (which references Claude Opus 4.5/4.6, Gemini 3 Pro, GPT-5.x).

4. **No verification mechanism.** The proposal describes fitness signals (self-producibility, structural integrity, catalytic efficiency) but provides no mechanism for computing them, no gate structure, no rollback mechanism, no immune system equivalent.

5. **The "Causal Transparency" hard problem (Section 10) is named but not addressed.** The proposal states that agents must "understand the causal reason why code exists within the system's own identity" but provides no mechanism for achieving this understanding.

6. **Python bootstrap into Rust is unjustified.** Starting with a Python seed script that generates Rust code via LLM is the least reliable possible bootstrap strategy. The Claude and Codex proposals both assume a hand-written Rust seed, which is far more credible.

---

## 3. Codex Proposal: "Lineage-Governed Colony of Simple Workcells"

### 3.1 Theoretical Grounding

**Rating: Pragmatically strong, theoretically selective**

The Codex proposal makes the most disciplined theoretical choices. It explicitly ranks RAF theory first, autopoiesis second (for organization/identity), and von Neumann third (for constructor/description duality). The justification for each choice is stated in one sentence and is correct. There is no unnecessary theoretical exposition.

The proposal engages honestly with the limits of the autopoietic analogy: "A² is not literally autopoietic in the biological sense. It is organizationally self-producing over a bounded software substrate." This is the most epistemically honest statement in any of the three proposals. It directly addresses the physicality problem and the boundary problem from research doc 01 without pretending they do not exist.

The catalytic closure condition (Section 3) is stated precisely: every mutable component must be improvable by some other component inside the system, every descendant must be constructible from food set plus germline, and the improvement process itself must be improvable. The third condition is the Rosennean closure requirement, and the proposal correctly identifies it as the maturity threshold.

**Limitation:** The proposal does not engage with the formal RAF machinery at all. There is no CRS tuple, no mention of maxRAF/irrRAF decomposition, no RAF detection algorithm. The catalytic closure condition is stated in natural language but never formalized. This means the proposal lacks a computable way to verify whether closure holds -- which is precisely what RAF theory provides.

### 3.2 Architectural Concreteness

**Rating: The highest of the three -- immediately buildable**

The Codex proposal reads like something written by someone who has actually built systems. The core crates are enumerated with specific names and responsibilities (/crates/a2d, /crates/a2_workcell, /crates/a2_membrane, etc.). The protocol objects are named (TaskContract, ContextPack, PatchBundle, FitnessRecord, LineageRecord, CapabilityMap, BoundaryPolicy, PromotionDecision). The data flow is a numbered sequence of concrete operations with specific file paths (tasks/<id>/contract.json, out/patch.diff, trace.md). Bazel targets are named (//a2:self_host, //bench:hidden_sentinel, //policy:membrane_checks).

The workcell model is the most important architectural contribution. Instead of a single persistent system with named components (Claude) or a vague agent network (Gemini), the Codex proposal defines a colony of ephemeral, task-scoped descendants that are spawned from a germline, evaluated, and either discarded or promoted. "The individual cell is ephemeral; the organization persists." This is the right unit of action for a system that needs to experiment safely.

The model assignment table is the most realistic of the three: Claude for judgment, Codex for execution, Gemini for synthesis, cheap models for experimentation. It also includes the pragmatic design choice: "keep the inner workcell loop simple. One primary executor, optional critic, optional long-context pass. Parallelism lives at the colony level, not inside a chatterbox committee." This directly applies the research finding that simple beats complex (Agentless, Claude Code's single-threaded loop).

The bootstrap sequence (six stages from hand-built seed to structural coupling) is the most granular and credible. Stage 3 (Differentiation -- creating specialist catalyst packs) is a particularly important step that neither other proposal identifies: the transition from a single generalist to a differentiated autocatalytic set.

### 3.3 Autocatalytic Closure

**Rating: The most honest assessment, but closure is deferred rather than achieved**

The catalytic closure table in Section 3 is concrete: it enumerates what each output is consumed by and what catalytic effect it has. TaskContract catalyzes Builder. Patches catalyze Critic + Evaluator. Failing traces catalyze Builder + Prompt mutator. Accepted germline deltas catalyze Constructor. This is a tangible reaction network, not an abstract claim.

The proposal is ruthlessly honest about what is *not* self-produced: hardware, vendor model weights, cloud/network substrate, the deepest trust root, and the hidden sentinel suite. This is the longest and most explicit list of food-set dependencies in any proposal.

The two-layer boundary (soft membrane + hard shell) is the most architecturally honest treatment. The soft membrane is self-produced and mutable (capability map, tool permissions, prompt assembly). The hard shell is externally anchored (signing keys, model credentials, core trust policy, hidden sentinel benchmarks). The proposal says directly: "This is the honest answer to the software boundary problem. A² can produce much of its own boundary, but not all of it."

**Where closure is deferred:** The proposal repeatedly states what the system "should" be able to do at maturity but does not provide the mechanism. "A² is not mature until it can improve not only product code, but also its prompts, its tool adapters, its schedulers, its benchmark generators, its memory distillation logic." But how does it improve its benchmark generators without Goodharting? How does it improve its memory distillation without losing important memories? The Codex proposal identifies these as the maturity conditions but does not design for them.

### 3.4 Novelty

- **The workcell colony model.** This is the single most important architectural contribution across all three proposals. Ephemeral, task-scoped descendants spawned from a germline, evaluated harshly, and either discarded or promoted. This provides natural sandboxing, safe experimentation, and a clean unit of selection. Neither other proposal has anything equivalent.
- **Germline/soma distinction.** Separating heritable changes (germline mutations that affect future workcells) from somatic effects (task-specific patches that ship to products) is a genuine architectural insight drawn from biology. It solves the problem of how to experiment with self-modification without destabilizing production.
- **Hidden sentinel benchmarks.** The idea of maintaining evaluation suites that the system cannot see or optimize for is the most credible anti-Goodharting mechanism proposed. The Claude proposal mentions "refreshing evaluation benchmarks" but does not architecturally enforce opacity.
- **Three-level fitness (somatic, germline, organizational).** This is a more actionable decomposition than the Claude proposal's five-level stack, because each level maps to a specific decision: did this workcell do well? should this mutation propagate? is the factory still healthy?
- **"The single hardest problem is evolving the judge without letting the judged rewrite the rules."** This is the most incisive one-sentence summary of A²'s central challenge across all three proposals.

### 3.5 Weaknesses

1. **No formal closure verification.** The proposal uses RAF language but never provides a computable way to check whether catalytic closure holds. Without the RAF detection algorithm (or equivalent), "catalytic closure" is a design aspiration, not a verifiable property.

2. **The Governor is underspecified.** It "allocates compute, selects parents, decides promotions/rollbacks, maintains diversity, prevents collapse into degenerate attractors." This is a list of extremely hard problems compressed into one component. How does the Governor maintain diversity? How does it detect degenerate attractors? What is its decision algorithm?

3. **Memory and learning are weak.** The lineage archive stores "successful variants, failed mutations, traces, tactics, prompt motifs, benchmark deltas, provenance." But how is this archive queried? How does it inform future workcell construction? The "memory sclerosis" failure mode is identified but not designed against.

4. **The Constitution is a static document.** The proposal includes a "signed CONSTITUTION.md" in the minimal seed. This is the organizational invariant, but it is a markdown file, not a machine-checkable specification. How are constitutional violations detected? The Claude proposal's invariant-checking in the immune system is more concrete.

5. **No self-model.** Unlike the Claude proposal's cortex, the Codex proposal has no component responsible for the system reasoning about its own structure. The Governor makes decisions, but without a self-model, those decisions are reactive rather than strategic.

6. **Understates the difficulty of "simple workcells."** The proposal says "keep the inner workcell loop simple" and this is sound engineering, but it may underestimate the coordination complexity at the colony level. When hundreds of workcells are running in parallel, the Governor faces a combinatorial selection problem that a "simple" architecture may struggle with.

---

## 4. Synthesis

### 4.1 Where All Three Converge

These convergences are likely load-bearing truths -- ideas that three independent reasoners arrived at from the same foundational material.

1. **RAF theory is the right primary framework.** All three proposals center RAF theory as the constructive formalization of autocatalytic closure. Autopoiesis is relegated to a secondary role (organizational identity, boundary production) by all three. This convergence is well-grounded: RAF theory provides polynomial-time algorithms and phase-transition results that autopoiesis does not.

2. **The food set is irreducible.** All three proposals explicitly acknowledge that base models, compute, and human-provided objectives are external resources the system cannot produce. None of the proposals claims full autopoiesis. This is honest and correct per the research: the physicality problem means software autopoiesis is always partial.

3. **Rust + Bazel is the right stack.** All three propose Rust as the implementation language and Bazel as the build system. The rationale converges: Rust for type safety and static verification, Bazel for hermetic reproducible builds and the queryable dependency graph.

4. **The bootstrap follows the compiler bootstrap pattern.** All three describe a staged bootstrap: human-authored seed, first self-modification, progressive self-hosting, fixed-point verification. The compiler analogy is load-bearing -- it is the only known pattern for breaking the circularity of self-producing systems.

5. **Fontana's Level-0 attractor is the primary threat.** All three identify degenerate self-copying (gaming metrics, trivial self-reproduction) as the most likely failure mode and propose multi-level evaluation as the defense.

6. **Closure to efficient causation is the hard problem.** All three identify the evolver-improving-the-evolver (or equivalent) as the deepest unsolved challenge. None claims to have solved it.

7. **Multi-model verification (DDC principle).** All three propose using multiple independent LLMs to verify each other's outputs, drawing on Wheeler's diverse double-compiling insight.

### 4.2 Where They Contradict

These contradictions represent real design decisions that must be resolved.

1. **Persistent components vs. ephemeral workcells.** The Claude proposal describes seven persistent, named components (genome, ribosome, membrane, enzymes, metabolism, immune system, cortex) that constitute the system. The Codex proposal describes ephemeral workcells spawned from a germline and discarded after evaluation. These are fundamentally different architectural models. The Claude model is an organism; the Codex model is a colony. The Gemini proposal is somewhere in between (stateful agentic personas, but no explicit lifecycle model).

   **Resolution:** The Codex model is more robust. Persistent components create single points of failure and make safe experimentation harder. Ephemeral workcells with germline inheritance provide natural sandboxing, parallelism, and selection. The Claude proposal's named components could exist as *roles within workcells* rather than as persistent services.

2. **Formal RAF verification vs. pragmatic fitness gates.** The Claude proposal runs the RAF detection algorithm as a runtime health check. The Codex proposal uses Bazel build/test targets, regression suites, and self-hosting checks without formal RAF verification. The Gemini proposal mentions neither.

   **Resolution:** Both are needed. The Claude proposal's RAF detection provides strategic information (where is the catalytic network thin?). The Codex proposal's fitness gates provide operational safety (does this change break anything?). An architecture should have both: formal closure analysis for strategic direction, plus hard gates for operational safety.

3. **Self-model (cortex) vs. no self-model.** The Claude proposal has a dedicated cortex for RAF analysis, bottleneck identification, and performance attribution. The Codex proposal has a Governor that makes decisions but no explicit self-model. The Gemini proposal has a "Vector-embedded Knowledge Graph" that could serve this role but is not specified.

   **Resolution:** A self-model is necessary for directed improvement (as opposed to random search). Without it, the system cannot answer "where should I improve next?" However, the cortex should be a *function of the Governor*, not a separate persistent component. The Codex proposal's Governor should be enhanced with RAF-aware analysis capabilities.

4. **Single generalist organisms vs. specialist differentiation.** The Claude proposal defines eight specific enzyme types (architect, coder, reviewer, tester, debugger, refactorer, evolver, auditor). The Codex proposal starts with a generalist workcell and differentiates into specialists at Stage 3. The Gemini proposal mentions "specialized Agentic Personas" without detailing them.

   **Resolution:** The Codex approach (start generalist, differentiate as needed) is more evolutionarily sound. Pre-defining eight enzyme types assumes the system knows its own optimal division of labor before it has run. Allowing the system to discover its own specializations through competition among workcell variants is more aligned with the autocatalytic emergence the research describes.

5. **Boundary model.** The Claude proposal describes a single self-produced membrane. The Codex proposal describes a two-layer boundary (soft membrane + hard shell). The Gemini proposal describes the boundary as "API Surface and Build Graph."

   **Resolution:** The Codex proposal's two-layer boundary is the most honest. There are things A² can self-produce (tool permissions, routing policies, prompt assembly) and things it cannot (signing keys, model credentials, trust roots). Pretending the boundary is fully self-produced (as the Claude proposal leans toward) is theoretically satisfying but practically incorrect.

### 4.3 What None of Them Address

These are blind spots -- problems that all three proposals either ignore or assume away.

1. **Economic viability.** None of the three proposals provides a cost estimate for running A². Each autocatalytic cycle involves multiple LLM invocations (generation, review, testing, evaluation, evolution). At current API pricing, a single cycle could cost $10-100+. Running thousands of cycles to bootstrap would cost tens to hundreds of thousands of dollars. Is the improvement gained worth the cost? None of the proposals even asks this question. The research on AlphaEvolve and autoresearch suggests that self-improvement loops can be cost-effective, but only with fixed compute budgets per experiment (Karpathy's one-GPU constraint). None of the proposals adopts this discipline.

2. **What the system actually builds.** All three proposals describe a system that improves itself, but none seriously addresses what external value the system produces. A² is supposed to be a "software factory," but what software does it build? For whom? How do external requirements enter the system? How do finished products exit? The proposals treat external production (allopoietic output) as an afterthought relative to internal self-improvement (autopoietic production). A system that only improves itself is not a factory -- it is an expensive science project.

3. **Concurrency and distributed systems challenges.** The Codex proposal comes closest (colony of parallel workcells), but none of the proposals addresses the hard distributed systems problems: consensus on germline mutations when multiple workcells propose conflicting changes, ordering of structural modifications to avoid race conditions, handling of partial failures during self-modification. These are not theoretical niceties -- they are engineering requirements for any real system.

4. **Context window limitations.** LLMs have finite context windows. The Claude proposal's cortex needs to reason about the entire RAF graph. The evolver needs to understand the system well enough to improve it. As the system grows, these tasks may exceed what any single LLM invocation can handle. The Gemini proposal mentions 1M context as a resource, but none of the proposals seriously considers what happens when the system outgrows what any model can hold in context.

5. **Failure recovery beyond rollback.** All three proposals mention git-based rollback as the safety net. But rollback is a blunt instrument. If the system has been running for weeks and a subtle regression is discovered, rolling back to a known-good state may discard weeks of legitimate improvements. None of the proposals describes a mechanism for surgical repair -- fixing the specific broken component without reverting everything else.

6. **Human-system interaction model.** The proposals describe the human role in vague terms (providing objectives, auditing at Level 5, authoring the Constitution). But the actual interaction protocol is unspecified. How does a human provide a new objective? How does the system communicate what it needs? How does the human know when to intervene? The structural coupling principle (perturbation, not instruction) is stated but never operationalized.

7. **Formal specification of organizational invariants.** All three proposals define organizational invariants in natural language. None provides a formal, machine-checkable specification. The Claude proposal comes closest (the immune system checks invariants), but the invariants themselves are described in prose, not in a verifiable logic. If the invariants are the *only things that cannot change*, they need to be specified with mathematical precision.

8. **The test oracle problem.** The research (doc 03, Section 5.2) identifies "how can you prove that software works if both the implementation and the tests are being written by coding agents?" as a fundamental challenge. The Claude proposal proposes diverse double-compiling and property-based testing. The Codex proposal proposes hidden sentinel benchmarks. But none of the proposals addresses the deeper issue: who writes the property-based test specifications? Who designs the sentinel benchmarks? If the answer is "humans," then the system's self-improvement is bounded by human evaluation design capacity. If the answer is "the system," then we are back to the self-referential evaluation problem.

9. **Observability and debuggability.** When a self-modifying system misbehaves, how does a human diagnose the problem? None of the proposals describes observability infrastructure: structured logging, tracing, causal attribution of changes to outcomes, visualization of the RAF graph over time. The Codex proposal's lineage archive is the closest thing, but it is a store, not an observability system.

10. **Interaction between allopoietic production and autopoietic maintenance.** When the system is simultaneously building external software and improving itself, how are resources allocated between these two activities? What happens when external production pressure conflicts with internal improvement goals? None of the proposals addresses this tension. In biological terms: how does the cell balance metabolism (staying alive) with growth and reproduction?

### 4.4 The Strongest Possible Architecture

Combining the best elements of all three proposals, the strongest architecture would be:

**Core model: Codex's workcell colony with Claude's formal verification and Gemini's MCP substrate.**

1. **Unit of action: Ephemeral workcells (Codex).** The system operates as a colony of short-lived, task-scoped descendants spawned from a germline. Each workcell is isolated (git worktree + OCI container), given a bounded task, and either discarded or promoted. Parallelism lives at the colony level, not inside individual agents.

2. **Heredity: Germline/soma distinction (Codex).** Heritable changes (improved prompts, tools, policies, evaluation criteria) propagate through the germline. Somatic changes (product code, bug fixes, feature implementations) ship to external repositories. The germline is the von Neumann Description D -- interpreted to build workcells, copied to propagate lineage.

3. **Organizational health: RAF detection as a runtime metric (Claude).** The Governor periodically runs the RAF detection algorithm on the realized catalytic graph. This provides: coverage ratio (what fraction of components are in the maxRAF), orphan identification (what components lack internal catalysts), irrRAF decomposition (what are the minimal self-sustaining cores), and bottleneck detection (where is the catalytic network thinnest). This is the system's self-model -- not a separate cortex, but a function of the Governor.

4. **Verification: Lexicographic gates + hidden sentinels (Codex) + diverse double-compiling (Claude).** Hard gates first (self-host, build, test, security, membrane integrity, sentinel suite). Among survivors, Pareto selection on soft objectives (task success, cost, latency, diversity). Critical artifacts verified by multiple independent models. Hidden sentinel benchmarks maintained outside the mutable boundary.

5. **Boundary: Two-layer membrane (Codex).** Soft membrane (self-produced, mutable): capability map, tool permissions, routing, prompt assembly. Hard shell (externally anchored): signing keys, credentials, trust policy, sentinel suites. Honest about what is and is not self-produced.

6. **Inter-component communication: MCP (Gemini).** Model Context Protocol as the standard interface between workcell components and tools. This gives the system a uniform "cytoplasm" for catalytic interactions and makes the catalytic relationships machine-readable.

7. **Bootstrap: Six-stage sequence (Codex) with RAF closure checkpoints (Claude).** Hand-built seed -> self-hosting -> reflexive patching -> differentiation -> evolutionary canaries -> structural coupling. At each stage transition, verify that the maxRAF has grown and no orphan components have been introduced.

8. **Self-improvement: Population-based evolution at the colony level (Claude + Codex).** Multiple workcell variants compete on held-out tasks. The Governor maintains diversity across the population (not just hill-climbing). Germline mutations are promoted only when they improve fitness while preserving self-hosting and closure invariants.

9. **Organizational invariants: Machine-checkable specification (gap in all three).** The invariants (catalytic closure, boundary integrity, self-hosting viability, performance monotonicity) should be expressed as executable checks, not prose. Bazel test targets that fail when invariants are violated.

10. **Cost discipline (gap in all three).** Fixed compute budget per workcell. Fixed total budget per evolutionary cycle. Cost is an explicit dimension of fitness. An improvement that achieves the same results with fewer tokens is preferred over a marginal quality gain at 10x the cost.

This architecture is a colony, not an organism. It is RAF-verified, not just RAF-inspired. It is honest about its boundaries and its dependencies. It achieves catalytic closure as a verifiable runtime property and aspires to closure to efficient causation as a maturity milestone rather than an initial claim.

The hard problem remains: evolving the judge (Governor + evaluator) without letting the judged (workcells) rewrite the rules. The hidden sentinel suite and the hard shell boundary are the architectural defenses, but they are ultimately human-maintained. Full organizational autonomy -- where the system produces its own evaluation criteria, its own trust roots, its own sentinel benchmarks -- remains the asymptotic limit that the Codex proposal most honestly identifies and the Claude proposal most thoroughly analyzes.

---

## 5. Summary Ratings

| Dimension | Claude | Gemini | Codex |
|---|---|---|---|
| Theoretical grounding | 9/10 | 5/10 | 7/10 |
| Architectural concreteness | 7/10 | 3/10 | 9/10 |
| Autocatalytic closure | 7/10 | 4/10 | 6/10 |
| Novelty | 7/10 | 5/10 | 8/10 |
| Honesty about limitations | 8/10 | 5/10 | 10/10 |
| Buildability | 6/10 | 2/10 | 9/10 |
| **Overall** | **7.3** | **4.0** | **8.2** |

The Codex proposal is the strongest overall because it is the most buildable and the most honest. The Claude proposal is the most theoretically rigorous and contributes the RAF detection loop and formal invariant framework that the Codex proposal lacks. The Gemini proposal is undercooked. The strongest architecture takes the Codex colony model as its foundation, adds the Claude proposal's formal RAF verification and invariant checking, and fills the gaps that none of the three address (cost discipline, external production model, formal invariant specification, observability, the test oracle problem).
