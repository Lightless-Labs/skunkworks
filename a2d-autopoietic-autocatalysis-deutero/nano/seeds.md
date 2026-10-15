# Nanoharness seed list (human insight + prior evidence)

Candidate changes to `nano/harness.py`, one experiment each, with the evidence that motivates them. Proposers (Claude, Codex Sol, later A²D's architect) may take, combine or ignore these. Every candidate goes through `nano/eval.py` (A/A-calibrated keep, then a confirmation replicate). Evidence is cited by LOG entry.

## Selection: add information from beyond the coder
1. **Sample 4, judge 1 (Sol).** C2-E1 offline: the judge picked a hidden-passing file in 39/39 decisive tuples (Claude 37/39); +10.3 pp vs D on train. → E002, with the E003 random-green control.
2. **Repair to green first, then judge.** Keep D's repair loop until public-green (D reaches green on about 99% of tasks), spend the leftover calls on fresh samples, and let the judge pick among the distinct green files. This keeps D's strength and adds selection only where calls are left.
3. **Judge with a clause checklist.** The judge prompt asks it to walk the numbered requirements and name the first clause each candidate violates before picking. Claude's judge reasons were clause-level (C2-E1).
4. **Judge-guided repair.** The judge names the violated clause; one coder call fixes the chosen file against that clause. This is S2's clause review with a stronger reviewer (S2 failed with Luna reviewing itself: conv 0.19).

## Prompting the coder
5. **Clause-check system prompt on the first attempt only.** H0's clause-check prompt raised first-attempt public-green from 0.77 to 0.82 (E10 diagnosis), but its repair instruction hurt repairs. Use it for attempt 1 and D's prompt for repairs.
6. **Researcher lessons in the task prompt.** S4b: +5.6 pp vs D on clean train (four lessons: precedence, coordinates, unchecked sizes, checked arithmetic). Disclosed as human insight.
7. **Self-written lessons** (`a2d distill` output, `harnesses/p4-*.json`): E12 showed no gain vs D. Low prior.

## Budget allocation
8. **Diversity across samples:** vary the sampling prompt per sample (for example "write it as a state machine" or "validate everything first") so the judge has more distinct candidates to choose from.
9. **Early stop for the judge:** if the first two samples are green and identical in behaviour on the public tests, skip further samples. This saves calls, but only helps a cost claim.

## Known dead ends at this regime (do not re-run without a new reason)
- Luna writing its own tests (S1, S1b), Luna reviewing its own code (S2), majority or agreement selection among Luna samples (S3), the seed metabolism's evolver and escalation (E10 diagnosis).
