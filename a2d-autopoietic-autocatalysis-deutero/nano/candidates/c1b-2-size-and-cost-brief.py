#!/usr/bin/env python3
# lanes: judge=codex/gpt-6.1-sol:medium
"""Candidate: size-and-cost brief (E002 plus a hidden-input brief for the coder).

Hypothesis: a short, task-agnostic brief on large and degenerate hidden
inputs, appended to every coder prompt, removes the performance and size
failures that every sample shares (C2-FA1: 8 of 40 failures are timeouts;
C2-FA2: huge declared dimensions, preallocation from caller counts,
exhaustive searches) and that no selector can fix.

One change vs E002: the coder's base prompt is the task prompt followed by
SIZE_BRIEF, for the four samples and for the no-green repair. The judge still
reads the original task prompt. Samples, judge, shuffle, parsing, dedupe,
branches and DIAG are as in E002 (DIAG adds "brief": true).
"""
import json
import random
import re
import sys

FENCE = "`" * 3

JUDGE = """You are reviewing candidate solutions to a programming task.

The task, exactly as the programmer received it (numbered requirements and the visible tests), is under TASK. The files under CANDIDATES are independent candidate solutions, and all of them pass the visible tests. Hidden acceptance tests check every numbered requirement.

Pick the single file most likely to pass all hidden tests. Read the requirements carefully and check each candidate against them, concentrating on where the candidates differ.

Reply with ONE JSON object and nothing else:
{"pick": "<letter>", "reason": "<one sentence>"}"""

SIZE_BRIEF = (
    "\n\n## Large and degenerate hidden inputs\n\n"
    "Besides checking every numbered requirement, the hidden tests run your code on inputs much larger "
    "and more degenerate than the visible tests, under a time limit. Before you answer, check your file "
    "against these points:\n"
    "- Bound every loop and every allocation by the data actually present, never by a size, count, "
    "capacity or dimension that the caller passes in. When a dimension is zero or the input is empty, "
    "return the required result at once, even if another declared dimension is huge.\n"
    "- Do not preallocate (with_capacity, reserve, vec![value; n]) from a caller-supplied number that "
    "has not been checked against the real input.\n"
    "- Work out the worst-case running time of each function on the largest input the requirements "
    "allow. If it grows faster than about n log n and a faster method exists, use it. If a search could "
    "try every combination before concluding that there is no answer, bound it or prune it.\n"
    "- Sizes, indices, counts and products of them must not overflow: use checked arithmetic or a wider "
    "integer type.\n"
    "These points never override a numbered requirement: where a requirement says what to return or how "
    "to fail, do exactly that.\n\n"
    "Respond with the whole file in a single " + FENCE + "rust block.\n"
)


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
        prompt += "\n\n## Previous attempt\n\nYour previous response contained no " + FENCE + "rust code block.\n"
    else:
        prompt += "\n\n## Previous attempt\n\n" + FENCE + "rust\n" + code.rstrip() + "\n" + FENCE
        prompt += "\n\n## Public test results\n\n"
        feedback = public["output"] if public is not None else "(this task has no public tests)"
        prompt += truncate(feedback, limit)
    prompt += "\n\nFix the file. Respond with the whole corrected file in a single " + FENCE + "rust block.\n"
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
        body += "\n### " + letter + ".rs\n" + FENCE + "rust\n" + code.rstrip() + "\n" + FENCE + "\n"
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
    base = task["task_prompt"] + SIZE_BRIEF
    diag = {"task": task["id"], "brief": True}
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
