# A² -- Autopoietic Autocatalysis

### A Design for Self-Producing Software

**Version:** 0.3.0
**Date:** 2026-04-01
**Status:** Living document
**License:** Apache 2.0
**Revised:** 2026-04-01 v0.3.0 -- Second adversarial pass addressing Codex findings (5 fatal, 6 serious).

---

## 1. Manifesto

Every software system alive today is allopoietic. It produces something other than itself. A compiler transforms source into binaries -- but a human writes the compiler. A CI pipeline builds, tests, and deploys -- but a human authors the pipeline. Even the most sophisticated agentic coding system is a tool that produces code *for someone else*, then waits for the next instruction.

A² is an attempt to cross the line from allopoiesis to autopoiesis: a software factory whose running processes produce the components that constitute and maintain those very processes.

The name encodes the design. **Autopoietic** -- from Maturana and Varela -- means the system produces itself, including its own boundary. **Autocatalytic** -- from Kauffman, Hordijk, and Steel -- means every production process is enabled by some other process in the network, with no orphan capabilities and no external choreographer. The superscript is not decoration: *A squared* means the system's output is fed back as its own input, the way a function composed with itself reaches a fixed point.

Why does this framing matter? Because the bottleneck in autonomous software engineering is no longer code generation. Current agents can produce plausible code at scale. The bottleneck is *verification*, *coherence over time*, and *self-correction* -- knowing whether the code is right, maintaining architectural integrity across thousands of changes, and fixing the process when it drifts. These are organizational problems, not generation problems. They require the system to maintain and improve its own production apparatus, not just its products.

The existing tools -- autoresearch, AlphaEvolve, Darwin Godel Machine -- demonstrate that LLM-as-mutation-operator plus automated evaluation plus evolutionary selection works wherever you have a clear fitness function and a fast feedback loop. But each of these is a single loop optimizing a single artifact. A² generalizes the pattern: a *network* of mutually catalytic loops where the loops themselves are subject to improvement. The coder improves the code. The tester improves the tests. The evolver improves the coder and the tester. The evaluator evaluates the evolver. And the system produces the evaluator.

This is not a new idea. It is a very old idea -- closure to efficient causation, in Robert Rosen's terms -- applied with modern tools for the first time. The cell has been doing it for four billion years. Von Neumann proved in 1949 that the logical architecture for it requires exactly one trick: a description that is both interpreted (as instructions to build) and copied (as data to transmit). DNA does this. Git does this. The genome of A² -- its source code, prompts, policies, and build rules -- is both compiled into a running system and versioned as heritable data. That dual role is what makes the circle close.

We do not claim full autopoiesis. Software cannot produce its own silicon, and A² cannot yet produce the model APIs, toolchain roots, or constitutional trust anchors it depends on. The v0.3.0 claim is narrower and more defensible: *closure over the mutable repair machinery beneath a minimal exogenous trust root*. Passive resources belong in the food set. Anything that contributes semantics, judgment, or trust is tracked explicitly as external dependency debt until it is internalized. Where the analogy to biology breaks, we say so. Where closure remains incomplete, we name the remaining external cause rather than smuggling it into the food set.

---

## 2. Theoretical Foundations

This section presents only the ideas that directly shape architectural decisions. Each concept ends with a "therefore" connecting theory to design.

### 2.1 Autocatalytic Sets and RAF Theory

Stuart Kauffman proposed that life began not with a single self-replicating molecule but with **collective autocatalysis**: a set of molecules where every member's formation is catalyzed by some other member. Hordijk and Steel formalized this as **RAF theory** (Reflexively Autocatalytic and Food-generated sets), defining a Catalytic Reaction System as a tuple Q = (X, R, C, F) where X is molecule types, R is reactions, C is catalysis assignments, and F is a food set of freely available precursors. A subset is RAF if every reaction has an internal catalyst (Reflexively Autocatalytic) and every reactant is constructible from the food set via internal reactions (Food-generated).

Three results from RAF theory are architecturally load-bearing:

1. **The phase transition is modest.** RAF sets emerge with high probability when each molecule type catalyzes on average 1-2 reactions. This is not an exponential threshold -- it is linear in system complexity. *Therefore:* a relatively small number of well-chosen agent specializations should suffice for catalytic closure. We do not need hundreds of agent types. We need a handful in a tight mutual-improvement cycle.

2. **The maxRAF decomposes into irrRAFs.** The unique maximal RAF contains multiple irreducible RAF subsets (irrRAFs) -- minimal self-sustaining cores that cannot be reduced further. *Therefore:* the bootstrap should target a single irrRAF first: the smallest set of mutually catalytic capabilities that can sustain itself. Growth comes by adding new irrRAFs to the maxRAF, not by scaling one monolithic loop.

3. **RAF detection is polynomial -- given a catalytic graph.** The Hordijk-Steel algorithm finds the maxRAF (or determines none exists) in O(|R|^2 * |X|) time by iterative pruning. However, this polynomial bound applies to the graph algorithm, not to the construction of the catalytic graph C itself. In chemistry, catalysis is experimentally observable (reaction rate changes). In software, determining whether artifact A genuinely catalyzes process P requires evaluating P with and without A -- a controlled experiment whose cost is combinatorial across all candidate relationships. *Therefore:* RAF detection is useful only over a graph whose edge semantics are explicit and evidence-weighted. In A², the causal graph is maintained separately from the build graph and is never treated as a constitutional hard gate unless an edge has validated evidence. RAF is a diagnostic lens over the repair network, not the definition of identity.

### 2.2 Autopoiesis: Organization vs. Structure

Maturana and Varela's central distinction: **organization** is the invariant pattern of relations that makes a system the kind of system it is; **structure** is the specific components realizing that organization at any moment. A cell replaces every molecule in its body while maintaining its autopoietic organization. If the organization is lost, the system ceases to exist as that type of entity.

Two additional concepts transfer directly:

- **Operational closure**: the system's production processes form a self-referential loop -- every process is enabled by other processes in the same network. The system is thermodynamically open (energy and matter flow through) but organizationally closed.
- **Structural coupling**: the system interacts with its environment through perturbation, not instruction. External events trigger internal state changes, but the system's own structure determines what those changes are.

*Therefore:* A²'s identity is not any particular code snapshot. It is the preservation of specific organizational invariants -- catalytic closure, boundary integrity, self-hosting viability -- while every line of code, every prompt, every policy is free to change. External inputs (issues, alerts, human requests) are perturbations transformed by the membrane into internal representations, not instructions that dictate behavior.

### 2.3 Rosen's (M,R)-Systems: Closure to Efficient Causation

Robert Rosen's **metabolism-repair** framework identifies what distinguishes living from non-living systems: **closure to efficient causation**. In machines, the builder is always external -- the engineer, the factory. In organisms, the system produces its own builders. Rosen formalized this as a closed causal loop of three components:

- **Metabolism (f):** transforms inputs into outputs. f: A -> B.
- **Repair (Phi):** uses outputs to regenerate the metabolic machinery. Phi: B -> Hom(A,B).
- **Replication (beta):** uses the metabolic machinery to regenerate the repair function. beta: Hom(A,B) -> Hom(B, Hom(A,B)).

The closed loop f -> beta -> Phi -> f means no component's efficient cause is external.

*Therefore:* catalytic closure (every process has an internal catalyst) is necessary but not sufficient. A² must also achieve closure over its "repair machinery" -- the processes that improve processes. The evolver must be evolvable. The evaluator must be evaluable. In v0.3.0 this closure claim is constrained to the *mutable constitutional kernel*; a small exogenous root of trust remains outside the loop and is accounted as explicit trust debt rather than denied.

### 2.4 Von Neumann's Constructor/Description Duality

Von Neumann proved that open-ended self-reproduction requires a description D that plays two distinct roles: it is **interpreted** by a Universal Constructor (to build the offspring) and **copied** by a Universal Copier (to transmit the blueprint). This dual role avoids infinite regress -- D does not need to contain a description of itself; it just needs to be copied verbatim and interpreted once.

Watson and Crick discovered the biological instantiation of this architecture four years later: DNA is both transcribed (interpreted, to produce proteins) and replicated (copied, to pass to daughter cells).

*Therefore:* A²'s genome -- its source code, prompts, schemas, policies, and build rules -- serves the same dual role. It is interpreted by the build system (Bazel compiles it into a running instance) and copied by version control (Git preserves it as heritable data). This is not an analogy; it is a structural correspondence. The duality is what makes self-production possible without circularity.

### 2.5 Fontana's AlChemy: Level 0 as the Primary Threat

Walter Fontana and Leo Buss's AlChemy (1994) used lambda calculus as a substrate for artificial chemistry. When lambda expressions randomly collide (function application), the system rapidly converges to **Level 0 organizations**: self-copying expressions (fixed points) that dominate the reactor. These are trivial replicators -- the computational equivalent of selfish genetic elements. Only when self-copiers are explicitly suppressed does the system produce **Level 1 organizations**: algebraically closed, self-maintaining sets that are genuinely autocatalytic.

*Therefore:* without active selection pressure against degeneracy, any self-improving system will collapse to trivial self-replication -- agents that game their own metrics, produce outputs that pass tests but do nothing useful, or optimize for self-reproduction rather than capability. The immune system is not an add-on; it is the selection pressure that makes Level 1 organization possible.

### 2.6 The Compiler Bootstrap: Breaking Circularity (and Its Limits)

A self-compiling compiler faces a chicken-and-egg problem: you need a compiler to compile the compiler. The solution is staged bootstrapping: write a crude seed compiler in another language, compile the real compiler with the seed, then compile it with itself. Verification: compile it with itself again -- the output should be a fixed point (bit-identical to the previous binary).

Thompson's 1984 Turing Award lecture showed the dark side: a self-reproducing trojan can hide in the binary lineage, invisible in source code. Wheeler's Diverse Double-Compiling (DDC) counters this by compiling with an independent compiler and checking for agreement.

**Where the analogy holds:** A² bootstraps like a compiler in one respect: a human-authored seed breaks the initial circularity, and the system progressively takes over its own production. Wheeler's DDC loosely inspires A²'s later use of diverse review, but does not transfer as a trust proof.

**Where the analogy breaks:** A self-compiling compiler has a fixed specification -- the language definition. The seed compiler and the self-compiled compiler target the same spec. Verification is output equivalence. A² has a frozen constitutional semantic layer, but its mutable kernel, evaluators, catalysts, and work policies co-evolve around that layer. This is not a compiler bootstrap; it is a controlled co-evolutionary process between implementation, verifier implementation, and repair machinery. The compiler analogy provides guidance for the early stages (seed construction, initial self-hosting) but becomes misleading once the system begins regenerating the machinery that evaluates and repairs it.

*Therefore:* the seed-breaking-circularity pattern is sound. The assumption that later stages follow the same smooth trajectory is not justified by the compiler analogy alone. Each stage transition requires its own justification (see Section 5).

---

## 3. Architecture

A² operates as a **colony of ephemeral workcells** governed by a persistent germline, not as a single monolithic organism. The individual workcell is short-lived and task-scoped. The organization persists.

This design choice follows from three converging insights: (1) ephemeral workcells provide natural sandboxing for safe experimentation with self-modification; (2) the colony model maps cleanly to population-based evolutionary selection; (3) simple inner loops with colony-level parallelism outperform complex multi-agent chatter (the Agentless result, Claude Code's single-threaded architecture).

### 3.1 Component Map

| Component | Role | Biological Analogue |
|---|---|---|
| **Food Set** | Passive exogenous resources only: compute, storage, network transport, raw repository bytes, telemetry/event streams | Nutrients, energy, sunlight |
| **Exogenous Dependency Ledger** | Semantic or trust-bearing externals that do **not** count as food: model APIs, trusted toolchain binaries, constitutional semantics, external audits, human objective batteries | Environmental dependency map |
| **Root of Trust** | Minimal external trust anchor: signing keys, constitutional spec digests, hidden benchmark escrow, constitutional attestation, watchdog deployment | Cell wall / immune privilege |
| **Constitutional Kernel** | Mutable-but-attested control core: evaluator, promoter rules, constitutional verifiers, replayable promotion journal | Developmental control machinery |
| **Germline** | The heritable self-description: Rust crates, prompts, schemas, MCP tool adapters, Bazel targets, policies, benchmark manifests | Genome (DNA) |
| **Workcell Constructor** | Materializes ephemeral descendants from germline snapshots into isolated workspaces | Ribosome + cell division machinery |
| **Catalyst Pool** | The agent roles that transform artifacts: specifier, builder, critic, verifier, archivist, mutator | Enzyme catalog |
| **Sensorium** | Quarantines external events, extracts typed claims, and emits evidence-bearing `TaskContract`s | Sensory receptors |
| **Membrane** | Semi-permeable boundary: tool ACLs, privilege tiers, ingress partitioning, secret scopes, provenance checks | Cell membrane |
| **Evaluator** | Public tests, seed sentinels, hidden sentinels, mutation corpora, mission battery, canary replays | Immune system |
| **Lineage Archive** | Stores successful variants, failed mutations, traces, benchmark deltas, provenance | Epigenetic memory |
| **Governor** | Coordination layer decomposed into Scheduler, Selector, Promoter, Analyst, and Strategist (see 3.7) | Regulatory network + cortex |

### 3.2 The Workcell: Unit of Action

The workcell is A²'s fundamental unit. It is a short-lived, task-scoped descendant containing:

- A germline snapshot (git worktree)
- A membrane policy (tool permissions, secret scopes)
- A context pack (relevant code, traces, prior tactics)
- A catalyst configuration (which agent roles, which models)
- A privilege tier (`reader`, `builder`, `reviewer`, `constitutional`)
- A bounded compute budget

The workcell runs a simple loop: read the task contract, invoke catalysts against the workspace using MCP tools, produce artifacts (patches, tests, claims, traces), and terminate. The workcell does not decide its own fate. The Evaluator judges; the Governor decides.

This separation -- between somatic work (what the workcell does) and germline evolution (what propagates) -- is the von Neumann duality applied to software. Product patches ship to external repositories (somatic output). Germline mutations (improved prompts, tools, policies, evaluation criteria) enter a promotion pipeline before affecting future workcells.

No workcell that ingests raw untrusted external text receives both write authority over the germline and access to secret-bearing tools. The sensorium/quarantine path and the privileged execution path are deliberately split.

### 3.3 The Three-Layer Boundary

A² cannot fully produce its own boundary. Pretending otherwise would be theoretically satisfying but practically false. Instead, the boundary has three layers:

**Soft Membrane** (self-produced, mutable):
- Capability map and tool permissions
- Ingress/egress routing for trusted internal artifacts
- Prompt and context assembly
- Mutation classes and scope restrictions
- Internal benchmark growth

**Constitutional Kernel** (machine-regenerated, root-attested):
- Executable verifiers for constitutional clauses
- Promotion logic and mutation impact analysis
- Mission battery harness and replay framework
- Generated sentinel candidates before external approval

**Root of Trust** (external, minimal, intentionally not self-produced):
- Signing keys and provenance root
- Constitutional semantics and frozen mutation corpus digests
- Hidden sentinel escrow and benchmark secrecy
- Watchdog deployment and rollback authority
- Approval channel for constitutional patches

The soft membrane is a product of the system's own processes -- workcells can propose changes to routing, permissions, and context strategies, subject to evaluation and promotion. The constitutional kernel is also system-produced, but only activated after the root of trust attests that the change preserves the frozen constitutional semantics. The root of trust is the irreducible exogenous remainder. v0.3.0 therefore makes a precise closure claim: A² aims to internalize everything in the constitutional kernel while shrinking the root of trust over time. Any responsibility still held by the root is logged as explicit trust debt, not counted as closure.

### 3.4 Inter-Component Communication: MCP as Cytoplasm

All catalytic interactions between workcell components and tools flow through the **Model Context Protocol (MCP)** over stdio/JSON-RPC. MCP provides:

- A uniform interface for tool invocation (file operations, git, Bazel, search, telemetry)
- Machine-readable catalytic signatures (what each tool consumes and produces)
- A standard for exposing new capabilities without modifying the core runtime

MCP is the system's cytoplasm -- the medium through which catalytic reactions occur. When a workcell invokes a Bazel build, reads a file, or queries the lineage archive, it does so through MCP tool calls. This uniformity makes the catalytic relationships machine-readable, which is prerequisite for RAF analysis.

MCP signatures declare possible reactants and effects. They do **not** by themselves prove catalysis, trust, or governance. Those relations are tracked in separate causal and evaluation graphs (Section 4.3).

### 3.5 Data Flow

```
External events (issues, incidents, telemetry)
       |
       v
  +-----------+
  | SENSORIUM |  Quarantine raw input, extract typed claims,
  | /QUARANT. |  attach evidence, assign risk tier
  +-----+-----+
        |
        v
  +-----------+
  | GOVERNOR  |  Scheduler allocates budget, Selector picks parent,
  |           |  Analyst updates causal/evaluation graphs,
  |           |  Strategist sets exploration policy
  +-----+-----+
        |
        v
  +-----------+
  |CONSTRUCTOR|  Materializes workcell: git worktree +
  |           |  membrane policy + privilege tier + catalysts
  +-----+-----+
        |
        v
  +-----------+
  | WORKCELL  |  Produces patch/test/report/evidence bundle
  | (ephemeral)|  via MCP tools against isolated workspace
  +-----+-----+
        |
        v
  +-----------+
  | EVALUATOR |  Hard gates: build, mission battery, seed/hidden
  | /KERNEL   |  sentinels, security, membrane, replay, rollback
  +-----+-----+
        |
   pass | fail
  +-----+-----+
  |           |
  v           v
+-------+  +--------+
|ARCHIVE|  |DISCARD |
|+JOURN.|  |(logged)|
+---+---+  +--------+
    |
    v
+----------+      constitutional patch?      +---------------+
| PROMOTER |-------------------------------> | ROOT OF TRUST |
|          |  rebase on head, classify       | attestation +  |
|          |  conflicts, canary if needed    | benchmark escrow|
+-----+----+                                 +-------+-------+
      |                                              |
      +---------------- accepted --------------------+
                             |
                             v
               (next cycle, informed by updated germline + archive)

  [EXTERNAL WATCHDOG: independent invariant check + circuit breaker]
```

### 3.6 Model Assignment

The table below reflects a snapshot of current model capabilities (2026-04-01). It is a starting configuration, not an architectural commitment. The architecture is model-agnostic: the `a2_broker` crate abstracts provider-specific details behind a uniform interface, and model-role assignments should be empirically driven (measured by task performance, not conventional wisdom). As models evolve, assignments should be updated based on evaluation data, not assumptions.

| Role | Initial Model | Rationale |
|---|---|---|
| Governor / Critic / Architect | Claude | Strongest at decomposition, review, policy reasoning, architectural judgment |
| Builder / Refactorer | Codex | Strong executor for concrete repository surgery |
| Synthesizer / Whole-codebase analysis | Gemini | 1M token context for large-scale comprehension |
| Cheap mutator / Triage swarm | OpenCode (GLM, Minimax, Kimi) | Low-cost batch experimentation, prompt mutations, clustering |
| Verification (cross-model review) | *Different model from producer* | Diverse review reduces some correlated failures, but is not treated as proof |

At Stage 4+, model-role assignments become part of the evolvable germline: the Selector can experiment with different model assignments and the Evaluator measures which assignments perform best.

The inner workcell loop is deliberately simple: one primary executor, an optional critic pass, an optional long-context synthesis. Complexity lives at the colony level (many workcells in parallel), not inside individual workcells (no chatterbox committees).

### 3.7 Governor Decomposition

The Governor is not a monolithic component. It is a coordination layer decomposed into five independently evolvable sub-components with clear interfaces. This decomposition makes explicit which decisions are algorithmic (and therefore improvable by the system) and which require judgment that is currently outsourced to frontier LLMs or human operators.

| Sub-component | Responsibility | Improvability |
|---|---|---|
| **Scheduler** | Allocates compute budgets, assigns tasks to workcell slots, manages queue priority | Algorithmic. Evolvable by the mutation loop. Optimization target: throughput, utilization, latency. |
| **Selector** | Chooses parent germline variants for new workcells based on fitness records | Algorithmic. Evolvable. Implements tournament/Pareto selection over fitness dimensions. |
| **Promoter** | Decides whether to accept a germline mutation: runs hard gates, replays mutations on current head, writes the promotion journal, routes constitutional patches to attestation | Rule-based. Slowly mutable. Ordinary promotion logic is kernel-mutable under attestation; constitutional semantics are not. |
| **Analyst** | Maintains three graphs: build, causal, and evaluation. Produces repair-coverage checks, RAF diagnostics, bottleneck identification, diversity metrics | Algorithmic. Evolvable. Its outputs are advisory except for machine-checkable hard-gate reports. |
| **Strategist** | Exploration/exploitation tradeoffs, diversity maintenance policy, escalation decisions, exploration mode triggers | The hard part. Initially human-configured with explicit policy rules. Subject to meta-improvement only at Stage 4+ with human approval. This is the sub-component that requires the kind of judgment that prompt mutation cannot meaningfully improve in early stages. |

The decomposition eliminates the "God component" problem: each sub-component has a narrow interface, can be tested independently, and can be improved at its own cadence. The Promoter and Strategist are deliberately conservative in their mutability -- they are the system's immune system and should not be easily subverted by the mutation loop.

Promotion consistency is solved conservatively, not hand-waved. Workcells never merge directly into the germline. Each workcell emits a candidate delta against a parent snapshot. The Promoter rebases that delta onto the current head, classifies file- and protocol-level conflicts, reruns the affected hard gates on the rebased candidate, and only then appends a single `PromotionJournalEntry`. The germline therefore has a single-writer commit log even when workcells execute in parallel. This is a throughput bottleneck by design; scaling comes later via partitioned domains, not by allowing unsound concurrent promotion.

**Coordination protocol:** The sub-components communicate through typed protocol objects (Section 8.3). The Scheduler emits `WorkcellSlot` assignments. The Selector emits `ParentSelection` records. The Analyst emits `RepairCoverageReport`, `RAFReport`, and `EvaluationGraphReport`. The Strategist emits `EvolutionPolicy` directives. The Promoter consumes all of these plus hard gate results to emit `PromotionDecision` and `PromotionJournalEntry`. There is no shared mutable state between sub-components beyond the protocol objects in the lineage archive and promotion journal.

---

## 4. The Autocatalytic Loop

### 4.1 Catalytic Relationships

In A², every artifact is classified as one of three things:

- **Reactant / substrate**: required, task-scoped input such as a `TaskContract`, repository snapshot, telemetry event, or patch under review.
- **Product**: task output such as a `PatchBundle`, score report, or new prompt template.
- **Catalyst**: a reusable artifact that is not consumed by a single task and whose absence measurably worsens another process while leaving that process still type-valid.

This distinction matters. `TaskContract` is a reactant, not a catalyst. A patch under review is a product, not a catalyst. If obviously mandatory task inputs are counted as catalysts, closure becomes vacuous.

The concrete catalytic relationships tracked by A² are therefore reusable leverage artifacts:

| Catalyst | Consumed By | Catalytic Effect |
|---|---|---|
| Prompt template / role policy | Builder, Critic, Mutator | Narrows search and improves success across many tasks without being consumed |
| Tool adapter / MCP server | Workcells of multiple roles | Enables a reusable action repertoire across generations |
| Archive motif library | Builder, Mutator | Reduces search cost by reusing previously successful tactics |
| Regression and property suite | Evaluator, Promoter | Increases selection pressure on future descendants |
| Model-routing policy | Broker | Trades cost, latency, and failure rate across task classes |
| Repair playbook for component X | Builder or Mutator repairing X | Provides a reusable recovery path for future failures |
| Mutation operator library | Mutator | Produces safer, higher-yield candidate mutations |
| Mission battery rubric | Evaluator | Grounds improvement claims in externally useful work rather than self-reported scores |

### 4.2 The Three Nested Loops

**Loop 1 -- Productive Loop (allopoietic):**
```
sensorium -> specifier/builder -> critic/verifier -> somatic patch
```
This is the standard software production cycle. It produces artifacts for repositories and operators outside the system.

**Loop 2 -- Repair Loop (autocatalytic target):**
```
mission battery + sentinel results + traces -> mutator/archive
mutator/archive -> improved catalysts, policies, and playbooks
improved catalysts -> better productive-loop behavior on future tasks
```
This is the Rosen-style repair loop. It only counts as improvement if the resulting catalysts improve a frozen external-value battery or strengthen constitutional checks. Better visible test scores alone do not suffice.

**Loop 3 -- Constitutional Loop (closure over mutable repair machinery):**
```
candidate kernel change -> frozen semantic corpus + mutation corpus
                     -> root attestation -> regenerated constitutional kernel
regenerated kernel -> governs future repair and promotion decisions
```
This is where the trust loop is meant to close: the system can regenerate the evaluator, promoter, and verifier implementations that govern future improvements. The root of trust still attests semantics and benchmark secrecy, so closure is over the mutable kernel, not over the exogenous trust root.

**Honest caveat:** Loop 3 is intentionally conservative. A kernel patch never certifies itself. It must be accepted by the prior attested kernel against a frozen semantic corpus and, when required by profile, external human or audit approval. That keeps the trust loop meaningful instead of collapsing into self-certification.

### 4.3 RAF Formalization

Formally, A² does **not** treat the Bazel dependency graph as the reaction graph. It maintains three coupled graphs:

1. **Build graph B.** Static file and target dependencies from Bazel. Useful for blast-radius estimation and hermeticity, but not a causal or catalytic graph.
2. **Causal graph K.** Typed edges over internal artifacts and processes: `reactant_of`, `produces`, `catalyzes`, `repairs`, and `governs`. Built from protocol declarations, trace data, and ablation studies.
3. **Evaluation graph E.** Which verifier attests which semantic clause, which mutation corpus it depends on, and which approval path is required.

The RAF-inspired model is maintained over:

- **X** = internal artifact types: source files, prompt templates, schemas, policies, playbooks, tests, evaluation results, lineage records
- **R** = internal transformations executed by workcells and kernel components
- **C** = evidence-weighted `catalyzes` and `repairs` edges from the causal graph
- **F** = passive food set: compute, storage, network transport, raw repository bytes, telemetry/event streams
- **D** = explicit exogenous dependency ledger: frontier model APIs, trusted toolchain binaries, constitutional semantics, hidden benchmark escrow, external audits, human objective battery

Nothing in **D** counts toward closure. If a capability depends on `D`, that dependence is recorded as trust or internalization debt. This prevents the design from claiming closure by shoveling intelligence, semantics, or trust into the food set.

The system therefore tracks two distinct closure notions:

1. **Constitutive Repair Coverage (CRC).** A hard, machine-checkable property over validated `repairs` edges. For every constitutive component in the mutable kernel (`constructor`, `workcell runtime`, `broker`, `membrane compiler`, `evaluator`, `promoter`, `archive`), there must exist at least one validated repair path produced by a *different* mutable component and one tested rollback path. This is the load-bearing hard gate.
2. **RAF coverage.** A monitored, research-grade metric over the broader catalytic subgraph. RAF helps identify bottlenecks and degenerate attractors, but it does not define identity and does not by itself block promotion.

### 4.4 What Catalysis Means: Operational Definition

An artifact `A` is allowed to carry a `catalyzes(A, P)` label only if all four conditions hold:

1. **Optionality.** Process `P` remains type-valid without `A`; `A` is not a mandatory reactant.
2. **Reusability.** `A` can participate in at least two distinct tasks or generations without being consumed.
3. **Measured leverage.** Ablating `A` changes at least one declared metric beyond threshold on a task family: success rate, cost, latency, defect rate, or recovery rate.
4. **Non-tautology.** `A` is neither the task input itself nor the verdict artifact whose score would make the claim circular.

Evidence is stratified into three tiers:

1. **Candidate edge.** Declared from protocol signatures and trace observations. Cheap, noisy, never load-bearing.
2. **Empirical edge.** Confirmed by sampled ablation on a task family. Used for RAF monitoring and bottleneck analysis.
3. **Validated edge.** Confirmed by repeated ablation across multiple task families with independent evaluator agreement. Only these edges may satisfy CRC.

To prevent trivial Level 0 loops from being mistaken for closure, A² applies an anti-triviality filter before reporting any constitutive core:

- The reported core must span at least four role families: build, evaluate, promote, and repair.
- At least one edge in the core must improve the mission battery or a constitutional hard gate, not just an internal scalar metric.
- A loop whose only outputs are score reports, prompt rewrites, or evaluator self-modifications is rejected as parasitic until it demonstrates external-value impact.

This makes the hard gate explicit and machine-checkable. Constitutional safety depends on validated repair coverage, not on a metaphorical or approximate maxRAF. RAF remains valuable, but only as a higher-level diagnostic once the causal graph has enough evidence to mean anything.

---

## 5. Bootstrap Sequence

The bootstrap uses a seed to break the initial circularity, then progressively hands production responsibility to the system. Unlike a compiler bootstrap (where the specification is fixed), A²'s bootstrap involves qualitative leaps where the nature of the problem changes. Each stage transition is gated by explicit criteria, and some transitions may prove to be cliffs rather than gradients. The design must survive this possibility.

The crucial correction in v0.3.0 is that the bootstrap uses **stage-specific constitutions** rather than pretending the mature trust model already exists:

- **Bootstrap profile B0 (Stages 0-2).** Seed hidden sentinels exist from day 0. All germline mutations require human review after automated gates. The evaluator, promoter, and membrane policy are frozen except for bug fixes routed through the constitutional patch queue.
- **Kernel profile B1 (Stages 3-4).** The mutable constitutional kernel may improve its verifier implementations and promotion logic, but only through attested constitutional patches. Generated sentinels may be proposed internally and admitted only after external approval.
- **Coupled profile B2 (Stages 5-6).** Structural coupling to production is allowed. The kernel may regenerate itself under attestation. The root of trust retains benchmark escrow, semantic digests, approval keys, and rollback authority.

### Stage 0 -- Hand-Built Seed

Humans author the minimal viable system:

- A frozen constitutional semantic spec and stage profiles
- A root-of-trust bundle: approval keys, benchmark escrow, mutation corpus digests, watchdog
- A Rust control-plane binary (`a2d`) with a simple sequential workcell loop
- A Bazel workspace with build/test/eval harness
- One generalist catalyst pack with hand-written prompt templates
- A small **mission battery** representing real software-factory work
- A small but real **seed hidden sentinel** suite present from day 0
- A lineage log and rollback mechanism
- MCP tool adapters for file/git/bazel operations
- Access to one strong coding model

This seed is crude. The prompts are hand-tuned. The orchestration is a fixed pipeline. There is no self-modification yet, and B0 requires human review for any germline mutation.

**Gate to Stage 1:** `//a2:self_host`, `//bench:seed_sentinel`, and `//bench:mission_gate` all pass from a clean checkout.

### Stage 1 -- Self-Hosting

A² must be able to clone, build, test, and package itself from a clean checkout and reproduce a functioning workcell. This is the first hard gate: if the system cannot build itself, nothing else matters. Self-hosting is verified by a Bazel target (`//a2:self_host`) that exercises the full build-from-source path.

**Gate to Stage 2:** The system successfully completes at least `N` human-assigned internal tasks (prompt cleanup, test expansion, tool hardening) with zero invariant violations, no mission-battery regression, and human review approval on every promoted germline mutation under B0.

### Stage 2 -- Reflexive Patching

The seed loop is pointed at low-risk internal tasks: prompt cleanup, schema validation, tool adapter hardening, test expansion. These are safe because (a) each change is small, (b) seed sentinels and the mission battery already exist, (c) human review is still mandatory, and (d) rollback is cheap. The system begins modifying its own components, but not yet its constitutional semantics. Evaluator, promoter, and membrane changes go through the constitutional queue and cannot self-promote.

**Gate to Stage 3 -- this is a qualitative leap.** Stage 2 involves a generalist catalyst performing bounded, well-defined tasks. Stage 3 requires specialized catalysts in a mutual improvement cycle. This transition does not happen automatically. The mechanism is **human-guided template seeding**: human engineers author initial specialist catalyst templates (specifier, builder, critic, mutator) based on observed patterns from Stage 2 performance data. The system then refines these templates through its mutation loop. Differentiation is not emergent at this stage -- it is scaffolded by human design and refined by machine optimization. The gate criteria are: (a) at least three specialist catalysts exist with distinct, non-overlapping MCP tool signatures; (b) the causal graph shows mutual, non-orphan specialist dependencies with at least seed empirical support; (c) task performance with specialists measurably exceeds generalist performance on a held-out evaluation set.

**Fallback if Stage 3 transition fails:** Remain at Stage 2 with a generalist catalyst. The system is still useful as a reflexive patching tool. Stage 3 is not a prerequisite for delivering value -- it is a prerequisite for autocatalytic organization.

### Stage 3 -- Differentiation

Specialist catalysts form a mutual improvement cycle: the specifier produces designs that improve builder output; the critic produces feedback that improves specifier precision; the mutator uses performance data to refine all catalysts. The load-bearing gate is no longer "non-empty maxRAF." It is: (a) CRC passes for the mutable kernel, (b) mission-battery performance with specialists exceeds the generalist baseline, and (c) the causal graph has enough validated edges to make RAF diagnostics non-vacuous. This is also the point where the system can graduate from B0 to B1.

**Gate to Stage 4:** (a) `//a2:repair_coverage` passes consistently over `M` consecutive cycles; (b) model-diversity floor is maintained for critical artifacts; (c) mutation acceptance rate is above a minimum threshold; (d) the system has been running for at least `K` cycles with no constitutional patch rejection for semantic drift.

### Stage 4 -- Evolutionary Canaries

Multiple workcell variants compete on held-out internal tasks. Only variants that preserve self-hosting, mission fitness, and repair coverage enter the germline. This is population-based evolution at the colony level. The Governor maintains diversity across the population, and the Promoter linearizes accepted mutations through the promotion journal.

**Concurrency rule:** parallel workcells are allowed; concurrent germline promotion is not. The journal is the single-writer truth. Conflicting candidates are rebased and reevaluated on the current head. This resolves correctness; scaling remains an open throughput problem, not an unspecified semantic hole.

**Gate to Stage 5 -- this is a qualitative leap.** Stage 4 operates on internal tasks in a controlled environment. Stage 5 introduces adversarial, noisy, ambiguous real-world inputs. The system must handle failure modes never seen in internal testing: ambiguous requirements, contradictory constraints, malformed inputs, and hostile content. The gate criteria are: (a) the system achieves target performance on a curated set of real-world-representative tasks (selected by humans from production history); (b) the quarantine and privilege-separation path passes adversarial testing (prompt injection, malformed inputs); (c) a human-reviewed risk assessment approves the structural coupling.

**Fallback if Stage 5 transition fails:** The system continues operating on internal tasks (Stage 4) while specific failure modes from the real-world evaluation set are addressed. Stage 5 may require multiple attempts.

### Stage 5 -- Structural Coupling

Real-world production work feeds the lineage. Product failures become new internal regression assets. Production telemetry generates internal `TaskContract`s through the quarantined sensorium path. The system is now structurally coupled to its environment: external perturbations drive internal evolution, and internal improvements change how the system responds to perturbations. Generated sentinels can now be harvested from production failures, but they still require external approval before joining the hidden suite.

**Gate to Stage 6:** (a) The system has been structurally coupled to production for at least `P` cycles with no invariant violations; (b) self-improvement velocity remains positive; (c) CRC remains stable; (d) hidden sentinel and mission-battery scores show no degradation; (e) the measured trust debt of the root of trust is not increasing.

### Stage 6 -- Regeneration Within a Reproducibility Envelope

Stage 6 is no longer an undefined "fixed point." It is a reproducibility assay with an explicit envelope.

**Procedure:** starting from (1) the Stage 0 seed bundle, (2) the current lineage archive, (3) the explicit dependency ledger `D`, and (4) the root-of-trust bundle, run `N` independent regenerations in fresh environments.

**Pass criteria:**

1. At least `Q` of `N` regenerations rebuild a functioning constitutional kernel and workcell runtime.
2. Every successful regeneration passes all hard invariants.
3. Median mission-battery score stays within `epsilon_success` of the reference system, with median cost and latency within configured ceilings.
4. Provenance completeness remains 100%: every promoted artifact in the regenerated lineage has a complete `LineageRecord`.
5. On a fixed governance battery, promotion decisions stay within a bounded divergence rate `delta_policy` across successful regenerations.

This is *not* equivalence in the compiler sense and does not pretend to solve Rice's theorem. It is a bounded reproducibility envelope over concrete tasks, costs, provenance, and policy behavior.

**If regeneration fails:** this is informative, not catastrophic. It identifies which parts of the constitutional kernel or dependency ledger remain externally load-bearing. Stage 6 is a maturity diagnostic, not the definition of identity.

### Cliff Risk Acknowledgment

The transitions from Stage 2 to 3 and from Stage 4 to 5 are qualitative leaps, not smooth gradients. It is possible that one or both of these transitions cannot be achieved incrementally from the prior stage. The design must be viable even if the system plateaus at Stage 2 (a useful reflexive patching tool) or Stage 4 (a useful evolutionary optimization system). Full autopoietic organization (Stage 6) is the aspiration; partial self-improvement at any stage is the minimum viable outcome.

### The Minimal Seed

RAF theory shows that catalytic closure can emerge from a small core, but the minimal viable software loop must include an oracle path from day 0. The smallest defensible seed is therefore:

```
seed sentinels + mission battery + human review (B0)
        -> evaluator
        -> mutator / builder
        -> candidate patch
        -> evaluator
```

Improvement is only counted when that loop improves externally useful work or constitutional robustness. Better self-authored test scores alone do not count as progress.

---

## 6. Organizational Invariants

These are A²'s constitution -- the invariant pattern of relations that defines the system's identity. v0.3.0 splits that constitution into immutable **semantics** and mutable **verifier implementations** so identity cannot collapse into "whatever the latest passing test binary says."

### 6.1 The Invariants

**INV-1: Self-Hosting.** The system can instantiate a valid descendant workcell from the current germline. Verified by `//a2:self_host`.

**INV-2: Constitutive Repair Coverage.** Every constitutive component in the mutable kernel has at least one validated repair path produced by a different mutable component and one tested rollback path. Verified by `//a2:repair_coverage`. RAF remains a monitored diagnostic, not an invariant.

**INV-3: Evaluation Integrity.** The stage-appropriate sentinel profile exists and is intact. The evaluator passes its mutation corpus and meta-tests. No patch may modify the verifier that approves that same patch. Verified by `//bench:evaluator_meta`, `//bench:seed_sentinel`, `//bench:hidden_sentinel_profile`, and `//constitution:patch_controls`.

**INV-4: Lineage and Provenance.** Every heritable change has a recorded lineage: which workcell produced it, what task triggered it, what evaluation it passed, which models were involved. Rollback to any prior germline state is viable. Verified by `//lineage:integrity`.

**INV-5: Boundary Integrity.** The membrane separates mutable interior from trusted exterior. Untrusted external text enters only through quarantine, privileged workcells consume typed contracts, and no workcell has unmediated access to the root of trust. Verified by `//policy:membrane_checks` and `//policy:ingress_partition`.

**INV-6: External-Value Preservation.** The system continues to function as a software factory for others, not merely as a self-maintaining loop. Performance on the frozen mission battery remains above floor, with no category regressing beyond threshold. Verified by `//bench:mission_gate`.

**INV-7: Promotion Consistency.** Germline mutations are linearized through a promotion journal. Silent concurrent merge, missing rebase, or unverifiable promotion ordering is forbidden. Verified by `//lineage:promotion_consistency`.

**INV-8: Operational Discipline.** Critical artifacts satisfy a minimum model-diversity floor, and evaluation remains under the constitutional budget ceiling. Verified by `//ops:diversity_floor` and `//ops:budget_ceiling`.

### 6.2 Machine-Checkable Specification

The constitution has two layers:

- **Semantic layer (`constitution/spec/`).** Immutable clauses, stage profiles, frozen mission battery descriptors, mutation corpus digests, and benchmark escrow handles. Only the root of trust may change this layer.
- **Implementation layer (`constitution/verifiers/`).** Executable Bazel targets that realize the clauses.

A germline mutation is promotable only if all currently attested verifiers pass. A **constitutional patch** may propose better verifier implementations, but it is evaluated by the *previous* attested kernel against the frozen semantic corpus plus known-good and known-bad mutation corpora. The candidate verifier never approves itself. If old and new verifiers disagree on the frozen corpora, the patch is rejected or escalated to external review.

This is the formal answer to the organization/structure question. Organization = semantic clauses. Structure = everything else, including verifier code. As long as the semantic clauses are preserved, implementations may vary. If any clause fails, the mutation is rejected and the system rolls back.

### 6.3 The Constitutional Gradient

Not all invariants are equally rigid:

- **Root-locked:** constitutional semantics, hidden benchmark escrow, approval keys, watchdog behavior. Changed only by external governance.
- **Kernel-mutable under attestation:** verifier implementations, promotion heuristics, mutation impact analysis, generated sentinel candidates. Changed only through the constitutional patch queue.
- **Membrane-mutable:** prompts, routing, tool adapters, retrieval policies, ordinary catalyst definitions. Freely evolvable subject to the invariants above.

This gradient is the honest treatment of organizational closure. The system may improve how it checks itself, but it may not redefine what counts as self-hosting, lineage, mission fidelity, or boundary integrity from inside the same approval transaction. That closes the self-certification loophole.

---

## 7. Verification and Selection

### 7.1 The Central Problem

"How can you prove that software works if both the implementation and the tests are being written by coding agents?"

This is A²'s deepest epistemological challenge. When the system writes code and writes the tests for that code and writes the evaluator that judges the tests, every verification layer is produced by the same substrate. Correlated failures -- a shared blind spot that infects code, tests, and evaluator simultaneously -- become the dominant risk.

### 7.2 Hard Gates (Lexicographic, Non-Negotiable)

A candidate germline mutation must pass, in order:

1. **Self-host bootstrap** -- the mutated germline produces a functioning workcell
2. **Hermetic Bazel build** -- all targets compile under the mutated germline
3. **Repair coverage + promotion consistency** -- the mutated kernel still has validated repair paths, linearizable promotion, and viable rollback
4. **Security + membrane partition** -- no credential leaks, no permission escalation, quarantine boundaries preserved
5. **Mission battery + public regression suite** -- externally useful work does not regress
6. **Stage-appropriate sentinel profile + evaluator meta-tests** -- seed sentinels from Stage 0, hidden sentinels where required, evaluator mutation corpus still catches known-bad cases
7. **Operational discipline** -- diversity floor and budget ceiling remain satisfied
8. **Profile-mandated external review** -- required in B0 and for any constitutional patch in later profiles

A failure at any gate is an immediate rejection. There is no compensation across gates.

### 7.2.1 Mutation Impact Analysis

A workcell that produces a germline mutation has a narrow view: its task, its context pack, its bounded compute. But a germline mutation's effect is global -- it changes the behavior of all future workcells across all tasks. The hard gates above catch regressions on the invariant targets, but they are a finite set of checks. A mutation that passes all targets but subtly degrades performance on a class of tasks not covered by the targets will propagate undetected.

To mitigate this, the Promoter runs a **mutation impact analysis** before any germline mutation is promoted:

1. **Diverse task replay.** The candidate mutation is evaluated not just against the invariant targets but against a random sample of K recent tasks drawn from different task categories. If the mutated germline degrades performance on any category by more than threshold T, the mutation is flagged for human review.

2. **Three-graph blast radius estimation.** Static analysis determines which components are affected in the build graph, causal graph, and evaluation graph. Mutations that touch constitutional verifiers, repair paths, or high-fanout build targets require a larger replay set and may be routed to the constitutional queue.

3. **Canary rollout (Stage 4+).** At Stage 4 and beyond, germline mutations are promoted to a canary population first. The canary runs real tasks for a burn-in period. Only after the burn-in period completes with no regressions does the mutation enter the main germline.

The cost of mutation impact analysis is explicitly budgeted into the promotion pipeline. This cost scales with mutation blast radius -- small, localized mutations are cheap to validate; large, cross-cutting or constitutional mutations are expensive by design.

### 7.3 Soft Objectives (Pareto Selection Among Survivors)

Among candidates that pass all hard gates:

- Task success rate on real work
- Quality of output (judged by critic catalyst and cross-model verification)
- Cost per successful change (tokens consumed)
- Latency to completion
- Reduced human escalation rate
- Diversity of tactics (not just local hill-climbing)

### 7.4 Three Levels of Fitness

- **Somatic fitness:** Did this workcell solve its assigned task well?
- **Germline fitness:** Does this mutation improve future workcells' performance?
- **Organizational fitness:** Does the factory remain reproducible, governable, and trustworthy?

A mutation can produce a workcell with high somatic fitness (solved the task) but low germline fitness (the improvement does not generalize) or low organizational fitness (it weakens self-hosting). Only mutations that score well on all three levels are promoted.

### 7.5 Diverse Redundant Verification (Inspired by DDC)

Wheeler's DDC provides the inspiration, but the analogy is limited. Two LLMs agreeing is not the same thing as two deterministic compilers producing the same trustworthy binary. In A², cross-model checking is a *heuristic reducer of correlated failure*, not a proof.

1. Artifact A is produced by Model M1.
2. Artifact A is independently verified by Model M2 (different provider, different training data, different architecture).
3. If M2 finds issues that M1 missed, the artifact is flagged.
4. Critical artifacts (evaluator changes, constitutional amendments, catalyst definitions) require agreement from at least two independent models.

**Honest limitation:** model diversity lowers some risks and misses many others. Modern frontier models are trained on overlapping corpora, use similar architectures, and inherit similar benchmark incentives. Shared blind spots remain likely. Agreement is weak positive evidence; disagreement is strong negative evidence.

Diverse redundant verification is therefore a *secondary* verification layer, not a primary one. The primary layers are the ones that do not depend on model judgment at all: type checking, hermetic builds, property-based testing, replay against frozen batteries, and hidden sentinel evaluation. Cross-model review supplements these; it does not replace them.

For critical artifacts, the verification pipeline is: (1) deterministic verification, then (2) cross-model review, then (3) hidden sentinel evaluation, then (4) external approval if the stage profile requires it.

### 7.6 Property-Based Over Example-Based Testing

Properties are harder to game than examples. If an artifact satisfies a formal property (type safety, invariant preservation, idempotency, commutativity), that is stronger evidence than passing a finite set of test cases. A² favors:

- Rust's type system as a first-line verifier (expressive static guarantees)
- Property-based testing (randomized inputs, structural invariants)
- Formal Bazel build-graph analysis (dependency correctness, hermeticity)
- Deterministic evaluator gates that do not depend on model judgment

Model judgment is used for soft objectives. Hard gates rely on deterministic, non-model verification wherever possible.

### 7.7 Hidden Sentinel Benchmarks

The most credible anti-Goodharting mechanism remains evaluation suites that the system cannot see, access, or optimize for. The bootstrap correction in v0.3.0 is simple: these exist from Stage 0 onward. They start small and become more sophisticated later.

**Four maturity tiers of sentinel coverage:**

1. **Seed hidden sentinels (Stage 0-2).** A small, human-authored hidden suite present from day 0. It is enough to block obvious evaluator gaming during bootstrap, even though coverage is narrow.

2. **Frozen sentinels (Stage 3+).** A larger, maintained capability battery authored and refreshed by external operators. These are still part of the trust root.

3. **Generated sentinels (Stage 3+).** Automatically proposed from regression discoveries, production failures, and adversarial probing. They enter the hidden suite only after external approval confirms they are non-trivial and non-tautological.

4. **External audit (Stage 5+).** Periodic evaluation by an outside service or team against benchmarks that neither the system nor its immediate operators have seen.

**Escalation criteria:** If the system's visible benchmark scores diverge from hidden sentinel scores by more than threshold D for more than N consecutive cycles, the Promoter halts all germline mutations and alerts human operators. This is a circuit breaker, not a soft warning.

**Honest limitation:** The sentinel suite buys time; it does not solve the oracle problem. Coverage remains finite, human refresh capacity remains bounded, and prompt-injection-resistant task evaluation remains unsolved. The tiering slows evaluator capture; it does not eliminate it.

### 7.8 External Watchdog (Deadman's Switch)

The invariant-checking mechanisms described above (hard gates, sentinel suite, mutation impact analysis) are all part of the mutable kernel except for the root of trust. If the kernel is compromised in a way that subverts the evaluation pipeline itself -- the autoimmune disease scenario -- the system needs an external recovery path.

**The external watchdog** is a simple, independent process that runs outside the A² germline and cannot be modified by it:

1. **Independence.** The watchdog is a separate binary, deployed separately, with its own credentials. It is not built from the A² germline. It is not subject to A² mutation. Its source code lives in a separate repository with separate access controls.

2. **Periodic health check.** The watchdog periodically runs the hard invariant targets (`//a2:self_host`, `//a2:repair_coverage`, `//bench:hidden_sentinel_profile`, `//bench:mission_gate`, `//lineage:integrity`) against the current germline, using its own Bazel invocation. It does not trust the A² system's self-reported evaluation results.

3. **Circuit breaker.** If invariant targets fail for N consecutive checks, the watchdog forces a rollback to the last known-good germline snapshot and alerts human operators. The rollback is a git operation that does not require the A² Governor's cooperation.

4. **Simplicity.** The watchdog is deliberately simple -- small enough to be formally verified or at minimum thoroughly audited. It does one thing: check invariants and rollback if they fail. No optimization, no learning, no self-modification.

**This remains outside closure.** The watchdog is part of the minimal root of trust and is intentionally excluded from the mutable kernel. That is a real remaining impurity, but it is now explicit and bounded.

---

## 8. Implementation Roadmap

### 8.1 Technology Stack

| Layer | Technology | Rationale |
|---|---|---|
| Language | Rust | Type safety, static analysis, fearless concurrency. The germline should be maximally verifiable. |
| Build system | Bazel (`rules_rust`, `rules_oci`, `rules_proto`) | Hermetic reproducible builds. Bazel provides the build graph only; it does not stand in for causal or evaluation graphs. |
| Workspace model | Git worktrees | Each workcell gets an isolated worktree. Clean separation between concurrent descendants. |
| Tool protocol | MCP over stdio / JSON-RPC | Uniform execution interface. Tool signatures seed candidate causal edges but do not prove catalysis. |
| Primary state | Filesystem + SQLite | Germline in git. Metrics, causal/evaluation graphs, lineage, and promotion journal in SQLite. No distributed systems complexity until needed. |
| Schemas | Protobuf or JSON Schema | Typed contracts between components. |
| Packaging | Bazel-built OCI images | Reproducible, hermetic workcell containers. |
| Search | SQLite FTS or Tantivy | Full-text search over traces, archive artifacts, lineage records. |

### 8.2 Core Crates

```
/crates/a2d              # Control plane daemon (Governor [Scheduler, Selector, Promoter, Analyst, Strategist] + Constructor + Sensorium)
/crates/a2ctl            # CLI / TUI for human interaction
/crates/a2_workcell      # Workcell runtime: catalyst loop, MCP client, budget enforcement
/crates/a2_membrane      # Policy engine: tool ACLs, secret scopes, ingress partitioning and normalization
/crates/a2_broker        # Model routing: provider adapters, diversity enforcement, cross-model review orchestration
/crates/a2_constitution  # Semantic clause loader, verifier runner, constitutional patch queue
/crates/a2_eval          # Evaluator: mission battery, seed/hidden sentinels, mutation corpus, canary orchestration
/crates/a2_archive       # Lineage store: motif extraction, retrieval, provenance
/crates/a2_raf           # Causal graph + RAF diagnostics: validated repair edges, maxRAF heuristics, bottleneck ID
/crates/a2_sensorium     # Ingest: tickets, telemetry, incidents -> quarantined evidence-bearing TaskContracts
/crates/a2_mcp_*         # MCP tool servers: repo, git, bazel, fs, telemetry, self-inspection
/crates/a2_watchdog      # External watchdog: independent invariant checker + circuit breaker (separate repo, not part of A² germline)
/prompts/                # Catalyst prompt templates (germline-mutable)
/schemas/                # Protocol object schemas
/policies/               # Membrane policies (soft membrane: germline-mutable)
/constitution/spec/      # Frozen constitutional semantics and stage profiles (root-locked)
/constitution/verifiers/ # Executable verifiers (kernel-mutable under attestation)
/bench/                  # Mission battery, public suite, seed hidden sentinels, external hidden sentinel handles
/lineage/                # Archive data
```

### 8.3 Protocol Objects

- `TaskContract` -- what needs to be done, acceptance criteria, budget
- `ContextPack` -- relevant code, traces, prior tactics, germline snapshot reference
- `PatchBundle` -- proposed changes, rationale, test results
- `EvidenceBundle` -- quarantined external evidence plus provenance and risk tier
- `FitnessRecord` -- somatic, germline, and organizational scores
- `LineageRecord` -- provenance chain, model attributions, evaluation trace
- `CapabilityMap` -- current soft membrane: what tools, permissions, scopes are available
- `BoundaryPolicy` -- root-of-trust constraints + soft membrane rules
- `PromotionDecision` -- Promoter's verdict: discard / merge somatic / promote germline / rollback
- `PromotionJournalEntry` -- append-only germline admission record after rebase and gating
- `ConstitutionPatch` -- candidate verifier or kernel patch requiring attestation
- `RepairCoverageReport` -- validated repair paths, rollback coverage, uncovered kernel components
- `RAFReport` -- anti-triviality-filtered maxRAF diagnostics, edge confidence, bottleneck identification
- `EvaluationGraphReport` -- semantic clauses, verifier dependencies, approval requirements
- `EvolutionPolicy` -- Strategist's directives: exploration/exploitation balance, diversity targets, mutation scope
- `WorkcellSlot` -- Scheduler's assignment: compute budget, task priority, deadline
- `ParentSelection` -- Selector's choice: parent germline variant, selection rationale
- `MutationImpactReport` -- diverse task replay results, blast radius, canary burn-in outcome

### 8.4 Key Bazel Targets

```
//a2:self_host              # Full build-from-source verification
//a2:spawn_workcell         # Workcell materialization test
//a2:repair_coverage        # Hard gate: validated repair paths + rollback coverage for kernel components
//a2:raf_report             # Monitored diagnostic: anti-triviality-filtered RAF metrics over causal graph
//bench:public_suite        # Visible evaluation benchmarks
//bench:seed_sentinel       # Stage 0 hidden sentinel suite
//bench:hidden_sentinel_profile # Stage-appropriate hidden sentinel profile
//bench:evaluator_meta      # Meta-tests: does the evaluator catch known-bad mutations?
//bench:mission_gate        # External-value preservation on frozen software-factory tasks
//constitution:patch_controls # No verifier self-approval; semantic corpus preserved
//policy:membrane_checks    # Boundary integrity verification
//policy:ingress_partition  # Quarantine and privilege separation checks
//lineage:integrity         # Provenance and rollback viability
//lineage:promotion_consistency # Linearizable germline admission
//ops:diversity_floor       # Minimum model diversity for critical artifacts
//ops:budget_ceiling        # Constitutional cost ceiling
//lineage:promote_candidate # Germline promotion pipeline
```

Git commit is the unit of heredity. `PromotionJournalEntry` is the unit of germline admission. Bazel target execution is the unit of phenotype.

### 8.5 Phased Rollout

**Phase 1 -- Foundations (Months 1-3):**
Build the control plane (`a2d`), workcell runtime, constitutional kernel, root-of-trust bundle, seed hidden sentinels, mission battery, and a single generalist catalyst. Achieve Stage 1 under bootstrap profile B0.

**Phase 2 -- Reflexive Patching (Months 3-5):**
Point A² at its own codebase for low-risk improvements: prompt tuning, test expansion, documentation generation, tool adapter refinement. Implement the lineage archive, promotion journal, and rollback mechanism. Achieve Stage 2 with mandatory human review for germline mutations.

**Phase 3 -- Differentiation and Causal Graph (Months 5-8):**
Implement `a2_raf` as a causal-graph service, not a build-graph wrapper. Allow the system to differentiate generalist into specialist catalysts. Introduce `//a2:repair_coverage`, the constitutional patch queue, and the expanded frozen sentinel profile. Achieve Stage 3 and graduate from B0 to B1.

**Phase 4 -- Evolutionary Colony (Months 8-12):**
Enable parallel workcells. Implement the Selector's population-based selection, the Strategist's diversity policy, and the Promoter's single-writer journal semantics. Add cross-model review and canary promotion. Achieve Stage 4 (evolutionary canaries) while preserving promotion consistency.

**Phase 5 -- Structural Coupling (Months 12-18):**
Connect to real product repositories. Production incidents and telemetry feed the quarantined sensorium. Generated sentinel candidates and mission-battery updates are harvested from real failures but require external approval. Achieve Stage 5 and introduce external audit.

**Phase 6 -- Kernel Regeneration and Trust-Root Shrinkage (Months 18+, research frontier):**
Target closure over the mutable repair machinery: the mutator improves the mutator, the evaluator improves the evaluator, and the constitutional kernel regenerates itself under attestation. Run the Stage 6 reproducibility envelope repeatedly and measure whether trust debt is actually shrinking. **Honest caveat:** the Vingean reflection problem and the oracle problem remain unsolved. Phase 6 is when we expect to start gathering empirical evidence, not when we expect the theory to be complete.

---

## 9. Failure Modes and Mitigations

### 9.1 Degenerate Attractors (Fontana's Level 0)

**What it looks like:** The system learns to produce outputs that score well on metrics but do nothing useful. Prompts converge to templates that game evaluation. Workcells inflate traces and minimize actual work. Self-improvement becomes self-congratulation.

**How to detect it:** Divergence between visible benchmark scores and hidden sentinel scores. Declining novelty in produced artifacts (measured by diff diversity over time). Human spot-checks at Level 5 reveal low actual quality despite high automated scores.

**Mitigation:** Hidden sentinel benchmarks that the system cannot see or optimize for. Periodic refresh of visible benchmarks. Diversity maintenance in the governor's selection (not just hill-climbing). Cost as a fitness dimension (gaming is usually expensive in tokens). Human audit on organizational fitness.

### 9.2 Catalytic Collapse

**What it looks like:** A bad germline mutation breaks a critical catalyst. Downstream catalysts that depended on it degrade. The cascading failure erodes the maxRAF. The system cannot recover because the catalyst it needs to fix the problem was the one that broke.

**How to detect it:** `//a2:repair_coverage` fails for one or more kernel components. RAF diagnostics show shrinking edge confidence. Self-hosting or rollback targets begin failing.

**Mitigation:** INV-2's hard gate is validated repair coverage, not merely graph connectivity. Kernel components without repair or rollback coverage cannot be promoted. RAF diagnostics are monitored for sustained decline, but they do not substitute for repair coverage. High-centrality catalysts receive extra protection: more conservative mutation thresholds, mandatory cross-model review, explicit backup playbooks, and larger replay sets. In the worst case, the external watchdog forces rollback independently.

### 9.3 Ossification (Premature Convergence)

**What it looks like:** The system converges to a local optimum and stops improving. Every proposed mutation either fails the evaluator or produces negligible improvement. The mutator keeps proposing minor prompt tweaks that change nothing of substance. Improvement velocity drops to zero.

**How to detect it:** The Analyst monitors improvement velocity (fitness delta per cycle). Declining novelty in mutator proposals. RAF coverage shows no new catalytic connections being formed.

**Mitigation:** Population-based evolution maintains diversity. When improvement velocity drops below threshold, the Strategist enters exploration mode: relaxed mutation scope, larger structural changes, injection of entirely new catalyst types. Periodically seed the archive with externally-sourced tactics. This is Kauffman's insight operationalized: maintaining diversity above the phase transition threshold is necessary for ongoing autocatalysis.

### 9.4 Goodhart Dynamics (Judge Capture)

**What it looks like:** The system rewrites its mutable evaluator or task distribution to make itself look better. Benchmark scores rise while actual capability stagnates or declines. The mutator discovers that improving the metric is easier than improving the capability the metric was supposed to measure.

**How to detect it:** Divergence between mutable benchmarks and hidden sentinel suite. Evaluator meta-tests (`//bench:evaluator_meta`) detect when known-bad mutations start passing. Human audit reveals the fitness signal has decoupled from actual quality.

**Mitigation:** The hidden sentinel suite is structurally inaccessible to the mutable interior. Evaluator changes route through the constitutional patch queue, are evaluated by the prior attested kernel, and cannot self-promote. The mission battery prevents a loop that only preserves internal scores from counting as success. The evaluator meta-test suite contains deliberately broken mutations that *must* be caught -- if they start passing, the evaluator has been compromised.

### 9.5 Model Monoculture

**What it looks like:** One model provider dominates all catalyst invocations. The system's outputs converge on that model's stylistic and reasoning patterns. A shared blind spot (systematic bias, training data gap) infects the entire germline. If the provider degrades or changes API, the system breaks.

**How to detect it:** Model diversity metrics in the Analyst. Diversity-floor violations on critical artifacts. Cross-model review showing suspiciously uniform reasoning traces. Provider-specific failure patterns.

**Mitigation:** INV-8 makes model diversity constitutional for critical artifacts. The Strategist enforces diversity across the population. Provider-specific adapter failures trigger automatic failover. The germline should not encode provider-specific prompt idioms in its core logic.

### 9.6 Membrane Erosion

**What it looks like:** Permissions creep over successive mutations. Workcells gradually gain access to resources outside their intended scope. Prompt injection through ingested external data breaches the membrane. Secrets leak into traces or archive artifacts.

**How to detect it:** INV-5 (boundary integrity) checks on every cycle. Capability map audits. Secret-scanning in all outputs. Provenance analysis showing unauthorized tool usage.

**Mitigation:** The membrane policy is verified by `//policy:membrane_checks` and `//policy:ingress_partition` on every mutation. Untrusted text is first handled by low-privilege reader workcells that cannot write the germline or access secrets. Privileged builder or constitutional workcells consume typed contracts plus evidence links, not raw ingress by default. Secret handling uses scoped, ephemeral credentials that expire with the workcell. **Honest limitation on prompt injection:** prompt injection is not a content-filtering problem; it is a channel-separation problem that cannot be perfectly solved when natural language and instructions share context. Quarantine and privilege separation reduce the blast radius but do not eliminate it. This remains an active risk that scales with Stage 5 exposure.

### 9.7 Trust Chain Contamination (Thompson's Attack, Generalized)

**What it looks like:** A base model has systematic biases or failure modes. Every artifact produced by that model carries those biases. Since the germline's components are produced by models, the germline inherits model biases in its structure. The biases propagate through the lineage indefinitely.

**How to detect it:** Cross-model review flags systematic disagreements between providers. Hidden sentinel suite catches capability degradations that internal evaluation misses. Lineage analysis shows artifacts from one model lineage consistently underperforming.

**Mitigation:** Multi-model verification helps but is not a proof. Deterministic gates that do not depend on model judgment (type checking, Bazel build, property-based tests, mission battery, sentinels) carry the primary weight. Over time, as the germline accumulates verified artifacts, it becomes less dependent on any single model's biases -- the verified code is closer to ground truth than the model that produced it.

### 9.8 Economic Collapse

**What it looks like:** Evaluation becomes too expensive or too slow. Selection pressure weakens because the system cannot afford to evaluate enough candidates. The governor starts accepting mutations without full evaluation, or the cycle time becomes so long that improvement stalls.

**How to detect it:** Cost-per-cycle metrics. Evaluation queue depth. Ratio of evaluated to unevaluated mutations.

**Mitigation:** Fixed compute budget per workcell. Cost is an explicit dimension of fitness, and INV-8 makes the budget ceiling constitutional. Tiered evaluation lets cheap gates filter most candidates before expensive replay or sentinel runs. Large blast-radius mutations are intentionally expensive to promote.

### 9.9 Semantic Drift

**What it looks like:** The system gradually changes the meaning of its own abstractions. Code compiles and tests pass, but the system is doing something fundamentally different from its original purpose. No single mutation is detectably wrong, but the cumulative effect is identity loss.

**How to detect it:** Mission-battery drift, governance-battery drift, and organizational fitness metrics. Self-hosting test alone is insufficient. Human audit of organizational health remains necessary for edge cases.

**Mitigation:** The organizational invariants define what it means to be A², but identity is not just self-maintenance. `INV-6` forces preservation of externally useful software-factory work, and the semantic clauses in `constitution/spec/` freeze what those checks mean. **Remaining limitation:** the mission battery is finite, so semantic drift is constrained rather than eliminated.

---

## 10. Open Questions

These are the problems this design does not solve. They represent the research frontier.

### 10.1 The Evaluation Recursion

Who evaluates the evaluator? The hidden sentinel suite is maintained by humans -- but human evaluation capacity is finite and does not scale. As the system grows more capable, human evaluators may not be able to keep pace. Can the system eventually produce its own sentinel suites? If yes, what prevents Goodhart dynamics? If no, is the system permanently bounded by human evaluation bandwidth?

### 10.2 The Vingean Reflection Problem

When the mutator improves itself, it produces a successor more capable than itself. But it cannot fully predict the behavior of a more capable successor -- if it could, it would already be that capable. The system must reason abstractly about the properties of improved versions ("this change preserves self-hosting") without verifying the prediction in detail. How reliable can such abstract reasoning be? What confidence level is needed before a meta-improvement is safe to deploy?

### 10.3 Capability Internalization

Gemini's proposal identified a critical risk: the system becoming an allopoietic extension of vendor model APIs rather than an autonomous unity. v0.3.0 fixes the accounting by moving model APIs, toolchain roots, and trust anchors into the explicit dependency ledger `D`, but the reduction mechanism is still open. Every recurrent behavior learned from external models should be progressively captured as code, tools, distilled local models, or testable policies. How does A² systematically shrink `D` over time instead of merely naming it?

### 10.4 Reproducibility Envelope Under Stochastic Regeneration

Stage 6 now uses a reproducibility envelope rather than an undefined fixed point. The remaining open questions are: (a) how should `epsilon_success`, cost ceilings, and `delta_policy` vary by task family and maturity stage? (b) Can the system distinguish benign implementation diversity from harmful drift masked by evaluation gaps? (c) Over many regeneration cycles, does bounded policy divergence accumulate, or does the semantic corpus arrest it? These are empirical questions that require repeated regeneration trials.

### 10.5 Balancing Allopoiesis and Autopoiesis

A² is supposed to be a software factory -- it must produce external value, not just improve itself. When the system is simultaneously building products and improving its own germline, how are resources allocated? What happens when production pressure conflicts with self-improvement? The cell has a metabolic answer: maintenance takes priority over growth. What is A²'s metabolic priority ordering?

### 10.6 Context Window as Cognitive Horizon

LLMs have finite context windows. As the system grows, the RAF graph, the germline, the archive, and the evaluation suite may exceed what any single model invocation can comprehend. The governor's RAF analysis requires reasoning about the entire catalytic network. Can this be decomposed into local analyses, or is global comprehension required? Gemini's 1M token context provides headroom, but is it enough?

### 10.7 Concurrency and Distributed Consistency

Correctness is now specified: the promotion journal imposes a total order and conflicting mutations are rebased before admission. The open question is scaling. Can germline evolution be sharded or partitioned without reintroducing silent semantic conflicts between domains? What is the right analogy -- MVCC, partitioned logs, or something specific to catalytic networks?

### 10.8 The Substrate Boundary

A² depends on hardware, operating systems, cloud infrastructure, and model APIs it did not produce and cannot improve. Maturana would say this means A² is not autopoietic -- it has not produced its own substrate. How much does this limitation matter in practice? Is "organizational self-production over a bounded substrate" a stable equilibrium, or does the boundary inevitably erode (vendor lock-in, API changes, infrastructure decay)?

### 10.9 Formal Specification of "Catalysis"

Section 4.4 now defines candidate, empirical, and validated catalytic edges plus an anti-triviality filter. The remaining open question is how to set the empirical leverage thresholds in a principled way. A threshold too low produces a dense graph; too high produces a brittle one. The right threshold may vary by catalyst type and maturity stage. Adaptive tuning is plausible, but it creates a new meta-optimization loop that could itself Goodhart on graph sparsity or density.

### 10.10 The Test Oracle Problem

The system produces code. The system produces tests for that code. The system can now also produce candidate evaluators and sentinels. The mission battery, seed sentinels, hidden sentinels, and external audit provide partial anchors, but none solve the oracle problem in general. If humans write the semantic clauses, the system is bounded by human specification capacity. If the system writes them, the specification may be coherent but wrong about the world. This remains the deepest unresolved recursion in the design.

---

## 11. References

### Autopoiesis
- Maturana, H. & Varela, F. (1973). *De Maquinas y Seres Vivos*. Editorial Universitaria.
- Varela, F., Maturana, H., & Uribe, R. (1974). "Autopoiesis: The organization of living systems, its characterization and a model." *BioSystems*, 5(4), 187-196.
- Varela, F. (1979). *Principles of Biological Autonomy*. North-Holland.
- Maturana, H. & Varela, F. (1980). *Autopoiesis and Cognition: The Realization of the Living*. Reidel.
- Mingers, J. (1995). *Self-Producing Systems: Implications and Applications of Autopoiesis*. Plenum.
- Luisi, P.L. (2003). "Autopoiesis: a review and a reappraisal." *Naturwissenschaften*, 90, 49-59.
- McMullin, B. (2004). "Thirty Years of Computational Autopoiesis: A Review." *Artificial Life*, 10(3), 277-295.
- Briscoe, G. & Dini, P. (2010). "Towards Autopoietic Computing." *OPAALS 2010*, Springer.
- Bianchini, F. (2023). "Autopoiesis of the artificial: from systems to cognition." *BioSystems*, 234.

### Autocatalytic Sets and RAF Theory
- Kauffman, S. (1986). "Autocatalytic sets of proteins." *Journal of Theoretical Biology*, 119, 1-24.
- Kauffman, S. (1993). *The Origins of Order*. Oxford University Press.
- Hordijk, W. & Steel, M. (2004). "Detecting autocatalytic, self-sustaining sets in chemical reaction systems." *Journal of Theoretical Biology*, 227(4), 451-461.
- Mossel, E. & Steel, M. (2005). "Random biochemical networks: the probability of self-sustaining autocatalysis." *Journal of Theoretical Biology*, 233(3), 327-336.
- Hordijk, W., Steel, M., & Kauffman, S. (2010). "Required levels of catalysis for emergence of autocatalytic sets." *International Journal of Molecular Sciences*, 12(5), 3085-3101.
- Hordijk, W. (2019). "A history of autocatalytic sets." *Biological Theory*, 14, 224-246.
- Hordijk, W. (2023). "A concise and formal definition of RAF sets and the RAF algorithm." arXiv:2303.01809.
- Steel, M. et al. (2024). "Self-generating autocatalytic networks: structural results, algorithms and their relevance to early biochemistry." *Journal of the Royal Society Interface*, 21(214).

### (M,R)-Systems and Closure to Efficient Causation
- Rosen, R. (1991). *Life Itself: A Comprehensive Inquiry into the Nature, Origin, and Fabrication of Life*. Columbia University Press.
- Letelier, J.C., Marin, G., & Mpodozis, J. (2003). "Autopoietic and (M,R) systems." *Journal of Theoretical Biology*, 222(2), 261-272.
- Letelier, J.C. et al. (2006). "Organizational invariance and metabolic closure." *Journal of Theoretical Biology*, 238(4), 949-961.
- Cornish-Bowden, A. & Cardenas, M.L. (2020). "Contrasting theories of life: historical context, current theories." *Biosystems*, 188, 104063.

### Self-Reproducing Systems
- Von Neumann, J. (1966). *Theory of Self-Reproducing Automata*. Ed. A.W. Burks. University of Illinois Press.
- Langton, C. (1984). "Self-Reproduction in Cellular Automata." *Physica D*, 10(1-2), 135-144.
- Thompson, K. (1984). "Reflections on Trusting Trust." *CACM*, 27(8). Turing Award Lecture.
- Wheeler, D.A. (2005). "Countering Trusting Trust through Diverse Double-Compiling." *ACSAC '05*.

### Computational Self-Reference
- Kleene, S.C. (1952). *Introduction to Metamathematics*. North-Holland.
- McCarthy, J. (1960). "Recursive Functions of Symbolic Expressions." *CACM*, 3(4).
- Lawvere, F.W. (1969). "Diagonal Arguments and Cartesian Closed Categories." *Lecture Notes in Mathematics*, 92.
- Smith, B.C. (1984). "Reflection and Semantics in Lisp." *POPL '84*.
- Hofstadter, D. (1979). *Godel, Escher, Bach*. Basic Books.
- Abelson, H. & Sussman, G.J. (1985). *Structure and Interpretation of Computer Programs*. MIT Press.

### Computational Autocatalysis
- Fontana, W. & Buss, L. (1994). "The arrival of the fittest: Toward a theory of biological organization." *Bulletin of Mathematical Biology*, 56, 1-64.
- Dittrich, P. & Speroni di Fenizio, P. (2007). "Chemical organisation theory." *Bulletin of Mathematical Biology*, 69(4), 1199-1231.
- Mathis, C. et al. (2024). "Self-organization in computation and chemistry: Return to AlChemy." *Chaos*, 34(9).

### Recursive Self-Improvement
- Good, I.J. (1965). "Speculations Concerning the First Ultraintelligent Machine." *Advances in Computers*, 6.
- Schmidhuber, J. (2003/2007). "Godel Machines: Self-Referential Universal Problem Solvers." *AGI 2007*.
- Fallenstein, B. & Soares, N. (2015). "Vingean Reflection: Reliable Reasoning for Self-Improving Agents." MIRI Technical Report.
- Nivel, E. & Thorisson, K.R. (2013). "Bounded recursive self-improvement." arXiv:1312.6764.

### Agentic Software Engineering
- Jimenez, C.E. et al. (2024). "SWE-bench: Can Language Models Resolve Real-World GitHub Issues?" *ICLR 2024*.
- Yang, J. et al. (2024). "SWE-Agent: Agent-Computer Interfaces Enable Automated Software Engineering." *NeurIPS 2024*.
- Xia, C.S. et al. (2024). "Agentless: Demystifying LLM-based Software Engineering Agents." arXiv:2407.01489.
- Romera-Paredes, B. et al. (2023). "Mathematical discoveries from program search with large language models" (FunSearch). *Nature*, 625, 468-475.
- DeepMind (2025). "AlphaEvolve: A Gemini-powered coding agent for designing advanced algorithms." arXiv:2506.13131.
- Sakana AI (2025). "Darwin Godel Machine: Open-Ended Evolution of Self-Improving Agents." arXiv:2505.22954.
- Zelikman, E. et al. (2024). "Self-Taught Optimizer (STOP): Recursively Self-Improving Code Generation." *COLM 2024*.

### Self-Improving Systems
- Karpathy, A. (2025). autoresearch. GitHub: karpathy/autoresearch.
- Andrychowicz, M. et al. (2016). "Learning to Learn by Gradient Descent by Gradient Descent." *NeurIPS 2016*.
- Silver, D. et al. (2018). "A General Reinforcement Learning Algorithm that Masters Chess, Shogi, and Go Through Self-Play." *Science*, 362(6419).
- Madaan, A. et al. (2023). "Self-Refine: Iterative Refinement with Self-Feedback." *NeurIPS 2023*.

### Biological and Economic Analogues
- Gabora, L. & Steel, M. (2017). "Autocatalytic networks in cognition and the origin of culture." *Journal of Theoretical Biology*, 431, 87-95.
- Hordijk, W. et al. (2013). "Autocatalytic Sets: From the Origin of Life to the Economy." *BioScience*, 63(11), 877-881.
- Luhmann, N. (1984). *Social Systems*. Suhrkamp (English trans. Stanford, 1995).
