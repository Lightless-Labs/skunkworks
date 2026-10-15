#!/usr/bin/env python3
"""Nanoharness candidate E002: sample 4, judge 1 (from C2-E1).

Hypothesis: four independent coder samples, public-tested, plus ONE call to
a stronger reviewer that picks among the distinct public-green files from the
spec alone, pass hidden tests more often than the plain repair loop at the
same 5-call budget. Offline on the frozen train pool: 0.831 (Claude judge) and
0.846 (Sol judge) vs D 0.743 (LOG C2-E1).

Budget per task (k = 5): 4 coder samples, then either 1 judge call (two or
more distinct green files), 1 repair call (no green file), or nothing (one
green file). The judge is the controller lane named "judge".
"""
import json
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


def judge(task, greens):
    letters = "ABCDEFGH"
    body = "TASK:\n" + task["task_prompt"] + "\n\nCANDIDATES:\n"
    for letter, code in zip(letters, greens):
        body += f"\n### {letter}.rs\n```rust\n{code.rstrip()}\n```\n"
    reply = request("model", lane="judge", system=JUDGE, prompt=body)
    if reply.get("ok"):
        match = re.search(r"\{.*\}", reply["text"], re.S)
        try:
            pick = json.loads(match.group(0))["pick"].strip().upper()[:1]
            if pick in letters[: len(greens)]:
                return greens[letters.index(pick)]
        except Exception:
            pass
    return greens[0]


def main():
    task = json.loads(sys.stdin.readline())["task"]
    base = task["task_prompt"]
    entries = [sample(task, base) for _ in range(min(4, task["k"]))]
    greens = []
    for code, public in entries:
        if public and public.get("all_passed") and code not in greens:
            greens.append(code)
    if len(greens) >= 2 and task["k"] >= 5:
        request("done", codes=[judge(task, greens)])
        return
    if greens:
        request("done", codes=[greens[0]])
        return
    best = max(entries, key=rank)
    codes = [c for c, _ in entries if c is not None]
    if task["k"] >= 5:
        code, public = sample(task, repair_prompt(base, best[0], best[1], task["max_feedback_chars"]))
        if code is not None:
            codes.append(code)
    request("done", codes=codes)


if __name__ == "__main__":
    main()
