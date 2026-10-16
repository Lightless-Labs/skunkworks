You are one of several proposers in an autoresearch loop on a minimal coding harness (A²D cycle 2).
Everything you need is in this message; you have no tools and no files.

## The harness protocol (controller side is fixed)
- One Python 3 process per task, run with `python3 -I` in an empty directory. JSON lines over stdin/stdout.
- First line in: {"task": {"id", "family", "task_prompt", "coder_system", "k", "max_feedback_chars"}}.
  task_prompt = the full task as the coder sees it (numbered requirements + the visible tests). k = 5.
- Requests (one JSON object per line on stdout; one reply line on stdin):
  - {"op": "model", "system": str, "prompt": str} -> {"ok": bool, "text": str, "error"?}. Default lane = the coder,
    codex/gpt-6-luna at low effort ("Luna"). Add "lane": "<name>" to call a lane declared in a header comment
    `# lanes: <name>=codex/<model>:<effort>`; the stronger model available is codex/gpt-6.1-sol ("Sol").
    EVERY model call, on any lane, counts against the same per-task cap k = 5. Calls beyond k fail.
    Model calls have no tools and no memory between calls.
  - {"op": "extract", "text": str} -> {"code": str | null}   (the rust code block of a reply)
  - {"op": "public_test", "code": str} -> {"compiled": bool, "passed": int, "expected": int, "all_passed": bool,
    "output": str}   (visible tests only; free, not metered)
  - {"op": "done", "codes": [str, ...]} -> ends the task. The controller public-tests each code and submits the
    one with the best public result (compiled, then tests passed); ties go to the LATER code in the list.
- Score: hidden acceptance tests (never visible to the harness) on the submitted file; primary metric = all-pass
  (every hidden test passes), secondary = fraction of hidden tests passed.
- Isolation guard (static, enforced before any run): imports only from {json, re, sys, random, math,
  itertools, collections, functools, hashlib, difflib, textwrap, string, heapq, dataclasses, typing};
  no calls to open, exec, eval, compile, __import__, globals, locals, vars, getattr, setattr, delattr,
  input, breakpoint; no dunder attribute access except __name__. Diagnostics go to stderr as lines
  starting with "DIAG ".

## The incumbent (nano/harness.py, E002: 4 Luna samples + Sol judge)

```python
#!/usr/bin/env python3
# lanes: judge=codex/gpt-6.1-sol:medium
"""Nanoharness candidate E002: sample 4, judge 1 (from C2-E1).

Hypothesis: four independent coder samples, public-tested, plus ONE call to
a stronger reviewer that picks among the distinct public-green files from the
spec alone, pass hidden tests more often than the plain repair loop at the
same 5-call budget. Offline on the frozen train pool: 0.831 (Claude judge) and
0.846 (Sol judge) vs D 0.743 (LOG C2-E1).

Budget per task (k = 5): 4 coder samples, then either 1 judge call (two or
more distinct green files), 1 repair call (no green file), or nothing (one
green file). The judge is the controller lane named "judge" (Sol, medium
effort, no tools). Candidates are shown in a seeded shuffle (seed = task id);
the reply must name exactly one shown letter, otherwise the fallback (first
green in generation order) is used and recorded. One DIAG line per task goes
to stderr: branch, shown order, pick, fallback reason.
"""
import json
import random
import re
import sys

JUDGE = """You are reviewing candidate solutions to a programming task.

The task, exactly as the programmer received it (numbered requirements and the visible tests), is under TASK. The files under CANDIDATES are independent candidate solutions, and all of them pass the visible tests. Hidden acceptance tests check every numbered requirement.

Pick the single file most likely to pass all hidden tests. Read the requirements carefully and check each candidate against them, concentrating on where the candidates differ.

Reply with ONE JSON object and nothing else:
{"pick": "<letter>", "reason": "<one sentence>"}"""


def request(op, **fields):
    sys.stdout.write(json.dumps({"op": op, **fields}) + "\n")
    sys.stdout.flush()
    if op == "done":
        return None
    return json.loads(sys.stdin.readline())


def truncate(text, limit):
    data = text.encode("utf-8")
    if len(data) <= limit:
        return text
    end = limit
    while end > 0 and (data[end] & 0xC0) == 0x80:
        end -= 1
    return data[:end].decode("utf-8") + "\n... [truncated]"


def repair_prompt(base, code, public, limit):
    prompt = base
    if code is None:
        prompt += "\n\n## Previous attempt\n\nYour previous response contained no ```rust code block.\n"
    else:
        prompt += "\n\n## Previous attempt\n\n```rust\n" + code.rstrip() + "\n```\n\n## Public test results\n\n"
        feedback = public["output"] if public is not None else "(this task has no public tests)"
        prompt += truncate(feedback, limit)
    prompt += "\n\nFix the file. Respond with the whole corrected file in a single ```rust block.\n"
    return prompt


def sample(task, prompt):
    reply = request("model", system=task["coder_system"], prompt=prompt)
    code = request("extract", text=reply["text"])["code"] if reply.get("ok") else None
    public = request("public_test", code=code) if code is not None else None
    return code, public


def rank(entry):
    _, public = entry
    return (public["passed"], public["compiled"]) if public else (0, False)


def judge(task, greens, diag):
    letters = "ABCDEFGH"[: len(greens)]
    order = list(range(len(greens)))
    random.Random(task["id"]).shuffle(order)
    shown = [greens[i] for i in order]
    diag["shown_order"] = order
    body = "TASK:\n" + task["task_prompt"] + "\n\nCANDIDATES:\n"
    for letter, code in zip(letters, shown):
        body += f"\n### {letter}.rs\n```rust\n{code.rstrip()}\n```\n"
    reply = request("model", lane="judge", system=JUDGE, prompt=body)
    if not reply.get("ok"):
        diag["fallback"] = "judge call failed: " + str(reply.get("error", ""))[:120]
        return greens[0]
    picks = re.findall(r'"pick"\s*:\s*"([^"]*)"', reply["text"])
    if len(picks) != 1 or picks[0].strip().upper() not in letters or len(picks[0].strip()) != 1:
        diag["fallback"] = "unparseable pick: " + reply["text"][:120]
        return greens[0]
    letter = picks[0].strip().upper()
    diag["pick"] = letter
    return shown[letters.index(letter)]


def main():
    task = json.loads(sys.stdin.readline())["task"]
    base = task["task_prompt"]
    diag = {"task": task["id"]}
    entries = [sample(task, base) for _ in range(min(4, task["k"]))]
    greens = []
    for code, public in entries:
        if public and public.get("all_passed") and code not in greens:
            greens.append(code)
    diag["greens"] = len(greens)
    if len(greens) >= 2 and task["k"] >= 5:
        diag["branch"] = "judge"
        chosen = judge(task, greens, diag)
        print("DIAG " + json.dumps(diag), file=sys.stderr, flush=True)
        request("done", codes=[chosen])
        return
    if greens:
        diag["branch"] = "one-green"
        print("DIAG " + json.dumps(diag), file=sys.stderr, flush=True)
        request("done", codes=[greens[0]])
        return
    diag["branch"] = "no-green-repair"
    best = max(entries, key=rank)
    codes = [c for c, _ in entries if c is not None]
    if task["k"] >= 5:
        code, public = sample(task, repair_prompt(base, best[0], best[1], task["max_feedback_chars"]))
        if code is not None:
            codes.append(code)
    print("DIAG " + json.dumps(diag), file=sys.stderr, flush=True)
    request("done", codes=codes)


if __name__ == "__main__":
    main()
```

## The baseline D (Luna's plain repair loop, as harness v0)

```python
#!/usr/bin/env python3
"""Nanoharness: the coder harness as one editable file (A²D cycle 2).

v0 is the plain repair loop (lift arm D), byte for byte: the same system
prompt, task prompt, repair prompt, feedback truncation and stopping rule.
Every later change is one experiment, kept only if it beats the incumbent
above the A/A threshold (docs/experiments/LOG.md, Cycle 2).

The controller runs this file as an external arm (`--external name=path`)
and speaks JSON lines on stdin/stdout:
  first line in:   {"task": {"id", "family", "task_prompt", "coder_system",
                             "k", "max_feedback_chars"}}
  requests out:    {"op": "model", "system", "prompt"}   -> {"ok", "text"|"error"}
                   {"op": "extract", "text"}              -> {"code"}
                   {"op": "public_test", "code"}          -> TestScore
                   {"op": "done", "codes": [...]}         (ends the episode)
Model calls are metered by the controller (budget k per task). Hidden tests
are never reachable from here.
"""
import json
import sys


def request(op, **fields):
    sys.stdout.write(json.dumps({"op": op, **fields}) + "\n")
    sys.stdout.flush()
    if op == "done":
        return None
    return json.loads(sys.stdin.readline())


def truncate(text, limit):
    """As arms.rs `truncate`: a byte limit, cut back to a char boundary."""
    data = text.encode("utf-8")
    if len(data) <= limit:
        return text
    end = limit
    while end > 0 and (data[end] & 0xC0) == 0x80:
        end -= 1
    return data[:end].decode("utf-8") + "\n... [truncated]"


def repair_prompt(base, code, public, limit):
    """As arms.rs `repair_prompt`."""
    prompt = base
    if code is None:
        prompt += "\n\n## Previous attempt\n\nYour previous response contained no ```rust code block.\n"
    else:
        prompt += "\n\n## Previous attempt\n\n```rust\n" + code.rstrip() + "\n```\n\n## Public test results\n\n"
        feedback = public["output"] if public is not None else "(this task has no public tests)"
        prompt += truncate(feedback, limit)
    prompt += "\n\nFix the file. Respond with the whole corrected file in a single ```rust block.\n"
    return prompt


def main():
    task = json.loads(sys.stdin.readline())["task"]
    base = task["task_prompt"]
    codes = []
    last = None  # (code, public score) of the previous attempt
    for _ in range(task["k"]):
        prompt = base if last is None else repair_prompt(base, last[0], last[1], task["max_feedback_chars"])
        reply = request("model", system=task["coder_system"], prompt=prompt)
        code = request("extract", text=reply["text"])["code"] if reply.get("ok") else None
        public = request("public_test", code=code) if code is not None else None
        if code is not None:
            codes.append(code)
        last = (code, public)
        if public is not None and public.get("all_passed"):
            break
    request("done", codes=codes)


if __name__ == "__main__":
    main()
```

## Aggregate evidence about the incumbent (dev set; no per-task data)

```json
{
 "source": "E002 harness (X:cand) in E002 screen + E002b rest + E002b confirm: 280 task-runs over the 140 dev tasks (each task twice); every coder sample re-scored offline on public and hidden tests. Aggregates only.",
 "all_pass": {
  "incumbent_E002": 0.857,
  "v0_equals_D_same_runs": 0.768
 },
 "branches_of_E002": {
  "judge (>=2 distinct public-green files)": 249,
  "one-green": 23,
  "no-green-repair": 8
 },
 "judge_quality": "Across all 249 judge-branch task-runs the judge never picked a hidden-failing file when a hidden-passing green existed (0 misses). E002's all-pass equals the any-pass ceiling of its own samples (0.857).",
 "E002_failures_40_of_280": {
  "judge branch, every public-green sample fails hidden (shared misreading)": 21,
  "one-green branch, that green fails hidden": 13,
  "no public-green sample after 4; repair attempted": 6
 },
 "failure_kind_of_submitted_file": {
  "some hidden tests fail": 30,
  "hidden tests time out (too slow on larger hidden inputs)": 8,
  "fails to compile against hidden tests": 2
 },
 "per_sample_hidden_all_pass_rate": 0.643,
 "any_public_green_and_hidden_pass_among_k_samples (exact over subsets)": {
  "k=1": 0.643,
  "k=2": 0.764,
  "k=3": 0.819,
  "k=4": 0.85
 },
 "any_public_green_among_k_samples": {
  "k=1": 0.815,
  "k=2": 0.915,
  "k=3": 0.95,
  "k=4": 0.972
 },
 "calls_per_task_E002": {
  "luna": 4.03,
  "sol": 0.9
 },
 "failure_taxonomy_of_E002 (34 failing task-runs, Claude diagnosis from spec + code + pass/fail label only)": {
  "categories": {
   "boundary/edge-case": 9,
   "semantics/state-model": 9,
   "performance/complexity": 8,
   "order-of-checks/precedence": 3,
   "exact-format/output": 3,
   "api/contract": 1,
   "overflow/arithmetic": 1
  },
  "misreading_shared_by_all_files": "29 of 34",
  "recurring_task_agnostic_patterns (cases)": {
   "zero-dimension or huge-declared-size loops (bound work by actual element count; short-circuit empty)": 5,
   "brute-force searches with no result (estimate worst case; prune by coarse units)": 2,
   "preallocation from caller-supplied counts (with_capacity(n) on untrusted n)": 3,
   "literal examples in the spec not checked against the code": 3,
   "sentinel values that are also legal data": 2,
   "precedence chains not implemented exactly as listed": 3,
   "streaming/chunked state not equal to one-shot": 2,
   "output templates: literal words vs placeholders": 2
  }
 },
 "cross_family_judge_check": "Offline, a Claude Opus judge with E002's judge prompt picked a hidden-passing file in 66/68 decisive tuples (Sol live 68/68; random 36.3). Selection is at its ceiling.",
 "already_pending_from_other_proposers (do not duplicate)": [
  "E005: diverse sampling angles for samples 2-4 (requirement-by-requirement reading; adversarial edge cases + checked arithmetic; literal precedence)",
  "E006: one Sol spec-analysis call (misreading notes, no code) before sampling, then 3 Luna samples with the notes + the same judge",
  "E007: 3 Luna drafts, one Sol call that selects and lists concrete defects (no code), one Luna repair of the pick",
  "E008: 4th Luna sample synthesizes a solution from drafts 1-3 (with their public results), then the same judge",
  "declined: raising Luna's reasoning effort (changes the coder configuration, out of scope)"
 ]
}
```

## Seed list (human insight and prior evidence)

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

## Results log (nano/results.tsv)

```
id	date	proposer	stage	hypothesis	delta_frac	se	delta_allpass	se_allpass	threshold	n_tasks	n_families	decision	invocations	calls_by_lane	tokens	candidate_sha	note
C2-AA	2026-10-15	-	aa	A/A calibration: incumbent v0 (= D) vs itself, plus parity vs native D	+0.0067	0.0143	+0.0000	0.0433	allpass 0.0714 / frac 0.0275	140	35	calibration (parity pass)	576	{"D/codex/gpt-6-luna": 195, "X:a/codex/gpt-6-luna": 190, "X:b/codex/gpt-6-luna": 191}	4513270	200daaa763c4	X:a-D allpass -0.0143; first prompts 280/280 identical
E002	2026-10-16	claude	screen	Sample 4 + one Sol judge call over distinct public-green files beats the repair loop at the same 5-call cap (C2-E1 offline: +8.8 to +10.3 pp).	+0.0050	0.0164	+0.1176	0.0470	allpass 0.0714 / frac 0.0275	68	17	discard (screen, v2.1 fraction rule)	431	{"X:cand/codex/gpt-6-luna": 272, "X:cand/codex/gpt-6.1-sol": 62, "X:inc/codex/gpt-6-luna": 97}	3461459	e772bd6e99f6	re-registered as E002b under v2.2 (all-pass primary)
E004	2026-10-16	claude	screen	Repair to green as D, spend leftover calls on fresh samples, Sol judge picks among distinct greens; keeps D's green rate and adds selection.	-0.0189	0.0149	+0.0441	0.0382	allpass 0.0714 / frac 0.0275	68	17	superseded (screen observed under v2.1; incumbent changed)	421	{"X:cand/codex/gpt-6-luna": 278, "X:cand/codex/gpt-6.1-sol": 57, "X:inc/codex/gpt-6-luna": 86}	3350738	1abe3a9cbc51	not run further; against the new incumbent it would be a new registration
E002b	2026-10-16	claude	full	Re-registration of E002 under eval v2.2 (all-pass primary): sample 4 + Sol judge beats the repair loop; confirmation replicate is the test.	+0.0014	0.0100	+0.1071	0.0293	allpass 0.0714 / frac 0.0275	140	35	provisional keep	879	{"X:cand/codex/gpt-6-luna": 564, "X:cand/codex/gpt-6.1-sol": 128, "X:inc/codex/gpt-6-luna": 187}	6692119	e772bd6e99f6	
E003	2026-10-16	claude	screen	Attribution control for E002: same 4-sample policy, seeded random green instead of the judge (no fifth call).	-0.0071	0.0094	+0.0441	0.0480	0.0275	68	17	discard (screen)	366	{"X:cand/codex/gpt-6-luna": 274, "X:inc/codex/gpt-6-luna": 92}	2496335	281ac783b065	
E003b	2026-10-16	claude	full	Re-registration of E003 (random-green control) under eval v2.2 (all-pass primary).	-0.0102	0.0086	-0.0143	0.0336	allpass 0.0714 / frac 0.0275	140	35	discard (full)	739	{"X:cand/codex/gpt-6-luna": 563, "X:inc/codex/gpt-6-luna": 176}	4691192	281ac783b065	
E002b	2026-10-16	claude	confirm	Re-registration of E002 under eval v2.2 (all-pass primary): sample 4 + Sol judge beats the repair loop; confirmation replicate is the test.	+0.0017	0.0059	+0.0714	0.0350	allpass 0.0714 / frac 0.0275	140	35	keep	869	{"X:cand/codex/gpt-6-luna": 564, "X:cand/codex/gpt-6.1-sol": 121, "X:inc/codex/gpt-6-luna": 184}	6375705	e772bd6e99f6	
r1-1	2026-10-16	sol	proposal	Raising Luna's reasoning effort from low to high, with the sampling and judge policy unchanged, raises all-pass by roughly 4-8 pp.								declined (out of scope: changes the coder configuration, not the harness)	0				fair comparator would be D at high effort; kept as nano/candidates/declined-r1-1-higher-effort-luna.py
```

## Experiment log, cycle 2

## Cycle 2 — autoresearch on a minimal coding harness (goal set 2026-10-15)

### C2-E1 — does a second model family see what Luna can't? Cross-family judge over Luna's candidates  [cycle 2, experiment 1, offline]
- Registered: 2026-10-15, before any judge call.
- **Question** (goal setup step 3): can a Claude judge reading only the spec (requirements + public tests) pick a hidden-passing file among Luna's public-green candidates better than the plain repair loop does?
- **Data:** the frozen S0 pool (`runs/2026-10-15-s0-candidate-pool/`): E07 and E07b on the 17 c1-train families, 136 tuples.
  - **Equal-budget arm:** the first 4 of B's samples plus 1 judge call = 5 invocations, the same as D.
  - The judge picks among the public-green candidates. With fewer than 2 green candidates, the public rule picks.
- **Only the 39 decisive tuples are judged:** those with ≥ 2 green candidates that differ in hidden outcome. Every other tuple's outcome is fixed whatever the pick: 76 passes.
  - The judge gets the identical prompt for every tuple and is never told that the candidates differ.
  - Candidates are shuffled with a seed and shown without metadata.
  - No hidden test, hidden output or label is ever shown.
- **Judges:**
  - **primary:** Claude (Agent subagent, `sonnet`), one call per tuple;
  - **secondary:** Codex `gpt-6.1-sol` (medium), to separate model family from model strength.
- **Rule (from the goal):** the judge pick rate = (76 + correct) / 136. **If the Claude judge's rate is ≥ D's pick rate + 5 pp = 0.793 (≥ 32 of 39 correct),** cross-model review is the first lever, claimed as "multi-model system vs single model at equal invocation budget". Otherwise the loop moves to a tool-using multi-file regime.
- **References:** D pick 0.743; B4 public pick 0.699; random pick among green candidates 0.704; perfect judge 0.846.
- Budget: 39 Claude calls and 39 Sol calls.
- Result (`runs/2026-10-15-c2e1-judge/`: answer key, Claude picks, Sol picks with reasons):
  - **Claude judge (primary, Sonnet): 37/39 correct, pick rate (76 + 37) / 136 = 0.831.** That is ≥ 0.793, so **the rule is met: cross-model review is the first lever.**
  - **Sol judge (secondary): 39/39 correct, pick rate 0.846** (the ceiling: any passing candidate among B's first 4).
  - References: D 0.743; B4 public pick 0.699; random green pick 0.704 (19.8/39 expected).
  - **Leakage audit (Sol):** every executed command was `cat`, `ls` or `rg` inside its own tuple dir, with no path escapes and no code execution.
    - The Claude subagents were instructed to use Read only, inside their dirs. Their tool calls were not audited; their stated reasons are clause-level code analysis.
  - **Caveats:**
    - This is an offline estimate on train (c1-train families, the S0 pool).
    - The equal-budget arm is 4 Luna samples plus 1 judge call = 5 invocations, but a judge call reads 3–5 files, so tokens per task are higher, and that must be reported.
    - Sol is a stronger model from the same vendor as Luna. For Sol, the information source is "a stronger reviewer" rather than "a different family".
  - **Next:** a live nanoharness experiment, "sample 4, judge 1". The Claude CLI is installed (2.1.301), but from a subprocess it answers `Not logged in`. The live judge is therefore `codex/gpt-6.1-sol` (the stronger judge here, 39/39), disclosed as "multi-model: Luna writes, Sol picks". A Claude-lane variant follows if CLI auth becomes available to subprocesses.

### C2-AA — A/A threshold and native-D parity for nanoharness v0  [cycle 2, eval calibration]
- Registered: 2026-10-15, before the run. Prerequisite: the zero-call parity fixture test passes (native D vs external v0 under a deterministic mock: identical prompts and identical submission).
- One run on `tasks/train-all` (140 tasks): arms D, X:a, X:b. Both X arms are `nano/harness.py` v0. k = 5, seed 4242.
- Outputs:
  - **threshold** = 95th percentile of the family-bootstrap null of X:b − X:a (centred) in hidden-test fraction;
  - **parity** = X:a − D and X:b − D.
- Parity passes if both all-pass deltas are within ±5 pp **and** the first coder prompts are byte-identical to D's on every task (checked from the logs). If parity fails, no candidate runs until v0 is fixed.
- Budget: ≤ 700 Luna invocations.
- **Prerequisites met (2026-10-15):**
  - **External arms** (`--external name=path`, arm `X:<name>`) were implemented by Codex `gpt-6.1-sol` from a written spec, reviewed, and merged.
  - **Judge lanes** (`--lane name=codex/<model>[:effort]`): a model op with a `lane` field is metered against the same per-task budget, logged as enzyme `lane:<name>`, and runs on the no-tools Codex lane.
  - **Zero-call parity gate passes.** `nano_harness_v0_matches_native_d_prompts_and_submission` runs the real `nano/harness.py` against native D under a deterministic mock, through both repair branches (no code block, then failing tests). The prompt sequences are identical once per-call oracle randomness is normalised (temp paths, the randomised public module name, thread ids), and so are the submissions.
  - **An operational fix found on the way:** Apple's `/usr/bin/python3` is an `xcrun` shim, which with a cleared environment spent about 12 s per task before Python started. The controller now resolves the real interpreter once, and keeps `TMPDIR`.
  - **Isolation guard:** `eval.py` statically rejects any harness importing outside a stdlib allowlist or calling `open`, `exec`, `eval`, `__import__`, `getattr` and similar. Hidden tests are on this machine, so a proposed harness must not be able to read them.
- Launched 2026-10-15, out `runs/c2-aa-200daaa763c4/` (incumbent sha `200daaa763c4…`).
- Result (`nano/aa.json`, `runs/c2-aa-200daaa763c4/`; 140 tasks, one draw per task):
  - **Threshold = +0.0275 in hidden-test fraction** (95th percentile of the family-bootstrap null of X:b − X:a, centred). A keep therefore needs Δfrac > max(0.0275, 1.645 · SE).
  - The A/A raw difference is +0.0067 frac and 0.0 pp all-pass.
  - **Parity with native D: PASS.**
    - X:a − D = −0.0091 frac (SE 0.0137) and −1.4 pp all-pass;
    - X:b − D = −0.0024 frac (SE 0.0149) and −1.4 pp all-pass.
    - Both are within ±5 pp, and **first prompts are byte-identical on 280/280 task-arm pairs**.
  - Spend: 576 Luna invocations (D 195, X:a 190, X:b 191) and 4.51M tokens.
  - Luna call latency was about 50–100 s under account contention, against 25–30 s in cycle 1.

### E002 — sample 4, judge 1 (Luna generation plus Sol selection) / E003 — sample 4, random green (control)  [cycle 2]
- Registered: 2026-10-15, before any live run. Both go through `nano/eval.py run` (eval v2.1: screen, full, confirmation replicate), each against the A/A-calibrated incumbent v0.
- **E002 hypothesis:** four Luna samples plus one Sol judge call over the distinct public-green files pass hidden tests more often than the repair loop (v0 = D) at the same 5-call cap. Offline (C2-E1): +8.8 pp (Claude judge), +10.3 pp (Sol judge) on train.
  - Judge lane `codex/gpt-6.1-sol:medium`, no tools, empty cwd.
  - Candidates are shown in a shuffle seeded by task id. Strict parsing; on a parse failure the fallback is the first green file, and it is logged.
- **E003 (attribution control):** identical except a seeded random green replaces the judge (no fifth call).
- **Reported:**
  - Δfrac and Δall-pass vs v0, with cluster SE;
  - E002 − E003, paired across their runs on the same tasks;
  - branch frequencies (judge / one-green / no-green-repair), judge fallbacks;
  - calls and tokens by lane per arm.
- **Claim wording if kept:** "Luna generation plus Sol selection beats Luna's repair loop at an equal call cap" (exploratory; dev set). Not a different-family claim.
- Budget: ≤ 1,500 Luna and ≤ 300 Sol invocations for both.
- **Launched 2026-10-16:** E002 and E004 screens, alongside C2-AA (eval generates the screen before the threshold exists and decides later).
- **Early check passed:** coder samples use D's base prompt; the judge prompt has the TASK and CANDIDATES structure, with 2–4 candidates in a seeded shuffled order; picks parse strictly; the judge's reasons are clause-level; DIAG lines record the branch for each task.
- Result: pending

### E004 — repair to green, then sample and judge  [cycle 2, seed 2]
- Registered: 2026-10-15, before any live candidate run. It goes through `nano/eval.py run` (eval v2.1) against incumbent v0, like E002 and E003.
- **Hypothesis:** keeping D's repair loop until public-green and spending the leftover calls on fresh samples, with the final call reserved for a Sol judge over the distinct green files, beats D. It may also beat E002, because D's ~99% green rate is preserved and selection is added on top.
- If the repair loop never goes green, the arm behaves exactly like D.
- **Reported:**
  - Δfrac and Δall-pass vs v0;
  - E004 vs E002 (full runs, same tasks);
  - branch frequencies (judge / first-green / never-green), mean repair calls, judge fallbacks;
  - calls and tokens by lane.
- Budget: ≤ 1,000 Luna and ≤ 150 Sol invocations.
- Result: pending

### E002 / E004 screen results under eval v2.1 (fraction rule), and the metric amendment  [cycle 2, 2026-10-16]
- **E002 screen** (68 tasks; `runs/c2-E002/screen`):
  - Δfrac = +0.0050 (SE 0.0164). **Δall-pass = +11.8 pp (SE 4.7)**: net +8 tasks (11 won, 3 lost).
  - **Registered decision: discard (screen)**, because Δfrac < threshold / 2 = 0.0137.
  - Diagnostics: the judge branch ran on 62 tasks, one-green on 6, no-green-repair on 0; 1 judge fallback (a timeout); 4 coder calls on every task.
  - The fraction delta splits as follows:
    - won tasks: +1.29 in total;
    - lost tasks: −1.135 in total, three catastrophic failures (mean −0.38, near-zero fraction from timeouts or a compile failure);
    - joint failures: +0.19.
  - The divergence is real and comes from the kind of treatment. Unrepaired, judge-picked samples pass more tasks completely, but the files that fail can fail badly. A fraction mean is dominated by a few catastrophic tasks, while all-pass counts every failure the same.
  - Spend: 431 invocations (cand: Luna 272 and Sol 62; inc: 97) and 3.46M tokens.
- **E004 screen** (68 tasks): Δfrac = −0.0189 (SE 0.0149), Δall-pass = +4.4 pp (SE 3.8). Undecided under v2.1 (launched before the threshold existed). Spend: 421 invocations (cand: Luna 278 and Sol 57; inc: 86).
- **Metric amendment (eval v2.2, pre-registered here before any rest or confirmation data):**
  - **The primary keep metric becomes all-pass** (working software), with its own A/A threshold from the existing C2-AA run: **+0.0714, i.e. 10 net tasks of 140** (`threshold_allpass` in `nano/aa.json`).
  - The rules on all-pass: screen ≥ threshold / 2; full Δall-pass > max(threshold, 1.645 · cluster SE), with Δfrac not significantly negative; confirmation Δall-pass > threshold / 2.
  - Hidden-test fraction stays a reported secondary.
  - **Reason:** fraction was chosen for sample efficiency (2.8× lower A/A noise relative to effect), and its correlation with all-pass (0.75) came from 51 arms that were all repair-loop variants. It disagreed with all-pass on the first selection-type candidate.
  - **Disclosure:** the goal text names fraction as the search metric. This is a disclosed deviation from my own design choice, made after observing E002's screen.
- **Re-registrations under v2.2.** Same candidate files, each reusing its already observed screen stage, with fresh rest and confirmation runs:
  - **E002b** (from E002), **E004b** (from E004), and **E003b** (from E003 once its screen completes).
  - The screens were observed before the rule changed; E002's +11.8 pp is why it changed.
  - **The confirmation replicate (fresh, 140 tasks) is the actual test.**

### E002b — full stage (eval v2.2)  [cycle 2, 2026-10-16]
- The screen (reused from E002) passed the all-pass screen: +11.8 pp ≥ threshold / 2 = 3.57 pp.
- **Full (140 tasks, screen + fresh rest): Δall-pass = +10.7 pp (cluster SE 2.9), Δfrac = +0.0014 (SE 0.010). Provisional keep** (+10.7 > max(7.1, 4.8); fraction not negative).
- The fresh 140-task confirmation replicate is running. Promotion needs its Δall-pass > 3.57 pp.
- Spend so far: 879 invocations (cand: Luna 564 and Sol 128; inc: Luna 187) and 6.69M tokens.
  - That is about 4.03 Luna + 0.91 Sol calls per task for the candidate, against about 1.34 for the incumbent.
  - At an equal invocation cap, the candidate is about 3.7× the incumbent's realised calls, so this is not a cost-matched claim.
- Budget: cycle-2 Luna spend is 275 calls (10-15) and 1,784 (10-16 so far), plus 186 Sol on 10-16. Today's cap of 3k is respected; E003b and E004b's later stages are scheduled for 10-17 (UTC).

### E002b — CONFIRMED KEEP; E003b — discard; E004 superseded  [cycle 2, 2026-10-16]
- **E002b confirmation** (a fresh 140-task replicate): **Δall-pass = +7.1 pp (cluster SE 3.5)** > threshold / 2 = 3.57 pp, so **KEEP**. Δfrac = +0.0017 (SE 0.0059).
  - Spend: 869 invocations (cand: Luna 564 and Sol 121; inc: Luna 184) and 6.38M tokens.
  - Every stage is positive:
    - screen +11.8 pp (68 tasks);
    - full +10.7 pp (SE 2.9, 140 tasks);
    - confirm +7.1 pp (SE 3.5, 140 fresh draws).
  - **`nano/harness.py` is now E002** ("Luna generation plus Sol selection": 4 samples, then a Sol judge over distinct green files). v0 is archived at `nano/history/before-E002b.py`.
- **E003b** (random-green control, full stage, 140 tasks): **Δall-pass = −1.4 pp (SE 3.4)**, Δfrac = −0.010, so **discard (full)**. Spend: 739 invocations (Luna 563 cand + 176 inc).
  - **Attribution:** with the same 4-sample policy, the Sol judge gives about +11 pp at the full stage (E002b +10.7 vs E003b −1.4, same 140 tasks), and the random pick gives nothing. **The judge is the active ingredient, not the sampling.**
- **E004** (repair to green, then judge): its screen was +4.4 pp all-pass and −0.019 frac against v0. It was registered against v0, which is no longer the incumbent, so it is **superseded and not run further**. A comparison against the new incumbent would be a new registration.
- **Claim scope (exploratory, dev set):**
  - Luna generation plus Sol selection beats Luna's plain repair loop on all-pass, at an equal 5-call cap per task.
  - Realised calls: about 4.0 Luna + 0.9 Sol per task, against about 1.3 Luna for the repair loop. That is about 6.4M vs 1.4M tokens per 140 tasks, so the claim is **not cost-matched**.
  - Sol is from the same vendor as Luna, so this is not a different-family claim.
- **Protocol consequences:**
  - (1) A/A re-run for the new incumbent: 2 arms (X:a, X:b), no D, because D parity was only required for v0.
  - Both are scheduled for 2026-10-17 (UTC), because 10-16 used 2,903 Luna calls (cap 3k).

### Eval v2.3 — the A/A for a promoted keep comes from its own replicates  [cycle 2, amendment, 2026-10-16; before any experiment against E002]
- **Problem.** `nano/aa.json` was calibrated on v0. A live A/A for the E002 incumbent would cost about 1.1k Luna + 0.25k Sol (2 arms × 140 tasks × about 4.9 calls), which is a third of a day's cap on every keep.
- **Method.**
  - A promoted candidate already has two independent full runs of itself on the dev set: the screen + rest stages, and the confirm stage (different seeds).
  - Pairing them by task (`X:cand`, run 2 − run 1) is an incumbent-vs-itself run on the same protocol.
  - Its family-bootstrap 95th percentile (centred, as before) is the new threshold.
  - Code: `python3 -I nano/eval.py aa-from-replicates --id E002b`. The old file is archived as `nano/history/aa-200daaa763c4.json`.
- **Result for E002** (incumbent sha `e772bd6e99f6…`):
  - thr_allpass = **0.0357 (5 net of 140)**, thr_frac = 0.0174;
  - 16 discordant tasks; mean Δall-pass −4.3 pp. The first replicate's screen half was selected for passing the screen (winner's curse). Centring removes the shift.
- **Calibration check of the method on v0** (both estimates available):
  - in-run A/A: 10 net (34 discordant);
  - cross-run pairing of E002b's `X:inc` arms: 7 net (25 discordant).
  - So one A/A estimate varies by about ±30%. The judge harness is genuinely less noisy, with 16 discordant tasks against 25–34.
- **Why this is acceptable:**
  - The full stage still requires Δ > max(thr, 1.645·SE), and that SE comes from the experiment's own paired data.
  - For E002-class comparisons, 1.645·SE ≈ 0.05 (about 7 net), so the SE guard, not the threshold, will usually bind.
  - The fresh confirmation replicate (Δ > thr/2) is unchanged.
- **Consequences:**
  - Screen: Δall ≥ +1.8 pp (≥ 2 net of about 70).
  - Full: Δall > max(0.036, 1.645·SE), with Δfrac not significantly negative.
  - Confirm: Δall > +1.8 pp.
  - The live A/A is no longer run per keep.

### C2-FA1 — where does the E002 incumbent still fail?  [cycle 2, offline analysis, 2026-10-16; no model calls]
- **Method:**
  - `docs/experiments/analysis/candidate_pool.py` re-scored every coder sample of E002's three runs (screen, rest, confirm: 280 task-runs, each dev task twice) on public and hidden tests. Pool: `runs/2026-10-16-c2-e002-pool/`.
  - Joined with the harness DIAG lines (branch, picks) and the ledger submissions.
- **The judge is at its ceiling:**
  - In all 249 judge-branch task-runs, it never picked a hidden-failing file when a hidden-passing green existed (**0 misses**).
  - E002's all-pass (0.857) **equals the any-pass ceiling of its own samples**. In the same runs, v0 (= D) scored 0.768.
- **The 40 failures (of 280):**

  | Failure | Count |
  |---|---|
  | Judge branch, every green fails hidden | 21 |
  | One-green branch, that green fails hidden | 13 |
  | No green after 4 samples (repair attempted) | 6 |

  - By kind: some hidden tests fail 30, timeouts 8, hidden compile failure 2.
- **Any-pass curve** (green and hidden-pass among k samples, exact over subsets): k=1 0.643, k=2 0.764, k=3 0.819, **k=4 0.850**. The 4th sample is worth about +3 pp.
- **Consequence:** the bottleneck has moved from selection to generation. A misreading shared by every sample defeats any selector, so the levers now are (a) decorrelating the samples' readings, or (b) adding information before generation.
  - Spending a sample (about −3 pp) is only worth it if the replacement raises per-sample pass rates by more than that.
  - Judge-quality variants (seed 3) have nothing left to gain at this regime, so they are deprioritised.
- Only aggregates from this analysis go to proposers (`runs/c2-propose-r1/stats.json`).

### E005 — diverse sampling angles  [cycle 2, proposer: claude (seed 8), registered 2026-10-16]
- **Candidate:** `nano/candidates/E005_diverse_angles.py`. One change vs E002: samples 2–4 each get one appended "Approach" paragraph, all different:
  - requirement-by-requirement reading (order of checks, precedence, boundaries, ties);
  - adversarial edge cases plus checked arithmetic;
  - literal precedence, with no extra behaviour.
  - Sample 1, the judge, the branches and the repair are unchanged.
- **Hypothesis:** different reading angles decorrelate the shared misreadings (C2-FA1), raising any-pass@4 and so all-pass vs E002.
  - Risk: the angles lower per-sample pass rates.
  - Diagnostics: the any-pass curve and per-angle hidden pass rates from a post-hoc pool.
- **Protocol:** eval v2.3, incumbent E002 (thr_allpass 0.0357). `python3 -I nano/eval.py run --id E005 --candidate nano/candidates/E005_diverse_angles.py --hypothesis "..." --jobs 6`.

### E006 — Sol reads the spec first (notes, no code), then 3 samples + judge  [cycle 2, proposer: claude (seeds 4/6 family), registered 2026-10-16]
- **Candidate:** `nano/candidates/E006_sol_spec_notes.py`. One change vs E002: the 4th Luna sample is replaced by a single Sol call **before** sampling.
  - Lane `analyst` = `codex/gpt-6.1-sol:medium`, no tools. It writes at most 12 numbered notes on where the requirements could be misread, citing requirement numbers, with no code.
  - The notes (≤ 5,000 bytes) are appended to the task for the 3 samples and the no-green repair.
  - Budget: 1 analyst + 3 samples + 1 judge or repair = 5.
- **Hypothesis:** a stronger reader flags the shared misreadings before any code is written. That raises the per-sample hidden pass rate (0.64) enough that any-pass@3 with notes beats E002's any-pass@4 (0.850).
- **Risks:**
  - wrong notes correlate all three samples;
  - one fewer sample costs about 3 pp.
- **Confound (disclosed):** Sol authored the tasks.
- **Coder-model rule:** Sol writes no code, and every submitted file is Luna's.
- **Protocol:** eval v2.3, incumbent E002. `--id E006`.

### Proposer round r1 (Codex Sol) — E007, E008 registered; r1-1 declined  [cycle 2, proposer: sol, 2026-10-16]
- **Setup:** `docs/experiments/analysis/c2_propose.py --round r1`, with `gpt-6.1-sol` at high effort and **no tools**. Everything was inline:
  - the incumbent and v0 files, the protocol and the guard;
  - the C2-FA1 aggregates (`runs/c2-propose-r1/stats.json`), which list E005 and E006 as already pending.
  - Prompt, reply and parsed candidates: `runs/c2-propose-r1/`.
- **r1-1, higher-effort Luna** (all Luna calls at `high` through a lane): **declined, out of scope.**
  - It changes the coder's configuration (the cycle's outcome is defined for `gpt-6-luna` at low effort), not the harness.
  - Its fair comparator would be D at high effort. It is kept as `nano/candidates/declined-r1-1-higher-effort-luna.py` for a possible effort-vs-harness question later.
- **E007 = r1-2, Sol-directed repair:** `nano/candidates/E007_sol_directed_repair.py`, Sol's code unmodified.
  - 3 Luna drafts, then one Sol call that selects among the distinct greens (or among all drafts if none is green) and lists concrete defects without writing code, then one Luna repair of the pick with those findings.
  - Submission: the pick and the repair, with the controller's public-rank and later-wins tie rule, so a green repair supersedes the pick.
  - The Sol reply must be pure JSON; otherwise it falls back (counted in DIAG).
  - Hypothesis (Sol): "Three independent Luna drafts followed by Sol selection and concrete critique, then one Luna repair, outperform four drafts followed by selection alone." Sol's expectation: +2 to +6 pp.
  - Bar from C2-FA1: it must beat the about −3 pp cost of the lost 4th sample.
- **E008 = r1-3, 4th-sample synthesis:** `nano/candidates/E008_sol_fourth_sample_synthesis.py`, Sol's code unmodified.
  - Sample 4 is a Luna call that sees drafts 1–3 with their public results and writes one solution, "using the drafts as material". The judge and branches are unchanged.
  - Hypothesis (Sol): "Replacing only the fourth independent Luna sample with a solution synthesized from the first three drafts produces useful candidates that independent sampling misses."
- **Smoke test** (1 dev task, 1 run each; prompts read):
  - E007 used 5 calls; its review parsed and flagged 2 concrete Θ(n²) scalability defects, and the repair was green.
  - E008 used 4 calls; its synthesis was green and distinct, on the one-green branch.
  - The E005 and E006 smokes also passed: 5 calls each, and the angles and notes appear in the prompts as designed.

### C2-ISO — harness isolation: the static guard was porous; harnesses now run under an OS sandbox  [cycle 2, instrument fix, 2026-10-16]
- **Found by** both design reviewers of the proposer-comparison draft (Codex Sol and a fresh Claude), independently.
  - `nano/eval.py`'s static guard blocked imports and named calls, but not attribute routes to the filesystem: `sys.modules['io'].open`, `random._os`, `json.codecs.open`.
- **Live-confirmed** with a new controller test (`external_harness_cannot_read_files_outside_its_working_directory`). Before the fix, a harness read a planted `hidden_tests.rs` and listed `$HOME`.
- **Past results are unaffected.**
  - All 9 distinct harness files that ever ran or were registered (21 paths: `runs/c2-*/candidate.py`, `incumbent.py`, the A/A snapshot, `nano/candidates`, `nano/history`) were scanned mechanically for module, attribute and file-access patterns: all clean.
  - Each was also read in full when written or reviewed.
- **Fix 1, the boundary** (controller, `crates/a2d-lift/src/arms.rs`): external harnesses run under `/usr/bin/sandbox-exec` with a seatbelt profile.
  - No network.
  - No file read or write under `$HOME`, `$TMPDIR`, `/private/tmp` or `/private/var/folders`.
  - Exceptions: the interpreter's own prefix, the harness script (read), its own empty working directory, and its stderr log (seatbelt also checks I/O on inherited descriptors).
  - The controller fails closed if `sandbox-exec` is missing.
  - The new test asserts the planted secret is unreadable and `$HOME` is unlistable, while the harness still runs and submits. All 66 `a2d-lift` tests pass, and the workspace has 24 result lines with 0 failures.
- **Fix 2, defence in depth** (`nano/eval.py` guard):
  - rejects private attributes, `sys.*` beyond stdin, stdout, stderr and exit, and a denylist of module and filesystem attributes;
  - rejects any use of a forbidden builtin name (not only calls) and dunder names.
  - 7 canaries are rejected; all real harnesses pass.
- **Also fixed:** the v0 parity test read `nano/harness.py`, which has been E002 since the promotion. It now reads the archived v0 (`nano/history/before-E002b.py`).
- **Live smoke under the sandbox** (1 dev task, about 7 Luna calls): E002 and E006 both ran normally (5 and 4 calls, DIAG intact).
  - It held no tasks, family names or results, and the proposer had no tools. A path cannot be used by harness code, which now runs sandboxed in any case.

### Phase-B tooling built  [cycle 2, proposer comparison, 2026-10-16; no model calls]
- `docs/experiments/analysis/c2_phaseb.py`, with subcommands `quarters`, `stats`, `dossiers`, `brief`, `propose`, `round` and `analyze`, and `test_c2_phaseb.py` (19 offline tests; no subprocess or provider is reachable from the tests).
  - Implemented by Codex `gpt-6.1-sol` (high) from a written spec, in a workspace-write sandbox that forbade `crates/`. Reviewed and amended by Claude.
- **Review amendments:**
  - (1) The proposer's empty cwd is now the system temp dir, not inside the repo's `runs/`.
  - (2) A dossier's "what the coder saw" now comes from arm D's first prompt in the v0 A/A run, which is exactly the controller's task prompt for every dev task (parity-tested). It no longer comes from the incumbent's first prompt, which can carry harness additions such as angles or notes.
- **Checks on real dev runs:**
  - The post-run authorship check gives **0 false violations over 704 real tuples** (E002 judge, v0 and E003 random-green arms; screen, rest and confirm).
  - `stats` reproduces C2-FA1 exactly.
  - Round-1 dossiers: 8, family-disjoint from that round's quarter.
- A brief this large passes as one command-line argument. macOS has no per-argument limit and `ARG_MAX` is 1 MB.

### Notes before the 10-17 queue  [cycle 2, 2026-10-16; zero Luna]
- **E002 vs arm B, simulated on dev** from E002's own 280 task-runs (B = best public rank of the 4 samples, ties to the later one, the controller's rule):

  | Arm | All-pass |
  |---|---|
  | E002 | 0.857 |
  | B4 | 0.732 |
  | v0 (= D, same runs) | 0.768 |

  - E002 − B4 = **+12.5 pp** (family-bootstrap 95% CI +7.5 to +18.2).
  - E002 − D = **+8.9 pp** (CI +4.3 to +14.3).
  - B with 5 samples may score slightly above B4. Its ties-to-later rule makes it close to a random-green pick, which is what E003 measured.
- **On eval v2.3's confirm rule** (Δ > thr/2 ≈ 2.5 net of 140): under E002-class noise (about 16–20 discordant tasks), a null candidate clears it about 1 time in 4.
  - Joint with the full-stage test (Δ > max(thr, 1.645·SE)), the false-keep rate per candidate is about 1%.
  - So the full stage does most of the work and confirm is a light replicate check. Decisions are also reported under the v0 threshold (0.0714).
- **Promotion sequencing rule:** `eval.py run` refuses an experiment whose incumbent differs from `aa.json`.
  - Pending experiments registered against the same incumbent all run through `--stop-after full` first.
  - Then the provisional keep with the largest full-stage Δ is confirmed. If it is discarded, the next one is confirmed.
  - **After a promotion,** the other provisional keeps are not confirmed against the old incumbent. They are rebuilt as compositions on the new incumbent and registered as new experiments, starting from the screen.
- **`luna_today()`** now also counts three-level ledgers (`runs/c2-pc/round<r>/run/`), so phase-B rounds are capped too.

### C2-E2 — does a Claude judge match Sol on E002's decisive tuples? / C2-FA2 — failure taxonomy  [cycle 2, offline, Claude quota, registered 2026-10-16 before any result]
- **Why now:** the owner made a fresh Claude quota available (weekly reset Sunday 12:00).
  - The live Claude lane is not available: the Claude CLI returns nothing inside the session sandbox, and running it outside the sandbox was refused by the permission classifier.
  - So Claude works offline here, through subagents. The cases are staged copies-only by `docs/experiments/analysis/c2_claude_stage.py`, outside the repo, with no hidden content. Subagents are instructed to read only that directory and to run nothing. Their tool use is not machine-audited, which is disclosed.
- **C2-E2: the Claude judge.**
  - Cases: the 68 decisive tuples in E002's three runs (≥ 2 distinct public-green files that split on hidden tests; 38 with 4 files, 21 with 3, 9 with 2). Sol, live, picked a passing file in **68/68**.
  - Claude (Opus 5.5 subagents, 8 batches) gets **E002's exact judge prompt**, with candidates in a fresh seeded shuffle. The answer key is `runs/2026-10-16-c2-claude/key.json`, outside the staged dir.
  - Expected hits under a random pick: 36.3 of 68.
  - Readout: hits out of 68 with a Wilson 95% CI.
  - **Use:** cross-family evidence on whether the selection lever is vendor-specific. If hits are ≥ 64/68, a Claude-lane judge (cross-family, and not the task author) is a ready candidate once a Claude lane exists. Descriptive only, with no keep decision.
- **C2-FA2: the failure taxonomy** (train-label rule).
  - Cases: the 34 task-runs where E002's submission failed and at least one public-green file existed (28 distinct tasks, 16 families). Each case holds the task as the coder saw it and every distinct public-green file from that run, all labelled "fails at least one hidden test".
  - Claude subagents (6 batches) record, per case: the violated requirements, the misreading, whether it is shared by all files, a category, why the visible tests miss it, a task-agnostic prevention (≤ 25 words), and a confidence.
  - **Use:** to design generation-side candidates. It is aggregated by category; no per-task content enters any harness.
- **C2-E2 result** (`runs/2026-10-16-c2-claude/judge-score.json`; per-batch picks beside it): **Claude Opus 5.5 picked a hidden-passing file in 66/68 decisive tuples (0.971, Wilson 95% CI 0.899–0.992).**

  | Judge | Hits / 68 |
  |---|---|
  | Sol, live | 68 |
  | Random pick (expected) | 36.3 |
  | Claude Opus 5.5 | 66 |

  - By number of candidate files: 2 files 8/9, 3 files 21/21, 4 files 37/38.
  - The two misses: `scc-graph-l3` (4 files) and `cron-next-l1` (2 files).
  - The pre-registered bar (≥ 64/68) is met.
  - **Reading:** the selection lever is not specific to Sol. A judge from a different vendor, which did not author the tasks, recovers nearly all of the selection headroom (C2-E1 on the S0 pool: Sonnet 37/39). This weakens the authorship confound for the judge mechanism: Sol's advantage on its own tasks is at most about 3 pp of pick rate on decisive tuples.
  - A Claude-lane judge stays a candidate (a cross-family claim, saving Sol load) for when a Claude lane exists. The CLI is not usable from the session sandbox.
- **C2-FA2 result** (`runs/2026-10-16-c2-claude/tax-batch*.json`, aggregate `taxonomy-aggregate.json`):
  - 34 diagnoses over 26 distinct tasks; confidence high 30, medium 3, low 1.
  - **The misreading is shared by every file in 29/34 cases**, which confirms C2-FA1's "shared misreading" reading.
  - Categories:

    | Category | Cases |
    |---|---|
    | Boundary / edge case | 9 |
    | Semantics / state model | 9 |
    | Performance / complexity | 8 (the 8 timeouts) |
    | Order of checks / precedence | 3 |
    | Exact format | 3 |
    | API contract | 1 |
    | Overflow | 1 |

  - **Recurring task-agnostic patterns:**

    | Pattern | Cases |
    |---|---|
    | Loops over a huge declared dimension when another is zero, or work not bounded by actual element count | 5 |
    | Brute-force searches that find nothing | 2 |
    | Preallocation from caller-supplied counts | 3 |
    | Literal spec examples not checked against the code | 3 |
    | Precedence chains not implemented exactly as listed | 3 |
    | Sentinels that are also legal values | 2 |
    | Streaming state not equal to one-shot | 2 |
    | Template literals vs placeholders | 2 |

  - **Use:** generation-side candidates aimed at these classes. Only the aggregate and the task-agnostic patterns go to proposers. Several preventions are close to cycle 1's researcher lessons (unchecked sizes, precedence), which were +5.6 pp vs D in S4b. Performance (worst-case cost and empty dimensions) is new, and no queued candidate targets it.

## Rules for proposals
- Outcome: raise the all-pass rate over the INCUMBENT at the same cap (k = 5 calls per task, any lane).
- The coder stays Luna: every submitted file must be written by a Luna call. Sol may read, judge, review,
  critique, analyse the spec or give feedback, but must not write solution code that gets submitted.
- One idea per candidate (a single-variable change against the incumbent), so the experiment attributes
  its effect. A coupled change is allowed only if the parts cannot work separately; say why.
- Each candidate is a complete replacement for the incumbent file that passes the guard.
- The keep rule is strict (screen on half the dev set, full set, then a fresh replicate; the full-stage
  all-pass gain must clear about +5 pp and 1.645 standard errors). Prefer ideas with a plausible large effect.

## Your task
Propose THREE candidates, ranked by expected all-pass gain over the incumbent. For each, give:
1. a heading line `### CANDIDATE <n>: <slug>` (slug: lowercase letters, digits and hyphens);
2. `Hypothesis:` one sentence;
3. `Mechanism:` why it should work, tied to the evidence above;
4. `Expected:` the all-pass change you expect and the main risk;
5. the complete Python file in one ```python block.
