#!/usr/bin/env python3
# lanes: judge=codex/gpt-6.1-sol:medium
"""Nanoharness candidate: E002 plus a failure-class checklist in every coder prompt.

One change vs E002 (nano/harness.py): the coder's task prompt (the four samples
and the no-green repair) gets one appended section listing the task-agnostic
failure classes found in E002's failing runs (C2-FA2): work not bounded by the
data present, worst-case cost, listed order of checks, boundaries and
overflow, sentinels that are legal data, incremental state equal to one-shot,
exact text, tracing literal examples, API names and derives. The judge sees the
original task prompt (without the checklist); every branch, the budget and the
DIAG line are E002's.

Disclosure: the classes were mined from E002 failures on the dev set and are
measured on the dev set, so the dev estimate is optimistic. The text is
task-agnostic (no task, family or test content).
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

CHECKLIST = """

## Before you answer: where files that pass the visible tests usually fail

Hidden acceptance tests check every numbered requirement, with larger and more degenerate data than the visible tests. Check your file against each point below before you reply.

1. Work bounded by the data present. Never loop or allocate over a declared size or a caller-supplied count (dimensions, capacity, n) when another dimension may be zero or the count may be huge: iterate over the elements that actually exist, return early on empty cases, and never pass an unchecked caller value to `with_capacity` or `vec![x; n]`.
2. Worst-case cost. Estimate the largest arguments the requirements allow and keep the algorithm near-linear on them. Every search or retry loop needs a bound that still holds when no answer exists.
3. Order of checks. Where the requirements list rules, validations or precedence, apply them in exactly the listed order, and resolve ties exactly as stated.
4. Boundaries. Inclusive versus exclusive ends, zero, one element, empty strings, minimum and maximum values; use checked or wider arithmetic wherever a value can overflow.
5. Sentinels. Never use a value as a marker (0, -1, MAX, an empty string) if it can also be legal data; use `Option` or an enum.
6. Incremental state. If data can arrive in pieces (chunks, repeated calls), the result must equal processing it all at once, for every split point.
7. Exact text. Reproduce literal words, punctuation, spacing and case exactly; in templates, tell fixed words apart from placeholders.
8. Examples. Trace every literal example in the requirements through your code and confirm the exact result.
9. API. Use exactly the names and signatures the requirements and tests use, and derive `Debug`, `Clone` and `PartialEq` on public types where all their fields allow it, so tests can compare and print them.
"""


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
    coder_base = base + CHECKLIST
    diag = {"task": task["id"], "checklist": True}
    entries = [sample(task, coder_base) for _ in range(min(4, task["k"]))]
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
    best = max(entries, key=rank) if entries else (None, None)
    codes = [c for c, _ in entries if c is not None]
    if task["k"] >= 5:
        code, public = sample(task, repair_prompt(coder_base, best[0], best[1], task["max_feedback_chars"]))
        if code is not None:
            codes.append(code)
    print("DIAG " + json.dumps(diag), file=sys.stderr, flush=True)
    request("done", codes=codes)


if __name__ == "__main__":
    main()
