#!/usr/bin/env python3
# lanes: judge=codex/gpt-6.1-sol:medium
"""Nanoharness candidate: E002 plus a failure-derived worst-case checklist on every coder prompt.

Idea: one fixed, task-agnostic checklist distilled from the C2-FA2
failure patterns (worst-case cost and degenerate sizes first, then literal
examples, precedence, sentinels, streaming, output templates), appended to
every Luna coder prompt, raises the per-sample hidden pass rate and so E002's
all-pass at the same budget.

Single change vs E002: the coder prompt is task_prompt + CHECKLIST (samples
and the no-green repair). The judge still sees the unmodified task_prompt.
Budget per task (k = 5) unchanged: 4 coder samples, then 1 judge call (two or
more distinct green files), 1 repair call (no green file), or nothing.
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

## Before you answer: hidden acceptance tests

Passing the visible tests is not enough. Hidden tests check every numbered requirement, with edge cases and inputs much larger than the visible ones. Check your file against each point below. Where a point and a numbered requirement disagree, the requirement wins; do not add behaviour the requirements do not ask for.

1. Worst-case cost. For each loop or search, estimate the work on the largest input the requirements allow. If it can exceed about 10^8 steps, or if a search may find nothing and run through its whole range, use a better algorithm or skip ahead in coarse units (whole blocks, rows, days) before fine ones.
2. Work bounded by actual data. Iterate over the elements that exist, not over a declared size, capacity or dimension. If any dimension or count is zero, return the empty result at once, even when another dimension is huge.
3. No preallocation from untrusted sizes. Never call with_capacity, vec![x; n] or similar with a count the caller supplies unless a requirement bounds it; grow as data arrives.
4. Literal examples. Run every example written in the requirements through your code by hand. The output must match exactly, character for character; in output templates, keep literal words literal and substitute only the placeholders.
5. Precedence. When several conditions or errors can apply, check them in exactly the order the requirements list them.
6. No sentinels. Do not use a value that is also legal data (0, -1, MAX, an empty string) to mean "none" or "unset"; use Option or a separate flag.
7. Incremental equals one-shot. If input can arrive in pieces (chunks, repeated calls, streaming), processing it in pieces must give exactly the same result as processing it all at once; keep partial state between calls.
8. Boundaries and arithmetic. Inclusive versus exclusive exactly as written; empty, single-element and maximum inputs; checked or wider arithmetic wherever values can overflow.
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
    base = task["task_prompt"].rstrip() + CHECKLIST
    diag = {"task": task["id"], "checklist": True}
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
