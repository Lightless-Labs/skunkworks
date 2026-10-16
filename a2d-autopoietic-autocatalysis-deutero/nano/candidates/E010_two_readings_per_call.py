#!/usr/bin/env python3
# lanes: judge=codex/gpt-6.1-sol:medium
"""Nanoharness candidate: two readings per coder call, then E002's judge.

One change vs E002 (nano/harness.py): each of the four Luna sample calls asks
for TWO complete files in one reply. Solution A follows the readings the coder
thinks most likely; Solution B takes the other reading on the one to three
points it is least sure of (or, if it is sure of every point, a different
algorithm or data structure). Every parsed file is public-tested (free), and
the unchanged Sol judge picks among up to 8 distinct public-green files.

Why: E002's all-pass equals the any-pass ceiling of its own samples (C2-FA1),
the judge never misses when a passing green exists, and 29 of 34 failures are a
misreading shared by every file (C2-FA2). Independent samples keep re-drawing
the same reading; a hedge written in the same reply differs by construction,
and costs no extra call.

Budget per task (k = 5): 4 coder calls (up to two files each), then 1 judge call
(two or more distinct green files), 1 repair call (no green file; E002's
single-file repair, unchanged), or nothing (one green file). One DIAG line per
task: branch, files parsed per call, origin of each green and of the pick
(call number and slot A/B), shown order, fallback reason.
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

TWO_FILES = """

## Answer format for this request: two complete files

Hidden acceptance tests check every numbered requirement. A file that passes the visible tests but fails hidden ones has usually read some requirement the wrong way, so hedge:

1. First, in at most six short bullet points, list the places where the requirements could be read two ways or are easy to get wrong (order of checks, boundaries, empty or zero cases, exact output text, overflow, cost on the largest sizes allowed), and say which reading you take.
2. Then a line `### Solution A`, followed by the whole file in one ```rust block, following the readings you think most likely.
3. Then a line `### Solution B`, followed by the whole file in one ```rust block, taking the other reading on the one to three points you are least sure of. If you are sure of every point, make B an independent implementation with a different algorithm or data structure instead.

Both files must be complete, build on their own and pass the visible tests. Use no other code blocks.
"""

SYSTEM_NOTE = (
    "\n\nFor this request only: reply with two complete files in two ```rust blocks "
    "(Solution A, then Solution B), as the prompt describes."
)

# A fenced rust block whose fences sit on their own lines, so that a doc-comment
# fence inside the code (three backticks after "///") does not end the block.
FENCE = r"^[ \t]*```(?:rust|rs)[^\n]*\n(.*?)^[ \t]*```[ \t]*$"
LETTERS = "ABCDEFGH"


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
    """E002's single-file coder call (used only for the no-green repair)."""
    reply = request("model", system=task["coder_system"], prompt=prompt)
    code = request("extract", text=reply["text"])["code"] if reply.get("ok") else None
    public = request("public_test", code=code) if code is not None else None
    return code, public


def parse_files(text):
    """The fenced rust blocks of a reply: at most two (the longest), in reply order."""
    blocks = []
    for body in re.findall(FENCE, text, re.S | re.M):
        if body.strip() and body not in blocks:
            blocks.append(body)
    if len(blocks) > 2:
        longest = sorted(range(len(blocks)), key=lambda i: len(blocks[i]), reverse=True)[:2]
        blocks = [blocks[i] for i in sorted(longest)]
    return blocks


def sample_pair(task, prompt, call_no, diag):
    """One coder call that should return two files; each parsed file is public-tested."""
    reply = request("model", system=task["coder_system"] + SYSTEM_NOTE, prompt=prompt)
    if not reply.get("ok"):
        diag["files_per_call"].append(0)
        return []
    text = reply.get("text") or ""
    codes = parse_files(text)
    if not codes:
        code = request("extract", text=text)["code"]
        codes = [code] if code is not None else []
    diag["files_per_call"].append(len(codes))
    out = []
    for slot, code in enumerate(codes):
        public = request("public_test", code=code)
        out.append((code, public, str(call_no) + "AB"[slot]))
    return out


def rank(entry):
    public = entry[1]
    return (public["passed"], public["compiled"]) if public else (0, False)


def judge(task, greens, diag):
    letters = LETTERS[: len(greens)]
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
    diag = {"task": task["id"], "files_per_call": []}
    entries = []
    for call_no in range(1, min(4, task["k"]) + 1):
        entries.extend(sample_pair(task, base + TWO_FILES, call_no, diag))
    greens = []
    origins = []
    for code, public, origin in entries:
        if public and public.get("all_passed") and code not in greens and len(greens) < len(LETTERS):
            greens.append(code)
            origins.append(origin)
    diag["greens"] = len(greens)
    diag["green_origins"] = origins
    if len(greens) >= 2 and task["k"] >= 5:
        diag["branch"] = "judge"
        chosen = judge(task, greens, diag)
        diag["picked_origin"] = origins[greens.index(chosen)]
        print("DIAG " + json.dumps(diag), file=sys.stderr, flush=True)
        request("done", codes=[chosen])
        return
    if greens:
        diag["branch"] = "one-green"
        print("DIAG " + json.dumps(diag), file=sys.stderr, flush=True)
        request("done", codes=[greens[0]])
        return
    diag["branch"] = "no-green-repair"
    codes = [entry[0] for entry in entries]
    if task["k"] >= 5:
        if entries:
            best = max(entries, key=rank)
            code, public = sample(task, repair_prompt(base, best[0], best[1], task["max_feedback_chars"]))
        else:
            code, public = sample(task, repair_prompt(base, None, None, task["max_feedback_chars"]))
        if code is not None:
            codes.append(code)
    print("DIAG " + json.dumps(diag), file=sys.stderr, flush=True)
    request("done", codes=codes)


if __name__ == "__main__":
    main()
