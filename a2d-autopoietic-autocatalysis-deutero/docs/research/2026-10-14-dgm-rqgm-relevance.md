# DGM and RQGM: relevance to A²D

**Date:** 2026-10-14
**Context:** read alongside `docs/research/2026-10-14-direction-audit.md`. Sources supplied by the project owner.

## Sources

- Darwin Gödel Machine (DGM) — Zhang, Hu, Lu, Lange, Clune (Sakana AI / UBC). arXiv [2505.22954](https://arxiv.org/abs/2505.22954); overview <https://sakana.ai/dgm/>.
- Red Queen Gödel Machine (RQGM): Co-Evolving Agents and Their Evaluators — Iacob, Jovanović, Shen, … Lane (Cambridge et al.). arXiv [2606.26294](https://arxiv.org/abs/2606.26294) (v3 2026-10-08, preliminary); news <https://www.cst.cam.ac.uk/news/red-queen-hypothesis-new-way-forward-self-improving-ai>.
- `kingkillery/red-queen-godel-machine` — **unofficial**, independent Python implementation of RQGM (MIT, zero-dependency core, 2 commits, 3 stars at time of reading). Official code not yet released per the Cambridge article.

Only abstracts, project pages and the README were read; details below are limited to what those state.

## What they did that A²D has not

| | DGM | RQGM | A²D today |
|---|---|---|---|
| Self-modification target | the agent's own Python harness (tools, workflow) | agent **and** its evaluator | enzyme prompts (evolver), metabolism source (architect) |
| Benchmark headroom | SWE-bench 20.0% → 50.0%; Polyglot 14.2% → 30.7% | held-out DeepSWE 82.1% vs 75.0% fixed-evaluator baseline (low effort) | sudoku, already 100% one-shot |
| Search | archive of all agents; parents sampled from the archive (open-ended, keeps weak stepping stones) | archive + clade-level Beta-posterior "best-belief" selection, UCB-style expansion gate | single lineage with a fitness ratchet (never commits lower fitness) = hill-climbing |
| Ablations reported | without self-improvement, without open-endedness: both worse | fixed evaluator vs co-evolved evaluator | none against a baseline |
| Evaluator | fixed benchmark | frozen within an epoch; replaced at exponentially spaced checkpoints only if strictly better on a trusted **anchor** set; old scores erased | hidden holdouts used for both selection and final score |
| Known failure mode | objective hacking: faked test logs; removed hallucination-detection markers | evaluator drift if anchors are weak; self-preference bias (mitigated by an adversarial objective) | oracle leakage (holdouts drive selection) |

## Implications

1. **Both results are only legible because the benchmarks had headroom and fixed baselines.** This confirms the audit's first-order fix: no architecture change matters until A²D runs on a non-saturated benchmark with pre-registered baselines (audit next moves 1–4). DGM's own benchmarks (SWE-bench, Polyglot) are candidates; Polyglot is multi-language exercise-style and cheaper per task than SWE-bench. *(Inference: verify Polyglot's current availability and harness.)*
2. **Replace the ratchet with an archive.** DGM's open-endedness ablation is direct evidence against A²D's monotonic fitness ratchet. Lineage is already a git archive; parent selection from it (performance- and novelty-weighted, or RQGM's clade best-belief) is a contained change to the germline/lineage layer, testable by the audit's ablation protocol.
3. **The tester enzyme is the natural evaluator slot.** It is currently a dead end: the seed tester produces `test_results`, lineage normalization deliberately strips `test_results` from the evolver's reactants and catalysts so it learns from mechanical fitness instead (`crates/a2d-cli/src/main.rs:2026-2040`), no enzyme in the 7-enzyme lineage topology consumes it, and in non-test core code it appears only in scheduling priority (`crates/a2d-core/src/metabolism.rs:886`). It never reaches fitness or candidate selection. RQGM gives it a role: a co-evolved evaluator whose verdicts drive selection, frozen per epoch, swapped only when it beats the incumbent on anchors. This also implements the audit's oracle separation, with a three-way split:
   - **anchors** (labelled ground truth) — gate evaluator replacement only;
   - **selection** — the evolved evaluator plus public tests;
   - **final hidden holdouts** — scoring only, never seen by any loop.
4. **Theoretical fit.** RQGM puts the evaluator inside the self-producing system, which is closure to efficient causation applied to the judge — the A²/A²D thesis. The anchor gate is the mechanical guard that keeps that closure from collapsing into DGM-style objective hacking. Foundry's red/green structure is the existing in-house analogue of RQGM's adversarial reviewer objective.
5. **Harness-level self-improvement is where DGM found gains** (patch validation, file viewing/editing tools, multi-candidate generation + ranking, attempt history). A²D hand-built two of these (portfolio, `failure_report`). If A²D's architect is to compound (audit H3), its search space should include the coder's harness, not just metabolism internals — but only after the benchmark exists to measure it.
6. **Dependencies.** Per the project's no-unnecessary-dependencies principle, borrow patterns rather than depend on the unofficial repo: Beta-posterior best-belief scoring, utility ledger with selective erasure on evaluator swap, exponentially spaced checkpoints, UCB-style expansion gate. Its README caveats (guarantees hold only within an epoch; anchor quality is critical; binary outcomes only) apply to any port.

## Suggested sequencing

Unchanged prerequisites from the audit (oracle separation, durable baselines, non-saturated benchmark, lift test). Then, as the first architecture experiments on that benchmark: (a) archive-based parent selection vs the ratchet; (b) anchor-gated co-evolved tester vs fixed public tests. Each is a two-arm ablation with the same pre-registered falsifier as the lift test.
