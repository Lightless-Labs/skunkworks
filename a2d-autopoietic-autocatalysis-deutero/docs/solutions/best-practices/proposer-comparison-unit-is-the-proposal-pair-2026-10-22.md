---
title: "In a Proposer Comparison the Unit Is the Proposal Pair, Not the Task"
date: 2026-10-22
category: best-practices
module: tools/gym/phaseb_sim.py
problem_type: evaluation_design
component: phase_b
tags: [phase-b, proposer-comparison, clustering, bootstrap, power, simulation, pre-registration]
symptoms:
  - "a (round, family) cluster bootstrap's null rejection rate rises to 0.07-0.10 at a nominal 0.05 as rounds grow or proposal quality varies more"
  - "adding tasks per round barely moves power; adding rounds does"
---

# In a proposer comparison the unit is the proposal pair, not the task

## Problem

Phase B compares two proposers. Each round, each proposer makes ONE change to a frozen
incumbent. Both candidates run on the same seeded subset of tasks, with one binary outcome per
(task, arm). Protocol v2's registered analysis pooled task-level differences and bootstrapped over
(round, family) clusters.

Every task in a round shares the same pair of proposals. The true difference in proposal quality
is therefore a round-level random effect. Resampling (round, family) clusters independently treats
the F families of a round as F independent draws of it, which attenuates its variance to roughly
1/F of what it should be.

The simulation (`tools/gym/phaseb_sim.py`, with the registered bootstrap re-implemented):
- the null rate is nominal (0.050) at cycle 2's own size (36 tasks × 4–6 rounds, between-proposal
  SD 3.5 pp);
- it rises to 0.07–0.10 with larger rounds or an SD of 5 pp.

Cycle 2's result was not affected, but the method does not scale.

## What works

- **Analyse at the unit of randomisation that carries the effect.** Take one difference per
  round (mean all-pass, informed minus uninformed) and run a one-sided t-test over R rounds. In
  every stress scenario its null rate stayed at 0.041–0.062: skewed effects, arm correlation,
  per-task mixed gains and losses, family effects, quarter schedules, and all of them combined.
- **Power comes from proposal pairs, not tasks.** The between-proposal variance does not shrink
  with tasks per round. At a fixed budget, many small rounds win: 16 tasks × 64 rounds reaches
  0.8 power at +5 pp under the conservative scenario, about 7.2k Luna, against 0.68 for 120 × 10
  at a similar cost.
- **Make the pairs independent by design.** If the informed arm sees earlier rounds' outcomes,
  proposals depend on history, and a t-test over rounds loses its justification (Sol's review
  blocker). Freeze the information before round 1 and give no outcome feedback between rounds.
  That tests "do the observations improve one proposal", not "does the proposer learn". Say so:
  it narrows the estimand.
- **Keep counts under a continuation cap are not an endpoint.** If at most one proposal per arm
  can continue to full evaluation, confirmed keeps are at most 1 vs 0. Report them descriptively.
- **Calibrate the simulation from your own repeat runs, then size against a stated conservative
  scenario.**
  - Two runs of one harness disagreed on 0.41–0.57 of the binomial 2π(1−π): outcomes concentrate
    per task.
  - Family clustering was absent (SE ratio about 0.96), but that does not prove family effects
    are absent, so the scenarios include them.
  - The between-proposal SD (about 3.5 pp by method of moments) has no identifiable bias
    direction. Treat it as a scenario, and size at 5 pp.
- **Pre-register the method before the data exists, and the numbers before the first proposal.**
  `--search-r` turns "the cheapest R with power ≥ 0.8" into a seeded, deterministic tool, so the
  design choice cannot drift with the results.

## Checklist for any "arm A vs arm B over rounds" design

1. What is shared within a round? That level is the unit.
2. Does any arm see outcomes between rounds? If so, the rounds are not independent: freeze the
   information, or use independent trajectories as units.
3. Does power grow with the thing you are buying? If it does not grow with tasks, buy rounds.
4. Can the claimed endpoint vary at all under the protocol's caps?
5. Simulate the registered analysis itself, not a stand-in, under the null.
