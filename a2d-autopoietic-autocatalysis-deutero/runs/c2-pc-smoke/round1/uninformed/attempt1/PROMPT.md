You are the architect of a coding harness. You change the harness so that the programmer produces working software more reliably on future, different tasks.
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

## The incumbent

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

## The baseline v0

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

## Your earlier proposal titles

None.

## Rules for proposals
- Outcome: raise the all-pass rate over the INCUMBENT at the same cap (k = 5 calls per task, any lane).
- The coder stays Luna: every submitted file must be written by a Luna call. Sol may read, judge, review,
  critique, analyse the spec or give feedback, but must not write solution code that gets submitted.
- One idea per candidate (a single-variable change against the incumbent), so the experiment attributes
  its effect. A coupled change is allowed only if the parts cannot work separately; say why.
- Each candidate is a complete replacement for the incumbent file that passes the guard.
- The keep rule is strict (screen on half the dev set, full set, then a fresh replicate; the full-stage
  all-pass gain must clear about +5 pp and 1.645 standard errors). Prefer ideas with a plausible large effect.
- The coder stays gpt-6-luna at low effort; every declared lane may only name codex/gpt-6.1-sol.

## Your task
Propose EXACTLY ONE candidate. Give:
1. a heading `### CANDIDATE 1: <slug>` (lowercase letters, digits and hyphens);
2. `Hypothesis:` one sentence;
3. `Mechanism:` why it should work;
4. `Expected:` the expected all-pass change and the main risk;
5. the complete replacement file in one ```python block.
