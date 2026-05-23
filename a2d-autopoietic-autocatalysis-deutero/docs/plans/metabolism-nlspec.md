# Metabolism NLSpec

## Why

The metabolism is the runtime orchestrator of the A²D catalytic network.
Without it, the seed's components (enzymes, workcells, germline, observer,
providers) are isolated modules that cannot form a self-sustaining loop.

The metabolism must be the component that makes the autocatalytic cycle
actually turn: scheduling enzyme invocations, routing artifacts between
them, managing workcell lifecycles, integrating observer health signals,
and triggering germline mutations when fitness improves.

It must do this without becoming a single point of failure or a source
of methodological monoculture. The metabolism itself should eventually be
evolvable — an enzyme in the RAF, not a privileged controller outside it.

## What

The metabolism:

1. **Schedules enzyme invocations** based on the catalytic graph (which
   enzymes have their reactants and catalysts available?)
2. **Spawns ephemeral workcells** for each invocation, providing scoped
   typed contexts from the germline
3. **Routes artifacts** from one enzyme's outputs to another enzyme's
   inputs, matching on ArtifactType
4. **Monitors workcell health** via the observer, killing pre-failure
   workcells and respawning fresh ones
5. **Gates germline mutations** by running RAF closure checks on proposed
   changes (delegation to germline module)
6. **Selects providers** for each enzyme via the provider registry
7. **Records lineage** — every invocation, its inputs, outputs, health
   metrics, and provider used

### Constraints

- No agent discretion in scheduling: enzyme readiness is determined by
  artifact availability, not by the metabolism "deciding" what to do next
- No self-evaluation: the metabolism does not assess its own health
  (the observer does that mechanically)
- Workcell lifecycle is fire-and-forget: spawn, observe, kill-or-complete
- The metabolism is itself an enzyme definition in the germline (bootstrap
  requirement: the system describes its own orchestrator)

## How

### Data flow

```
food set (requirements, compute, model APIs)
    │
    ▼
┌─────────┐     ┌──────────┐     ┌──────────┐
│  Coder  │────▶│  Tester  │────▶│ Evolver  │
│ (code)  │     │ (results)│     │ (defs)   │
└─────────┘     └──────────┘     └──────────┘
    ▲                                  │
    └──────────────────────────────────┘
                  (enzyme_defs catalyze coder)
```

### Scheduling algorithm

1. Build artifact availability map from food set + prior outputs
2. For each enzyme in the germline:
   - Check: are all reactants available?
   - Check: are all catalysts available?
   - If both: enzyme is "ready"
3. Invoke all ready enzymes (potentially in parallel)
4. Collect outputs, update artifact map
5. Repeat until no new enzymes become ready (fixed point)

### Workcell lifecycle

1. Spawn workcell with typed context (enzyme def + available artifacts)
2. Invoke provider (LLM call)
3. Record tool events from provider response into workcell
4. Check observer: should_kill?
   - Yes → kill workcell, log failure, optionally retry with fresh workcell
   - No → collect outputs, complete workcell
5. Archive workcell trace in lineage

### Mutation flow

When the evolver enzyme produces new enzyme_defs:
1. Parse the proposed enzyme definition
2. Call germline.propose_replace() (or propose_add)
3. If accepted (RAF gate passes): commit to germline
4. If rejected: log the rejection, continue with current germline

### Observer integration

- After each workcell completes or is killed, feed its trace to the observer
- Aggregate health metrics across all workcells in the current cycle
- If system-level entropy exceeds threshold, pause the cycle
- The metabolism does NOT interpret health metrics — it forwards them

## Done

- [ ] Metabolism can run one full cycle: coder → tester → evolver → coder
- [ ] Artifacts route correctly between enzymes via ArtifactType matching
- [ ] Workcells are spawned fresh for each invocation (no reuse)
- [ ] Pre-failure workcells are killed and optionally retried
- [ ] Evolver output triggers germline mutation via RAF gate
- [ ] Every invocation is recorded with inputs, outputs, provider, and health
- [ ] The metabolism's own definition exists as an enzyme in the germline
- [ ] Running `cargo test` with mock providers exercises the full cycle
