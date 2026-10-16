#!/usr/bin/env python3
# lanes: judge=codex/gpt-6.1-sol:medium
"""Candidate: doubt pairs (E002 with two files per Luna call).

Hypothesis: asking each of the four Luna calls for two whole files, A (its
best reading of the requirements) and B (the other plausible reading of the
one or two points A had to guess at), puts the right reading of a shared
misreading into the pool at no extra call, and E002's Sol judge picks it.

One change vs E002: the four sampling prompts end with PAIR, and each reply
is split into up to two files (the first and the last rust code block that
contains a function; the extract op is the fallback when there is none).
Every file is public-tested. The judge prompt, the seeded shuffle, the strict
pick parsing, dedupe by text, the branch rules and the no-green repair (one
file, D's repair prompt) are unchanged; up to 8 distinct greens reach the
judge (letters A-H). Budget: 4 Luna calls, then 1 judge or 1 repair or
nothing, as in E002. DIAG adds file counts and the origin (sample number,
A or B) of each green and of the submitted file.
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

PAIR = (
    "\n\n## Two versions\n\n"
    "Write two complete versions of the file. This replaces any instruction to reply with a single "
    "code block.\n\n"
    "Version A is your best reading of the requirements.\n\n"
    "Version B is for your doubts. The visible tests pin down only part of the behaviour, and hidden "
    "tests check every numbered requirement. Before writing B, name in one to three short lines, outside "
    "any code block, the points where A had to guess what the requirements mean: an inclusive or exclusive "
    "boundary, empty or zero-sized input, the order in which checks or errors apply, ties, what is returned "
    "on an error, the exact output text, or how large the hidden inputs can be. For the one or two points "
    "that matter most, B follows the other plausible reading of the requirement (if A could be slow on the "
    "largest inputs the requirements allow, B is the version that stays fast). Everything else in B stays "
    "as in A, and both versions must pass the visible tests. If A had to guess nothing, write B as an "
    "independent second solution.\n\n"
    "Reply with exactly two " + FENCE + "rust blocks, each holding a whole file: first A, then B.\n"
)

BLOCK = "(?is)" + FENCE + r"(?:rust|rs)[^\n]*\n(.*?)\n[ \t]*" + FENCE


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


def split_files(text):
    blocks = [b.strip() + "\n" for b in re.findall(BLOCK, text) if "fn " in b]
    if len(blocks) > 2:
        blocks = [blocks[0], blocks[-1]]
    return blocks


def sample_pair(task, prompt):
    reply = request("model", system=task["coder_system"], prompt=prompt)
    if not reply.get("ok"):
        return [(None, None)]
    files = split_files(reply["text"])
    if not files:
        code = request("extract", text=reply["text"])["code"]
        files = [code] if code is not None else []
    if not files:
        return [(None, None)]
    return [(code, request("public_test", code=code)) for code in files]


def rank(entry):
    public = entry[1]
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
    base = task["task_prompt"]
    diag = {"task": task["id"]}
    entries = []
    for i in range(min(4, task["k"])):
        for j, (code, public) in enumerate(sample_pair(task, base + PAIR)):
            entries.append((code, public, str(i + 1) + "AB"[min(j, 1)]))
    greens = []
    origin = {}
    for code, public, tag in entries:
        if public and public.get("all_passed") and code not in greens:
            greens.append(code)
            origin[code] = tag
    diag["files"] = len([e for e in entries if e[0] is not None])
    diag["b_files"] = len([e for e in entries if e[0] is not None and e[2].endswith("B")])
    diag["greens"] = len(greens)
    diag["green_tags"] = [origin[c] for c in greens]
    greens = greens[:8]
    if len(greens) >= 2 and task["k"] >= 5:
        diag["branch"] = "judge"
        chosen = judge(task, greens, diag)
        diag["chosen"] = origin[chosen]
        print("DIAG " + json.dumps(diag), file=sys.stderr, flush=True)
        request("done", codes=[chosen])
        return
    if greens:
        diag["branch"] = "one-green"
        diag["chosen"] = origin[greens[0]]
        print("DIAG " + json.dumps(diag), file=sys.stderr, flush=True)
        request("done", codes=[greens[0]])
        return
    diag["branch"] = "no-green-repair"
    best = max(entries, key=rank)
    codes = [e[0] for e in entries if e[0] is not None]
    if task["k"] >= 5:
        code, public = sample(task, repair_prompt(base, best[0], best[1], task["max_feedback_chars"]))
        if code is not None:
            codes.append(code)
    print("DIAG " + json.dumps(diag), file=sys.stderr, flush=True)
    request("done", codes=codes)


if __name__ == "__main__":
    main()
