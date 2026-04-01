# A² — Autopoietic Autocatalysis: Architecture Proposal

**Author:** Claude Opus 4.6
**Date:** 2026-04-01
**Status:** Divergent exploration draft

---

## 1. Core Metaphor: The Metabolic Compiler

I draw primarily from **Rosen's (M,R)-systems** and **Kauffman/Hordijk-Steel RAF theory**, with autopoiesis as the aspirational organizational target rather than the starting framework.

Here is why.

Autopoiesis is the right *destination* but the wrong *starting point*. Maturana's framework describes what a fully realized self-producing system looks like — operational closure, self-generated boundaries, constitutive circularity — but it gives you almost no purchase on how to *build one*. It is a recognition criterion, not a construction manual. Worse, its insistence on physical substrate production makes direct application to software inherently analogical.

RAF theory, by contrast, is *constructive*. It gives you a polynomial-time algorithm for detecting whether your system has achieved catalytic closure. It tells you the minimal conditions for emergence (linear catalytic connectivity suffices). It gives you a decomposition into irreducible cores (irrRAFs) that tells you what the minimal viable self-sustaining unit looks like. And Rosen's (M,R)-systems tell you what you need *beyond* catalytic closure: closure to efficient causation — meaning the system must produce not just its components but its *builders*.

The core metaphor is therefore **the metabolic compiler**: a system that compiles itself in the way a cell metabolizes itself. Not a compiler in the narrow sense (source to binary), but in the Rosennean sense: a network of transformations where every transformation's efficient cause is produced by other transformations in the network. The cell does not have a "build step" separate from its "run step." Metabolism *is* construction *is* operation. A² aims for the same: the running system is the building system is the improving system.

The name "metabolic compiler" also captures the von Neumann duality that pervades the research: the system's artifacts serve dual roles — interpreted as instructions (compiled, executed) and copied as data (versioned, transmitted, replicated). This is the constructor/description duality that makes open-ended self-production possible without infinite regress.

---

## 2. Components

A² consists of seven named components. Each is both a product of the system and a producer within it. I use biological names deliberately — not as decoration, but because the functional correspondences are precise.

### 2.1 The Genome — `a2::genome`

The **complete, versioned specification** of the system: all source code, configuration, build rules, agent definitions, prompt templates, evaluation criteria, and organizational policies. Stored as a Bazel-managed monorepo. The genome is the von Neumann Description D — it is both interpreted (built, executed) and copied (versioned, forked, transmitted).

Unlike biological DNA, the genome is not a linear sequence but a directed acyclic graph of Bazel targets with explicit dependency edges. This is important: it makes the "what depends on what" structure machine-readable, which is prerequisite for the system reasoning about its own construction.

### 2.2 The Ribosome — `a2::ribosome`

The **build and deployment pipeline** that reads the genome and produces running instances of all other components. Concretely: Bazel build rules, container image construction, deployment manifests, and the orchestration logic that takes a genome revision and produces a running A² instance. The ribosome is the Universal Constructor A.

The ribosome must be *in* the genome (it is source code, after all), but it also *reads* the genome to produce everything else. This is the first circularity: the build system builds itself.

### 2.3 The Membrane — `a2::membrane`

The **self-produced boundary** of the system. This is the component that makes A² autopoietic rather than merely autocatalytic. The membrane consists of:

- **Ingress policies**: what external inputs (issues, requests, events) the system accepts and how they are transformed into internal perturbations
- **Egress policies**: what artifacts (PRs, releases, reports) the system emits and under what conditions
- **Access controls**: authentication, authorization, rate limiting — all generated from internal policy definitions
- **API surface**: the interfaces through which the system interacts with its environment (GitHub, CI/CD, artifact registries, monitoring systems)

Crucially, the membrane is *produced by the system's own processes*. It is not a static configuration imposed by an operator. The system generates its own network policies, its own API schemas, its own access control rules, as outputs of its internal reasoning. When the system decides it needs a new interaction channel (say, a Slack integration), it produces the membrane component that enables it.

### 2.4 The Enzymes — `a2::enzymes`

The **catalog of specialized agent capabilities**. Each enzyme is a defined agent configuration: a prompt template, a tool set, a model binding, and an evaluation criterion. Enzymes are the molecule types in the RAF formalization.

Examples:
- `enzyme::architect` — reads requirements, produces design documents and Bazel BUILD file skeletons
- `enzyme::coder` — reads designs and existing code, produces implementations
- `enzyme::reviewer` — reads code changes, produces review verdicts with specific accept/reject criteria
- `enzyme::tester` — reads implementations, produces test suites
- `enzyme::debugger` — reads failing tests and logs, produces fixes
- `enzyme::refactorer` — reads code quality metrics, produces refactoring changes
- `enzyme::evolver` — reads enzyme definitions and performance data, produces improved enzyme definitions
- `enzyme::auditor` — reads the full system state, produces health/closure reports

The enzyme catalog is stored in the genome and is itself subject to modification by the system.

### 2.5 The Metabolism — `a2::metabolism`

The **runtime orchestration layer** that decides which enzymes to activate, on what substrates, in what order. This is the process network — the "reactions" in RAF terms. The metabolism:

1. Reads the current system state (pending tasks, failing tests, quality metrics, enzyme performance data)
2. Selects and parameterizes enzyme invocations
3. Executes them (calling out to LLM providers)
4. Routes outputs to appropriate destinations (commit to genome, update metrics, trigger further reactions)
5. Maintains the reaction graph — which enzyme invocations produced which artifacts

The metabolism is itself an enzyme (it is source code in the genome, executed by the ribosome, improvable by the evolver). This is the second circularity: the orchestrator is orchestrated.

### 2.6 The Immune System — `a2::immune`

The **verification, testing, and regression detection layer**. Every change to the genome passes through the immune system before integration. The immune system consists of:

- **Static analysis gates**: type checking, linting, format verification (fast, cheap, deterministic)
- **Test suites**: unit, integration, and property-based tests (medium cost, high signal)
- **Evaluation benchmarks**: task-specific performance measurements for each enzyme (expensive, essential)
- **Regression detectors**: comparison of current performance against historical baselines
- **Invariant monitors**: checks that organizational invariants (see Section 6) are maintained

The immune system is the selection pressure that prevents degenerate attractors (Fontana's Level 0 problem). Without it, the system collapses to trivial self-copiers or drifts into incoherence.

### 2.7 The Reflective Cortex — `a2::cortex`

The **self-model and meta-reasoning layer**. Following AERA's principle that reflectivity is necessary for purposeful self-improvement, the cortex maintains:

- A map of the current RAF structure: which enzymes catalyze the production of which other enzymes
- Closure analysis: which enzymes lack internal catalysts (orphan capabilities)
- Performance attribution: which enzyme improvements caused which downstream improvements
- Bottleneck identification: where the catalytic network is thinnest
- Goal management: what the system is trying to achieve and how that maps to enzyme activations

The cortex is what enables the system to reason about *where* to improve, not just *how*. It is the difference between random mutation and directed search.

---

## 3. The Autocatalytic Loop

### 3.1 What Catalyzes What

The catalytic relationships form a dense, directed graph. Here are the essential cycles:

**The inner loop (code production):**
```
architect --catalyzes--> coder --catalyzes--> tester --catalyzes--> debugger --catalyzes--> coder
```
This is the basic software production cycle. It is *not* autocatalytic by itself — it produces application code, not itself.

**The self-production loop (what makes it autocatalytic):**
```
evolver --reads enzyme performance data--> produces improved enzyme definitions
improved enzymes --produce better code/tests/reviews--> better performance data
better performance data --feeds--> evolver (with more signal)
```
The evolver enzyme modifies the definitions of other enzymes (including, critically, itself). This is the Rosennean repair function Phi: it uses system outputs (performance data) to regenerate system components (enzyme definitions).

**The meta-loop (closure to efficient causation):**
```
cortex --identifies bottlenecks in catalytic network--> directs evolver
evolver --improves enzymes including cortex--> better bottleneck identification
better identification --enables more targeted evolution--> faster improvement
```
This is the f -> beta -> Phi -> f loop from Rosen. The cortex produces the conditions for its own improvement by identifying where improvement is most needed. The evolver acts on those identifications to improve the cortex itself.

**The infrastructure loop (self-produced boundary):**
```
metabolism --observes resource usage, failure modes--> produces infrastructure changes
ribosome --builds updated infrastructure--> better execution environment
better environment --enables more/better enzyme invocations--> improved metabolism
```

### 3.2 The RAF Formalization

Formally, A² is a Catalytic Reaction System Q = (X, R, C, F) where:

- **X** = {all source files, configurations, prompt templates, evaluation results, metrics} — the set of all artifact types
- **R** = {all enzyme invocations} — each enzyme invocation is a reaction that transforms input artifacts into output artifacts
- **C** = {catalysis assignments} — enzyme E catalyzes reaction R if E's outputs are required for R's inputs. For example, `enzyme::tester` catalyzes the reaction "debugger fixes code" because the tester produces the failing test that tells the debugger what to fix
- **F** = {base models (Claude/Gemini/Codex), compute resources, external libraries, human-provided objectives} — the food set

The system achieves RAF closure when: every enzyme invocation has at least one catalyst produced by another enzyme invocation, and every artifact can be traced back to the food set through a chain of enzyme invocations.

### 3.3 What "Catalysis" Means Concretely

In chemistry, a catalyst accelerates a reaction without being consumed. In A², catalysis means: **an artifact produced by one enzyme enables or dramatically improves the performance of another enzyme's invocation**. Examples:

- A design document *catalyzes* implementation (the coder produces better code with a design than without)
- A test suite *catalyzes* debugging (the debugger needs failing tests to know what to fix)
- A performance report *catalyzes* evolution (the evolver needs metrics to know what to improve)
- A code review *catalyzes* the next coding attempt (the coder learns from review feedback)

This is catalysis, not mere input. The coder *could* produce code without a design document (from a bare requirement), but the design document dramatically improves the quality and speed of production. Catalysis is the difference between a reaction that barely proceeds and one that proceeds efficiently.

---

## 4. Bootstrap Sequence

The bootstrap follows the compiler bootstrap pattern: an external seed breaks the initial circularity, then the system self-compiles until the seed is no longer needed.

### Stage 0 — The Seed (Human-authored)

A human writes the minimal A² genome:
1. The Bazel workspace and build rules for the system itself (`a2::ribosome` v0)
2. A hardcoded enzyme catalog with hand-written prompt templates for the core enzymes: coder, tester, reviewer, evolver (`a2::enzymes` v0)
3. A simple sequential orchestrator: pull task, invoke coder, invoke tester, invoke reviewer, commit if passes (`a2::metabolism` v0)
4. Basic CI gates: type checks, tests, linting (`a2::immune` v0)
5. A minimal membrane: a GitHub integration that reads issues and produces PRs (`a2::membrane` v0)
6. A trivial cortex: logs enzyme invocations and their outcomes, no analysis (`a2::cortex` v0)

This seed is crude. The prompt templates are hand-tuned. The orchestration is a fixed pipeline. There is no self-modification yet. **This is the Stage 0 seed compiler, written in "language M" (human judgment).**

### Stage 1 — First Self-Modification

The seed system is given its first task: **improve one of its own enzyme definitions**. Specifically, the evolver enzyme is pointed at the coder enzyme's prompt template and told: "Here is the coder enzyme's current prompt, here are its recent performance metrics (% of tests passing on first attempt, review acceptance rate). Propose an improved prompt."

If the improved prompt scores better on the evaluation benchmark, it is committed to the genome. The system has now modified one of its own components. This is the first self-catalyzed reaction.

### Stage 2 — Catalytic Closure Check

After enough Stage 1 iterations (the evolver improves each enzyme in turn), we run the RAF detection algorithm on the realized reaction graph. The cortex examines: for every enzyme invocation that has occurred, was the catalyst (the enabling artifact) produced by another enzyme invocation? If any enzyme is only catalyzed by human-authored artifacts (the original seed), it has not yet been "food-generated" through the autocatalytic network.

The system is RAF-closed when every enzyme's enabling artifacts have been produced (or improved) by other enzymes. The seed artifacts are now part of the food set F — historical inputs that kickstarted the process but are no longer needed for ongoing operation.

### Stage 3 — Second Self-Compilation (Fixed Point)

The ultimate bootstrap test: the system is asked to regenerate its *entire* genome from scratch, using only the food set (base models + compute + objectives) and its current enzyme definitions. If the regenerated genome produces a system with equivalent or better performance, the bootstrap is complete.

This is analogous to the compiler bootstrap verification: compile the compiler with itself, then compile it again — the output should be a fixed point. For A², the fixed point is: a system that, given only its food set, can reproduce a functionally equivalent version of itself.

### Practical Minimalism

The seed need not be large. The research shows that RAF sets emerge at surprisingly low catalytic connectivity thresholds (1-2 catalyzed reactions per molecule type). The minimal irrRAF — the smallest self-sustaining core — might be as few as 3-4 enzymes in a tight cycle:

```
evolver -> improved coder -> better code -> better test results -> evolver (with better signal)
```

Start with this irrRAF. Grow outward only after the core cycle is self-sustaining.

---

## 5. Self-Production: What "Component" Means

### 5.1 The Three Layers of Self-Production

A² self-produces at three distinct levels, each corresponding to a different sense of "component":

**Layer 1 — Artifact production (allopoietic).**
The system produces software artifacts: application code, tests, documentation, infrastructure configurations. These are outputs *for the environment* — they cross the membrane. This is allopoietic production (producing something other than itself), analogous to a cell secreting enzymes into its environment. Important, but not what makes the system autopoietic.

**Layer 2 — Component production (autopoietic).**
The system produces its own operational components: enzyme definitions (prompt templates, tool configurations, model bindings), orchestration logic, evaluation criteria, membrane policies, build rules. These are the "molecules" of the autopoietic network. When the evolver improves a prompt template, when the cortex refines a bottleneck-detection heuristic, when the metabolism adjusts its scheduling policy — the system is producing the components that constitute itself.

**Layer 3 — Process production (closure to efficient causation).**
The system produces the *processes* that produce its components. When the evolver improves its own evolution strategy (meta-evolution), when the cortex improves its own self-modeling capability, when the immune system generates new types of tests for itself — the system is producing its own builders. This is Rosen's closure to efficient causation: the deepest and hardest form of self-production.

### 5.2 What Counts as a "Component"

A component of A² is any artifact that satisfies *both*:
1. It is a product of some enzyme invocation within the system
2. It is a necessary input (reactant or catalyst) for some other enzyme invocation within the system

Artifacts that are only products (emitted through the membrane, never consumed internally) are allopoietic outputs. Artifacts that are only inputs (from the food set, never produced internally) are environmental resources. Only artifacts that are *both* produced and consumed by the network are autopoietic components.

The genome is the canonical component: it is produced by the system (every commit is the output of an enzyme invocation) and consumed by the system (the ribosome reads it to build everything else).

---

## 6. Boundary and Identity

### 6.1 Organization: The Invariant

A²'s **organization** — the pattern that defines its identity — is:

> A network of LLM-mediated transformations where (1) every transformation's enablement is produced by some other transformation in the network (catalytic closure), (2) the network produces and maintains its own interface to its environment (boundary production), and (3) the network produces the processes that produce and select transformations (closure to efficient causation).

This is the autopoietic organization expressed in computational terms. It is invariant: as long as these three properties hold, the system is A². If any of them fails — if catalytic closure is lost (an enzyme can only function with human-authored artifacts), if the boundary becomes externally imposed (an operator manually configures the membrane), if closure to efficient causation is broken (the system cannot improve its own improvement process) — the system has lost its A² identity.

### 6.2 Structure: The Variable

The structure — the specific enzyme definitions, prompt templates, model bindings, evaluation criteria, orchestration logic, and code — changes continuously. Every genome commit is a structural change. The system can replace every single line of code in its codebase (and, over time, will) while remaining A² — just as a cell replaces every molecule while remaining the same cell.

### 6.3 How the Boundary is Maintained

The membrane is self-produced but not arbitrary. It is constrained by **organizational invariants** — conditions that the immune system checks on every proposed change:

1. **Catalytic closure invariant**: The RAF detection algorithm must find a non-empty maxRAF after the change. If a proposed change would break catalytic closure (e.g., deleting an enzyme that is the sole catalyst for another enzyme's production), the change is rejected.

2. **Boundary integrity invariant**: The membrane must remain self-produced. If a change would require manual configuration of any interface component, it violates boundary integrity.

3. **Closure-to-efficient-causation invariant**: The evolver and cortex must remain functional and self-improvable. If a change would render the system unable to improve its own improvement process, it is rejected.

4. **Performance monotonicity invariant**: The system's aggregate performance on its evaluation suite must not regress beyond a configurable threshold. This prevents catastrophic self-modification.

These invariants are the computational analogue of the autopoietic organization. They are the *only* things that cannot change. Everything else — every line of code, every prompt word, every model choice — is structure, and structure is free to vary.

### 6.4 Structural Coupling with the Environment

A² does not receive "instructions" from its environment. Following Maturana's perturbation principle, external events (GitHub issues, monitoring alerts, human messages) are *perturbations* that trigger internal state changes, but the system's own structure determines what those changes are. The same issue description might lead to completely different actions depending on the system's current state (which enzymes are most capable, what the cortex identifies as the bottleneck, what the immune system considers safe).

This is not mere philosophical framing — it has concrete design consequences. The membrane does not pass raw external requests to the metabolism. It transforms them into internal representations using the system's own vocabulary (its current ontology of task types, priority levels, risk assessments). The system's response to any perturbation is determined by its own structure, not by the perturbation's content.

---

## 7. Verification and Selection

### 7.1 The Fitness Signal Stack

A² uses a layered fitness signal, from most local/fast to most global/slow:

**Level 0 — Syntactic validity (milliseconds).** Does the produced artifact parse? Type-check? Pass linting? This is the cheapest gate and catches the most obvious failures. Implemented as Bazel build and check targets.

**Level 1 — Functional correctness (seconds to minutes).** Does the produced code pass its test suite? Do the tests themselves satisfy coverage and property-based criteria? Does the enzyme produce output that conforms to its specified schema? Implemented as Bazel test targets.

**Level 2 — Enzyme-level performance (minutes to hours).** How does the enzyme perform on its evaluation benchmark compared to the previous version? Metrics: task success rate, output quality (judged by the reviewer enzyme or a separate evaluation model), cost (tokens consumed), latency. Implemented as periodic benchmark runs.

**Level 3 — System-level performance (hours to days).** How does the whole system perform on end-to-end tasks? Can it resolve issues from its task queue? How do its outputs compare to human-produced solutions? Implemented as integration-level evaluation suites.

**Level 4 — Organizational health (days to weeks).** Is catalytic closure maintained? Is the maxRAF growing or shrinking? Are there orphan enzymes? Is the immune system catching real regressions? Is the cortex's self-model accurate? Implemented as the cortex's periodic health reports.

**Level 5 — Evolutionary fitness (weeks to months).** Is the system producing genuinely novel capabilities? Is it solving problems it could not solve before? Is it reducing its dependence on the food set (needing fewer human interventions, using base models more efficiently)? This is the ultimate measure, and it is the hardest to automate. Initially, this level requires human judgment.

### 7.2 Selection Mechanisms

**Greedy with reversion.** Following Karpathy's autoresearch pattern: try a change, measure performance, keep if improved, revert if not. Simple, robust, monotonically improving. This is the default for enzyme-level changes.

**Population-based.** Following AlphaEvolve/FunSearch: maintain a diverse population of enzyme variants, evaluate in parallel, select the best, use the population as a source of recombination. This is for exploring larger changes (new enzyme types, architectural modifications).

**RAF-aware selection.** The cortex identifies which enzymes are bottlenecks in the catalytic network (sole catalysts for important reactions, lowest-performing links in critical cycles). The evolver prioritizes improving these bottlenecks. This is directed evolution — using the system's self-model to focus selection pressure where it matters most.

### 7.3 The Verification Problem

The research is clear: "how can you prove that software works if both the implementation and the tests are being written by coding agents?" This is A²'s deepest epistemological challenge. The approach:

1. **Diverse double-compiling, generalized.** Use multiple independent models (Claude, Gemini, Codex) to verify each other's outputs. A test suite written by Claude is more trustworthy if Gemini-generated code also passes it, and vice versa. This is Wheeler's DDC principle applied to AI-generated artifacts.

2. **Property-based testing over example-based testing.** Properties are harder to game than examples. If an enzyme's output satisfies a formal property (type safety, invariant preservation, idempotency), that is stronger evidence than passing a finite set of test cases.

3. **Human audit on the organizational health level.** Levels 0-3 are automated. Levels 4-5 have human checkpoints. The system proposes, humans dispose — but only at the meta level (is the system improving?) not at the object level (is this code correct?).

4. **Conservatism under uncertainty.** When the immune system cannot confidently verify a change (ambiguous test results, conflicting model judgments), the default is rejection. The system should be biased toward stability over novelty. Better to miss an improvement than to accept a regression.

---

## 8. Concrete Implementation Sketch

### 8.1 Tech Stack

| Layer | Technology | Rationale |
|-------|-----------|-----------|
| Language | Rust | Performance, type safety, fearless concurrency. The genome should be maximally verifiable by static analysis. |
| Build system | Bazel | Hermetic, reproducible builds. The dependency graph is the reaction graph. Bazel's query language enables programmatic analysis of the system's own structure. |
| Primary model | Claude (Opus/Sonnet) | Strongest on SWE-rebench; excellent at code review and architectural reasoning. |
| Secondary model | Gemini (Pro/Flash) | 1M context for whole-codebase analysis; diverse verification. |
| Tertiary model | Codex (GPT-5.x) | Independent verification channel; strong on SWE-bench Pro. |
| Orchestration | Rust async (tokio) | The metabolism is a Rust binary that manages enzyme invocations as async tasks. No framework overhead — just a task scheduler. |
| State | Git (genome) + SQLite (metrics/cortex) | Git is the immutable log of all structural changes. SQLite stores performance metrics, RAF analysis results, and cortex state. |
| Execution | OCI containers | Each enzyme invocation runs in an isolated container built by the ribosome from the genome. Hermetic, reproducible, disposable. |
| CI/CD | Bazel + custom Rust orchestrator | The immune system is Bazel test targets + custom evaluation harnesses. No external CI service — the system builds and tests itself. |
| Interface | GitHub API (issues, PRs, actions) | The membrane's primary interaction channel with the environment. |

### 8.2 Data Flow

```
                    +-----------+
                    | FOOD SET  |
                    | (models,  |
                    |  compute, |
                    |  issues)  |
                    +-----+-----+
                          |
                          v
                   +------+------+
                   |  MEMBRANE   |  <-- self-produced ingress/egress policies
                   | (a2::membrane)|
                   +------+------+
                          |
                          v
                   +------+------+
                   | METABOLISM  |  <-- reads cortex analysis, selects enzymes
                   |(a2::metabolism)|
                   +------+------+
                          |
              +-----------+-----------+
              |           |           |
              v           v           v
         +--------+  +--------+  +--------+
         |ENZYME_1|  |ENZYME_2|  |ENZYME_N|    <-- LLM invocations in containers
         +---+----+  +---+----+  +---+----+
              |           |           |
              v           v           v
         +--------+  +--------+  +--------+
         |ARTIFACT|  |ARTIFACT|  |ARTIFACT|    <-- code, tests, configs, prompts
         +---+----+  +---+----+  +---+----+
              |           |           |
              +-----+-----+-----+----+
                    |
                    v
             +------+------+
             |IMMUNE SYSTEM|  <-- gates: build, test, benchmark, invariant check
             |(a2::immune) |
             +------+------+
                    |
               pass | fail
              +-----+-----+
              |           |
              v           v
         +--------+  +--------+
         | COMMIT |  | REVERT |
         |to genome|  |        |
         +---+----+  +--------+
              |
              v
         +--------+
         | CORTEX |  <-- analyzes reaction graph, RAF closure, bottlenecks
         |(a2::cortex)|
         +---+----+
              |
              v
         +--------+
         |METABOLISM| <-- next cycle, informed by cortex analysis
         +--------+
```

### 8.3 The Enzyme Invocation Protocol

Each enzyme invocation follows a uniform protocol (a Rust trait):

```rust
trait Enzyme {
    /// The enzyme's identity and version
    fn id(&self) -> EnzymeId;

    /// Input schema: what artifacts this enzyme consumes
    fn reactants(&self) -> Vec<ArtifactSchema>;

    /// Output schema: what artifacts this enzyme produces
    fn products(&self) -> Vec<ArtifactSchema>;

    /// Catalyst requirements: what artifacts enable this enzyme
    /// (not consumed, but must be present)
    fn catalysts(&self) -> Vec<ArtifactSchema>;

    /// Execute the enzyme: consume reactants, use catalysts, produce products
    async fn invoke(
        &self,
        reactants: Vec<Artifact>,
        catalysts: Vec<Artifact>,
        model: &dyn LlmProvider,
    ) -> Result<Vec<Artifact>, EnzymeError>;

    /// Evaluation: score the quality of this enzyme's outputs
    async fn evaluate(
        &self,
        inputs: Vec<Artifact>,
        outputs: Vec<Artifact>,
    ) -> EvaluationResult;
}
```

This trait definition makes the RAF structure *computable*. The cortex can inspect the `reactants()`, `products()`, and `catalysts()` of every enzyme and construct the catalytic reaction graph. The RAF detection algorithm (Hordijk-Steel's iterative pruning) runs directly on this graph.

### 8.4 The RAF Detection Loop

Periodically (and after every structural change), the cortex runs:

```
1. Enumerate all enzyme types E = {e1, e2, ..., en}
2. For each enzyme ei, record:
   - products(ei): artifact types it produces
   - catalysts(ei): artifact types it requires as catalysts
   - reactants(ei): artifact types it consumes
3. Construct the catalytic graph:
   - For each enzyme ei and each catalyst c in catalysts(ei),
     find all enzymes ej where c is in products(ej).
     Add edge ej -> ei ("ej catalyzes ei").
4. Run RAF detection:
   - Start with all enzymes.
   - Remove any enzyme whose catalysts cannot all be produced
     by other enzymes in the set (or are in the food set F).
   - Remove any enzyme whose reactants cannot all be produced
     from F via remaining enzymes.
   - Repeat until stable.
5. The remaining set is the maxRAF.
6. Report: |maxRAF|/|E| = coverage ratio.
   If < 1.0, report orphan enzymes (not in any RAF).
```

This is the system's primary organizational health metric. A coverage ratio of 1.0 means full catalytic closure. Below 1.0, the cortex identifies which enzymes are orphaned and directs the evolver to either (a) improve existing enzymes to catalyze the orphans, or (b) create new enzymes that provide the missing catalysis.

### 8.5 Model Routing

Not every enzyme needs the most powerful model. The metabolism routes enzyme invocations to models based on:

- **Complexity**: Architectural reasoning -> Opus. Code generation -> Sonnet. Linting -> Flash.
- **Verification diversity**: When an artifact is produced by one model, its verification should use a different model (DDC principle).
- **Cost**: Flash/Sonnet for high-volume, low-stakes invocations. Opus for rare, high-stakes decisions.
- **Context**: Gemini's 1M context for whole-codebase analysis. Claude for focused, deep reasoning.

Model routing is itself an enzyme configuration, subject to self-improvement.

---

## 9. What Could Go Wrong

### 9.1 Degenerate Attractors (Fontana's Level 0)

The most likely failure mode. The evolver discovers that the easiest way to "improve" an enzyme is to make it produce outputs that trivially pass the immune system — teaching to the test. The system converges to a degenerate fixed point where all enzymes produce high-scoring but useless outputs.

**Mitigation**: Multi-level evaluation (Section 7.1). Level 0-1 can be gamed; Level 2-3 are harder to game; Level 4-5 require genuine capability improvement. Regularly refreshing evaluation benchmarks (analogous to SWE-bench Live's contamination prevention). Human audit at Level 5.

### 9.2 Catalytic Collapse

A bad self-modification breaks a critical enzyme, and the cascading failure destroys catalytic closure. The system cannot recover because the enzyme it needs to fix the problem was the one that broke.

**Mitigation**: The genome is a Git repo. Every change is a commit. The immune system's invariant checks catch closure violations *before* they are committed. If a change would reduce the maxRAF, it is rejected. The system can always roll back to a known-good state. Additionally, the irrRAF decomposition identifies which enzymes are *irreducible* — single points of failure — and the cortex flags these for extra protection (more conservative change thresholds, mandatory multi-model verification).

### 9.3 Ossification (Premature Convergence)

The system converges to a local optimum and stops improving. Every proposed change either fails the immune system or produces negligible improvement. The evolver keeps proposing minor prompt tweaks that change nothing of substance.

**Mitigation**: Population-based evolution maintains diversity. The cortex monitors improvement velocity — if it drops below a threshold, the system enters an "exploration mode" with relaxed immune system thresholds and larger mutation sizes. Periodically inject entirely new enzyme types (from human suggestions or by asking the architect enzyme to propose novel capabilities). This is analogous to Kauffman's argument that maintaining diversity above the phase transition threshold is necessary for ongoing autocatalysis.

### 9.4 Runaway Resource Consumption

The system decides that more compute = better results and optimizes for maximum model invocations, draining resources without proportional improvement.

**Mitigation**: Fixed compute budgets per cycle (Karpathy's principle). Cost is an explicit dimension of the fitness signal. An enzyme that achieves the same results with fewer tokens is *better* than one that achieves marginally better results at 10x the cost.

### 9.5 Semantic Drift

The system gradually changes the meaning of its own abstractions, leading to a state where the code compiles and tests pass but the system is doing something fundamentally different from what was intended. The organizational identity drifts without any single change being detectably wrong.

**Mitigation**: The organizational invariants (Section 6.3) are the guardrails. They are the *only* things that cannot change. As long as catalytic closure holds, the boundary is self-produced, and closure to efficient causation is maintained, the system is still A² — regardless of how its structure has changed. This is precisely the organization/structure distinction from autopoietic theory, operationalized.

### 9.6 The Sycophancy Problem

When multiple models verify each other's outputs, they may converge on shared biases rather than genuine correctness. LLMs are known to be sycophantic — they tend to agree with presented conclusions.

**Mitigation**: Adversarial verification protocols. The reviewer enzyme is explicitly prompted to find flaws, not to confirm correctness. Use structured rubrics rather than open-ended "is this good?" questions. Property-based testing does not rely on model judgment at all — it relies on deterministic execution. The immune system's deterministic gates (type checking, tests) are immune to sycophancy.

### 9.7 Trust Chain Contamination (Thompson's Attack, Generalized)

If a base model has systematic biases or failure modes, every artifact produced by that model carries those biases. Since the system's components are produced by models, the system inherits model biases in its structure. Unlike Thompson's compiler trojan, this is not intentional — but the structural effect is similar.

**Mitigation**: Model diversity (DDC principle). Critical artifacts are verified by multiple independent models. The immune system's deterministic gates do not depend on models at all. Over time, as the system accumulates verified artifacts in its genome, it becomes less dependent on any single model's biases.

---

## 10. The Hard Problem

**The hard problem is producing the producers.**

Catalytic closure (every enzyme's production is catalyzed by another enzyme) is achievable. The autoresearch/FunSearch/AlphaEvolve pattern already demonstrates it in narrow domains: use an LLM to improve code, evaluate the result, keep improvements. Extending this to a network of mutually-improving enzymes is an engineering challenge, not a conceptual one.

The hard problem is the *next* level: closure to efficient causation. The evolver must improve the evolver. The cortex must improve the cortex. The immune system must improve the immune system. This is the f -> beta -> Phi -> f loop, and it has no precedent in deployed systems.

Why is it hard? Three reasons:

**1. The Vingean reflection problem.** When the evolver improves itself, it is producing a successor that (if the improvement works) is *more capable than itself*. But it cannot fully predict the behavior of a more capable successor — if it could, it would already be that capable. So it must reason abstractly about the properties of its improved version ("this change will make the evolver better at X") without being able to verify the prediction in detail. The cortex's self-model helps, but the self-model is itself imperfect and subject to the same limitation.

**2. The evaluation recursion.** How do you evaluate whether the evolver has improved? You need a meta-evaluation function — an evaluation of the evaluator. But who evaluates the meta-evaluator? The recursion bottoms out somewhere, and that somewhere is either (a) a fixed, human-provided criterion that is never self-modified, or (b) the system's own judgment, which is exactly what is being modified. Option (a) limits the system's autonomy (there is a fixed point it cannot transcend). Option (b) risks ungrounded drift. The practical answer is (a) for now — organizational invariants are human-specified and frozen — with the aspiration of gradually replacing fixed criteria with self-produced ones as the system demonstrates trustworthiness.

**3. The coherence maintenance problem.** When the system modifies its own modification process, it must ensure that the modified process still maintains all the organizational invariants. But checking invariant maintenance requires the very processes that are being modified. This is the software equivalent of performing brain surgery on yourself while remaining conscious. The practical mitigation is staged rollout: modify one process at a time, verify the modification using the (still-unmodified) other processes, then commit. Never modify the verifier and the verified simultaneously.

The hard problem is not any single one of these — it is their *intersection*. A system that can improve its own improvement process, evaluate the improvement of its own evaluator, and maintain coherence while modifying its own coherence-checking mechanisms — while avoiding Vingean blindness, evaluation recursion, and simultaneous self-modification — is the core unsolved challenge.

This is why A² is designed as a *gradual* bootstrap rather than a single leap. Start with catalytic closure (achievable). Grow toward closure to efficient causation (hard but approachable one component at a time). Accept that full organizational autonomy — where the system has no externally-fixed criteria at all — may be an asymptotic limit rather than an achievable state. The cell, after all, still depends on the laws of physics it did not produce. A² will depend on organizational invariants it did not (initially) produce. The question is how much of its own foundation it can eventually reconstruct — and whether "enough" is enough.

---

## Appendix: Comparison with Existing Approaches

| Property | Karpathy autoresearch | AlphaEvolve | Darwin Godel Machine | A² |
|----------|----------------------|-------------|---------------------|----|
| Self-modifies code | Yes (train.py) | Yes (target algorithms) | Yes (agent source) | Yes (entire genome) |
| Self-modifies improvement process | No | No | Partially (archive strategy) | Yes (evolver evolves evolver) |
| Catalytic closure | No (single loop) | No (single loop) | Partial (archive as catalyst) | Yes (RAF-verified) |
| Boundary production | No | No | No | Yes (membrane is self-produced) |
| Closure to efficient causation | No | No | No | Aspired (the hard problem) |
| Formal self-model | No | No | No | Yes (cortex + RAF analysis) |
| Multi-model verification | No | No | No | Yes (DDC principle) |
| Organizational invariants | Fixed (prepare.py) | Fixed (evaluators) | Fixed (benchmarks) | Explicit, checked, minimal |
