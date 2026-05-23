# 02 — Converge-Refinery Patterns for A²D

**Created:** 2026-04-01
**Source:** `~/Projects/Banade-a-Bonnot/converge-refinery/`

---

## Three Iteration Modes

### 1. Converge (Lamarckian — directed improvement)
- Models see reviews, suggestions, and other models' answers
- Cross-pollination through explicit feedback
- Fast convergence via directed signal
- **A²D use:** Mutation verification — do N models agree this enzyme change is correct?

### 2. Brainstorm (Score-only diversity hunting)
- Models see ONLY their own prior answers + aggregate mean scores (no rationale)
- Selection: highest **controversy score** = mean_score × stddev(scores)
- Returns a panel of diverse, high-quality answers where evaluators *disagree*
- **A²D use:** Mutation generation — find variants that are high-quality but disputed

### 3. Evolve (Darwinian — blind variation + selection)
- Models iterate independently (complete lineage isolation)
- Selection pressure from scores alone, no rationale
- Culled models restart from scratch (natural diversity injection)
- **A²D use:** Blind exploration of mutation space, countering MVT violation (4x under-explore)

## Key Patterns

### Controversy Scoring (mean × stddev)
- High score + high agreement = safe but potentially boring
- High score + high disagreement = interesting and risky
- Captures disagreement as a feature, not a bug
- For A²D: mutations where evaluators disagree rank higher for exploration

### Self-Evaluation Exclusion
- An enzyme cannot evaluate its own mutations (Constitution Invariant 4)
- The refinery enforces this structurally: model N never scores model N's answer
- Position bias mitigation: anonymized, shuffled labels

### Score-Only Feedback (Anti-Monoculture)
- In brainstorm/evolve modes, models don't know WHY they scored low
- Forces genuine exploration rather than converging on shared narrative
- Prevents the methodological monoculture Sawdust documented (10/11 overlap)

### Lineage Tracking
- Per-model score trajectories across rounds
- Which proposals survived, mutated, or died
- Proposal history paired with per-evaluator scores
- Maps directly to A²D's lineage archive requirement

## Integration Points

| Refinery Component | A²D Module | Connection |
|---|---|---|
| Convergence strategy | Germline mutation gating | Use converge mode to verify proposed mutations |
| Controversy scoring | Evolver mutation selection | Rank mutation candidates by controversy |
| Score-only iteration | Anti-monoculture enforcement | Prevent models from gaming evaluation |
| Self-eval exclusion | Constitution Invariant 4 | Structural enforcement of information barriers |
| Lineage tracking | Lineage archive | Record mutation history with per-evaluator scores |
| Darwinian mode | Exploration budget | Counter MVT violation through blind variation |
