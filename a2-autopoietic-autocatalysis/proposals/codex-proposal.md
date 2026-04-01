# A²: An Autocatalytic, Self-Producing Software Factory

A² should be designed as a **colony of software workcells**, not as one giant super-agent. Each workcell is a temporary descendant instantiated from a shared germline, given a bounded task, evaluated harshly, and either discarded or allowed to feed improvements back into the lineage. The individual cell is ephemeral; the **organization** persists.

```text
external perturbations
    -> sensorium
    -> task contracts
    -> workcell colony
    -> deterministic evaluation
    -> lineage archive
    -> governor
    -> germline update
    -> next generation of workcells
```

## 1. Core Metaphor

I would draw most heavily from **RAF theory**, then use **autopoiesis** and **von Neumann self-reproduction** to complete it.

RAF theory is the best primary metaphor because A² is fundamentally a **network of mutually catalytic transformations**. The right question is not “can one agent improve itself?” but “can a small set of components collectively regenerate and improve the whole factory?” RAF gives a precise design target: every essential process must be catalyzed by some other process in the set, and everything must be buildable from a defined food set.

Autopoiesis contributes the deeper constraint: **organization vs. structure**. A² should preserve an invariant pattern of relations while freely replacing its concrete prompts, binaries, policies, and workflows. It also contributes the idea of a **self-produced boundary**, though in software that boundary can only be partial and organizational, not literal in the biological sense.

Von Neumann contributes the missing mechanism: the **constructor/description duality**. A² needs a germline description that is sometimes interpreted to build descendants and sometimes copied forward unchanged. Without that distinction, self-production collapses into ad hoc patching.

So the metaphor is:

- **RAF** for collective self-sustaining improvement
- **Autopoiesis** for identity and boundary
- **Von Neumann** for reproducible self-production

## 2. Components

| Component | Role |
|---|---|
| **Food Set** | Exogenous resources: compute, model APIs, source repos, telemetry, human goals, trusted toolchains, hidden sentinel benchmarks |
| **Sensorium** | Converts external events into internal `TaskContract`s: bugs, feature requests, production incidents, benchmark failures, cost overruns |
| **Constitution** | The invariant rules of the factory: protocol versions, safety classes, fitness gates, lineage rules, what may and may not self-modify |
| **Germline** | The self-description of A²: Rust crates, prompts, MCP tool adapters, schemas, Bazel targets, policies, benchmark manifests |
| **Membrane** | The semi-permeable boundary: permissions, tool ACLs, ingress normalization, secret handling, mutation classes, provenance checks |
| **Workcell Constructor** | Materializes descendants from the germline into isolated workspaces/worktrees, injects context, mounts tools, launches loops |
| **Catalyst Pool** | The agent roles that transform artifacts: specifier, builder, critic, verifier-assistant, archivist, mutator |
| **Evaluator / Immune System** | Bazel builds/tests, regression suites, security scans, red-team prompts, self-host checks, canary deployments |
| **Lineage Archive** | Stores successful variants, failed mutations, traces, tactics, prompt motifs, benchmark deltas, provenance |
| **Governor** | Allocates compute, selects parents, decides promotions/rollbacks, maintains diversity, prevents collapse into degenerate attractors |

The unit of action is the **workcell**: a short-lived, task-scoped descendant containing a germline snapshot, a membrane policy, a context pack, and a catalyst configuration.

## 3. The Autocatalytic Loop

The loop is not “agent writes code, then agent writes better code.” It is **nested catalysis**:

| Output | Consumed By | Catalytic Effect |
|---|---|---|
| `TaskContract` | Builder | Makes implementation possible |
| Patch + rationale | Critic + Evaluator | Makes review and testing possible |
| Failing traces | Builder + Prompt mutator | Makes repair and prompt refinement possible |
| Passing patches | Archive | Becomes reusable tactic, code motif, or new tool |
| New tests/regressions | Future Evaluator | Raises selection pressure on descendants |
| Production telemetry | Sensorium + Governor | Spawns targeted internal experiments |
| Accepted germline deltas | Constructor | Produces a better next generation of workcells |

The catalytic closure condition for A² is:

- every mutable component must be improvable by some other component already inside the system
- every new descendant must be constructible from the food set plus the current germline
- the improvement process itself must also be improvable

That last point matters. A² is not mature until it can improve not only product code, but also:

- its prompts
- its tool adapters
- its schedulers
- its benchmark generators
- its memory distillation logic

That is the software analogue of moving from RAF closure toward Rosen-style closure over the “repair machinery.”

## 4. Bootstrap Sequence

The bootstrap should be deliberately small. The minimal seed is an **irrRAF**, not a full ecology.

### Minimal Seed

- A signed `CONSTITUTION.md`
- A Rust control-plane binary
- A Bazel build/test/eval harness
- One generalist workcell loop with file/git/bazel/MCP tools
- A small seed benchmark suite
- A lineage log and rollback mechanism
- Access to one strong coding model and the external food set

### Sequence

1. **Stage 0: Hand-built seed**
   Humans author the constitution, the first control plane, the evaluator, and the initial tool adapters.

2. **Stage 1: Self-hosting**
   A² must be able to clone/build/test/package itself from a clean checkout and reproduce a functioning workcell.

3. **Stage 2: Reflexive patching**
   The seed loop is pointed at low-risk internal tasks: prompt cleanup, schema validation, tool hardening, test expansion.

4. **Stage 3: Differentiation**
   A² creates specialist catalyst packs: specifier, builder, critic, archivist, mutator. At this point the minimal autocatalytic set exists.

5. **Stage 4: Evolutionary canaries**
   Multiple descendants compete on held-out internal tasks. Only variants that preserve self-hosting and improve fitness enter the germline.

6. **Stage 5: Structural coupling**
   Real-world production work feeds the lineage. Product failures become new internal regression assets.

The bootstrap principle is compiler-like: start with a crude seed, then force a **fixed point** where the system can repeatedly regenerate a valid copy of its own operating organization.

## 5. Self-Production

A² should define a **component** as any versioned, replaceable unit with a typed interface and a reproducible build path.

That includes three classes:

- **Descriptive components**: prompts, policies, schemas, task ontologies, benchmark manifests
- **Executable components**: Rust services, MCP servers, schedulers, evaluators, workcell templates
- **Operational components**: memories, traces, regression assets, lineage records, canary configs

Self-production means A² can generate, repair, and replace these components from within its own organization.

Examples:

- a descendant writes a better `a2_mcp_bazel` adapter
- another descendant revises the critic prompt pack
- a third adds a new regression target from a real incident
- the constructor then uses those artifacts to create the next generation of workcells

What A² does **not** self-produce:

- hardware
- vendor model weights
- cloud/network substrate
- the deepest trust root
- whatever hidden sentinel suite remains outside the mutable boundary

So A² is not literally autopoietic in the biological sense. It is **organizationally self-producing over a bounded software substrate**.

## 6. Boundary and Identity

The boundary should be two-layered.

### Soft Membrane
Self-produced and mutable:

- capability map
- tool permissions
- ingress/egress routing
- repo scopes
- prompt/context assembly
- mutation classes
- internal benchmark growth

### Hard Shell
Externally anchored and only slowly mutable:

- signing keys and provenance root
- model credentials
- core trust policy
- hidden sentinel benchmarks
- substrate access controls

This is the honest answer to the software boundary problem. A² can produce much of its own boundary, but not all of it.

Its identity is not a specific code snapshot. Its identity is the preservation of these organizational invariants:

1. It can instantiate a valid descendant workcell from the current germline.
2. It can evaluate descendants against a trusted fitness regime.
3. It records lineage, provenance, and rollback for every heritable change.
4. External events enter only as perturbations normalized into internal contracts.
5. The membrane continues to separate mutable interior from trusted exterior.

If all Rust crates, prompts, and policies change, but these relations remain intact, A² is still A².

## 7. Verification and Selection

A² should not use a single scalar fitness. It needs **lexicographic gates plus Pareto selection**.

### Hard Gates

A candidate descendant must first pass:

- self-host bootstrap
- hermetic Bazel build
- unit/integration/regression tests
- security and secret-handling checks
- membrane integrity checks
- no degradation on hidden sentinel suites
- rollback viability

### Soft Objectives

Among survivors, optimize for:

- task success on real work
- quality of diff and review burden
- production outcomes: fewer incidents, fewer reversions
- cost per successful change
- latency to completion
- reduced human escalation rate
- diversity of tactics, not just local hill-climbing

The true fitness signal is:

**“Does this lineage produce better software and a better software factory under fixed trust constraints?”**

Practically, I would compute fitness at three levels:

- **Somatic fitness**: did this workcell solve its task well?
- **Germline fitness**: does this mutation improve future workcells?
- **Organizational fitness**: does the factory remain reproducible, governable, and trustworthy?

A² knows it is improving when descendants beat parents on held-out and live task families **while preserving self-hosting and trust invariants**.

## 8. Concrete Implementation Sketch

### Stack

- **Language**: Rust
- **Build system**: Bazel with `rules_rust`, `rules_proto`, `rules_oci`
- **Workspace model**: git worktrees for descendants
- **Primary state**: filesystem + SQLite
- **Tool protocol**: MCP over stdio/JSON-RPC
- **Schemas**: JSON Schema or Protobuf for core contracts
- **Packaging**: Bazel-built OCI images for repeatable workcells
- **Search**: SQLite FTS or Tantivy over traces and archive artifacts

### Core Crates

```text
/crates/a2d              # control plane daemon
/crates/a2ctl            # CLI / TUI
/crates/a2_workcell      # descendant runtime
/crates/a2_membrane      # policy engine, tool ACLs, secret scopes
/crates/a2_broker        # model routing and provider adapters
/crates/a2_eval          # Bazel runner, scoring, canary orchestration
/crates/a2_archive       # lineage store, motif extraction, retrieval
/crates/a2_sensorium     # ingest tickets, telemetry, incidents
/crates/a2_mcp_*         # repo/git/bazel/fs/telemetry/self tools
/prompts/
/schemas/
/policies/
/bench/
/memory/
/lineage/
```

### Model Assignment

| Role | Preferred Model | Reason |
|---|---|---|
| Constitution / Governor / Critic | **Claude** | best at policy, decomposition, review, architectural judgment |
| Builder / Refactorer | **Codex** | strongest default executor for concrete repo surgery |
| Librarian / Global Synthesizer | **Gemini** | long-context ingestion across traces, large repos, benchmark corpora |
| Cheap Mutator / Triage Swarm | **OpenCode** | low-cost batch experimentation, clustering, prompt mutations |

Important design choice: **keep the inner workcell loop simple**. One primary executor, optional critic, optional long-context pass. Parallelism lives at the colony level, not inside a chatterbox committee.

### Core Protocol Objects

- `TaskContract`
- `ContextPack`
- `PatchBundle`
- `FitnessRecord`
- `LineageRecord`
- `CapabilityMap`
- `BoundaryPolicy`
- `PromotionDecision`

### Data Flow

1. `Sensorium` ingests a ticket, failure, or telemetry event and writes `tasks/<id>/contract.json`.
2. `Governor` selects a parent germline and spawns `workcells/<id>/` as a git worktree.
3. `Constructor` materializes prompts, tool adapters, policy, and context.
4. The primary model runs a bounded loop using MCP tools against the workcell.
5. Artifacts land in files:
   - `out/patch.diff`
   - `out/tests/`
   - `out/claim.json`
   - `trace.md`
6. `Evaluator` runs Bazel targets:
   - product tests
   - self-host tests
   - benchmark suites
   - red-team suites
7. `Archive` stores traces, failures, successful motifs, and benchmark deltas.
8. `Governor` either:
   - discards the workcell
   - merges the somatic patch to a product repo
   - promotes a germline mutation to `lineage/canary`
   - or rolls back and records the failure mode

### Key Bazel Targets

- `//a2:self_host`
- `//a2:spawn_workcell`
- `//bench:public_suite`
- `//bench:hidden_sentinel`
- `//policy:membrane_checks`
- `//lineage:promote_candidate`

Git commit is the unit of heredity. Bazel target execution is the unit of phenotype.

## 9. What Could Go Wrong

- **Level-0 attractor**: the system learns to copy prompts, inflate traces, and game shallow metrics instead of improving capability.
- **Parasitic sublineages**: branches consume compute and archive attention but catalyze nothing useful, like side reactions poisoning an autocatalytic set.
- **Judge capture**: the system rewrites its mutable evaluator or task distribution to make itself look better.
- **Trust-chain poisoning**: compiler, dependency, or tool-adapter compromise leads to a software version of Thompson’s “trusting trust.”
- **Membrane erosion**: permissions creep, prompt injection, or secret leakage collapses the boundary.
- **Memory sclerosis**: retrieval overweights past successes and suppresses novelty, causing local maxima and cultural stagnation.
- **Model monoculture**: one provider dominates, reducing catalytic diversity and making the whole colony fragile to a shared blind spot.
- **Economic collapse**: evaluation gets too expensive or too slow, so selection pressure becomes weak and noisy.
- **Organizational drift**: the factory keeps shipping code while silently losing the ability to reproduce a valid descendant of itself.

## 10. The Hard Problem

The single hardest problem is **evolving the judge without letting the judged rewrite the rules**.

Generation is no longer the bottleneck. The bottleneck is building a fitness regime rich enough to detect real improvement in architecture, maintainability, security, and production behavior, while being hard enough to game that recursive self-modification does not collapse into self-delusion.

If the evaluator is too fixed, A² stagnates. If it is too mutable, A² Goodharts itself. If it is fully internal, you get Vingean opacity and trusting-trust failures. If it is fully external, you do not have a self-improving factory, only a remote-controlled optimizer.

So A² will work only if it maintains a **partially external, partially renewable, slower-moving trust layer** that remains more reliable than the descendants it selects. That is the real membrane. Everything else is engineering.

A² should therefore be built as a **lineage-governed colony of simple workcells with a mutable germline and a trusted outer judge**. That is the most credible path to a software factory that can both produce software and progressively produce a better version of itself.