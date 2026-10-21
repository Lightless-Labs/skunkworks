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
"""Three Luna drafts, one Sol selection/critique, one Luna repair."""
import json
import random
import sys

REVIEW = """You are selecting and reviewing Rust solutions.

TASK is exactly the programmer's task. Hidden acceptance tests check every
numbered requirement. Candidate files are fallible even when public-green.

Choose the candidate most likely to become fully correct with a small,
well-founded repair. If public-green candidates are supplied, select among
them. Audit the chosen file against the full task, including required API
compatibility and algorithmic scaling.

Identify only concrete defects supported by the task and the actual code.
For each defect, name the relevant requirement, explain how the code
violates it, and describe a triggering input or input class when possible.
Do not invent requirements. Do not request stylistic changes.
Do not write Rust code, replacement functions, patches, or pseudocode.

Return exactly one JSON object:
{"pick":"<letter>","fixes":["<concrete defect and required correction>", ...]}

Use an empty fixes list if you cannot establish a concrete defect.
Selection and feedback must refer to the same candidate."""


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


def repair_prompt(task, entry, fixes):
    code, public = entry
    prompt = task["task_prompt"] + "\n\n## Previous attempt\n\n"
    if code is None:
        prompt += "Your previous response contained no ```rust code block.\n"
    else:
        prompt += "```rust\n" + code.rstrip() + "\n```\n"
        prompt += "\n## Public test results\n\n"
        feedback = (
            public["output"]
            if public is not None
            else "(this task has no public tests)"
        )
        prompt += truncate(feedback, task["max_feedback_chars"])
    if fixes:
        prompt += "\n\n## Reviewer findings about this file\n\n"
        for i, finding in enumerate(fixes, 1):
            prompt += str(i) + ". " + finding + "\n"
        prompt += (
            "\nCheck these findings against the original requirements. "
            "Correct supported defects while preserving required behavior "
            "and the public API. Avoid unrelated rewrites.\n"
        )
    prompt += (
        "\nFix the file. Respond with the whole corrected file "
        "in a single ```rust block.\n"
    )
    return prompt


def sample(task, prompt):
    reply = request("model", system=task["coder_system"], prompt=prompt)
    code = (
        request("extract", text=reply["text"])["code"]
        if reply.get("ok")
        else None
    )
    public = request("public_test", code=code) if code is not None else None
    return code, public


def rank(entry):
    _, public = entry
    return (public["passed"], public["compiled"]) if public else (0, False)


def review(task, pool, all_green, diag):
    letters = "ABCDEFGH"[:len(pool)]
    order = list(range(len(pool)))
    random.Random(task["id"]).shuffle(order)
    shown = [pool[i] for i in order]
    diag["shown_order"] = order
    fallback = pool[0] if all_green else max(pool, key=rank)

    body = "TASK:\n" + task["task_prompt"] + "\n\nCANDIDATES:\n"
    body += (
        "All supplied files pass the public tests.\n"
        if all_green
        else "No supplied file passes all public tests.\n"
    )
    for letter, (code, public) in zip(letters, shown):
        body += f"\n### {letter}.rs\n```rust\n{code.rstrip()}\n```\n"
        if not all_green and public is not None:
            body += "\nPublic result:\n" + json.dumps({
                "compiled": public["compiled"],
                "passed": public["passed"],
                "expected": public["expected"],
            })
            body += "\n" + truncate(
                public["output"], task["max_feedback_chars"]
            ) + "\n"

    reply = request("model", lane="judge", system=REVIEW, prompt=body)
    if not reply.get("ok"):
        diag["fallback"] = (
            "review failed: " + str(reply.get("error", ""))[:120]
        )
        return fallback, []

    try:
        result = json.loads(reply["text"])
    except (ValueError, TypeError):
        diag["fallback"] = "review was not a JSON object"
        return fallback, []
    if not isinstance(result, dict):
        diag["fallback"] = "review was not a JSON object"
        return fallback, []

    pick = result.get("pick")
    fixes = result.get("fixes")
    if (
        not isinstance(pick, str)
        or len(pick.strip()) != 1
        or pick.strip().upper() not in letters
        or not isinstance(fixes, list)
        or any(not isinstance(item, str) for item in fixes)
    ):
        diag["fallback"] = "invalid review fields"
        return fallback, []

    pick = pick.strip().upper()
    fixes = [item.strip() for item in fixes if item.strip()]
    diag["pick"] = pick
    diag["findings"] = len(fixes)
    return shown[letters.index(pick)], fixes


def finish(codes, diag):
    print("DIAG " + json.dumps(diag), file=sys.stderr, flush=True)
    request("done", codes=codes)


def main():
    task = json.loads(sys.stdin.readline())["task"]
    entries = [
        sample(task, task["task_prompt"])
        for _ in range(min(3, task["k"]))
    ]
    codes = [code for code, _ in entries if code is not None]
    diag = {"task": task["id"]}

    if task["k"] < 5:
        diag["branch"] = "short-budget"
        finish(codes, diag)
        return

    greens = []
    pool = []
    for entry in entries:
        code, public = entry
        if code is not None and all(code != old[0] for old in pool):
            pool.append(entry)
        if (
            public
            and public.get("all_passed")
            and all(code != old[0] for old in greens)
        ):
            greens.append(entry)
    diag["greens"] = len(greens)

    if not pool:
        diag["branch"] = "no-code-repair"
        last = (None, None)
        for _ in range(task["k"] - len(entries)):
            last = sample(task, repair_prompt(task, last, []))
            if last[0] is not None:
                codes.append(last[0])
        finish(codes, diag)
        return

    chosen, fixes = review(task, greens or pool, bool(greens), diag)
    chosen_green = bool(chosen[1] and chosen[1].get("all_passed"))

    if chosen_green and not fixes:
        diag["branch"] = "review-keep"
        finish([chosen[0]], diag)
        return

    diag["branch"] = "review-repair"
    repaired = sample(task, repair_prompt(task, chosen, fixes))

    if chosen_green:
        submitted = [chosen[0]]
        if repaired[0] is not None:
            submitted.append(repaired[0])
    else:
        submitted = codes
        if repaired[0] is not None:
            submitted.append(repaired[0])

    diag["repair_green"] = bool(
        repaired[1] and repaired[1].get("all_passed")
    )
    finish(submitted, diag)


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

high-effort-review
Hypothesis: Raising Sol’s review effort from medium to high will uncover enough concrete hidden-test defects to improve all-pass performance.

two-round-review-repair
Hypothesis: Replacing two independent Luna drafts with a second Sol review/Luna repair round will increase hidden all-pass by correcting defects and regressions that survive the first repair.

contrastive-review
Hypothesis: Requiring Sol to investigate behavioral disagreements between drafts before selecting one will expose concrete requirement violations that a review of only the selected draft misses.

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
