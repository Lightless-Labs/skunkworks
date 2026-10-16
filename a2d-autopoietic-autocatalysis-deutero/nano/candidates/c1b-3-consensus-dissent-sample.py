#!/usr/bin/env python3
# lanes: judge=codex/gpt-6.1-sol:medium
"""Candidate: consensus dissent (E002 with a dissenting 4th sample when 1-3 agree).

Hypothesis: when samples 1-3 already give two or more distinct public-green
files, a 4th sample that sees them and deliberately departs from their
untested agreement (on the one or two points the literal requirements most
plausibly read differently) supplies the reading a shared misreading hides,
and E002's Sol judge picks it.

One change vs E002: sample 4 uses the dissent prompt (task prompt, the
distinct early greens, and the dissent instructions) when samples 1-3 have at
least 2 distinct public-green files; otherwise sample 4 is a plain sample as
in E002. Samples 1-3, the judge prompt, the seeded shuffle, the strict pick
parsing, dedupe by text, the branch rules and the no-green repair are
unchanged. Budget: 4 Luna calls, then 1 judge or 1 repair or nothing. The
dissent reply is read with the extract op, or, if it holds two or more
rust code blocks, as its longest block. DIAG adds dissent, dissent_green,
dissent_new and picked_dissent.
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

DISSENT_HEAD = (
    "\n\n## Earlier solutions\n\n"
    "The files below were written independently for this task, and each passes the visible tests. "
    "Hidden tests check every numbered requirement. A behaviour that all of these files share but that "
    "no visible test checks may be a misreading they have in common.\n"
)

DISSENT_TAIL = (
    "\n## Your solution\n\n"
    "1. In a few short lines, list the behaviours on which these files agree but that no visible test "
    "pins down: boundaries (inclusive or exclusive, empty or zero-sized input), the order in which checks "
    "or errors apply, ties, what is returned on an error, the exact output text, and how the code copes "
    "with the largest inputs the requirements allow.\n"
    "2. For each one, reread the requirement it comes from word by word, and pick the one or two where "
    "the requirement most plausibly asks for something different from what the files do.\n"
    "3. Write your own complete solution: it follows the requirements on those points, is correct "
    "everywhere else, and still passes the visible tests. If no point stands out, write the most careful "
    "solution you can.\n\n"
    "Do not quote code from the earlier files. Respond with the short list, then your whole file in a "
    "single " + FENCE + "rust block.\n"
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


def dissent_prompt(base, early):
    prompt = base + DISSENT_HEAD
    for number, code in enumerate(early, 1):
        prompt += "\n### Solution " + str(number) + "\n\n" + FENCE + "rust\n" + code.rstrip() + "\n" + FENCE + "\n"
    return prompt + DISSENT_TAIL


def sample(task, prompt):
    reply = request("model", system=task["coder_system"], prompt=prompt)
    code = request("extract", text=reply["text"])["code"] if reply.get("ok") else None
    public = request("public_test", code=code) if code is not None else None
    return code, public


def sample_dissent(task, prompt):
    reply = request("model", system=task["coder_system"], prompt=prompt)
    if not reply.get("ok"):
        return None, None
    blocks = [b.strip() + "\n" for b in re.findall(BLOCK, reply["text"]) if "fn " in b]
    if len(blocks) >= 2:
        code = max(blocks, key=len)
    else:
        code = request("extract", text=reply["text"])["code"]
    public = request("public_test", code=code) if code is not None else None
    return code, public


def distinct_greens(entries):
    greens = []
    for code, public in entries:
        if public and public.get("all_passed") and code not in greens:
            greens.append(code)
    return greens


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
    base = task["task_prompt"]
    diag = {"task": task["id"], "dissent": False}
    n = min(4, task["k"])
    entries = [sample(task, base) for _ in range(min(3, n))]
    early = distinct_greens(entries)
    if n >= 4:
        if len(early) >= 2 and task["k"] >= 5:
            diag["dissent"] = True
            code, public = sample_dissent(task, dissent_prompt(base, early))
            entries.append((code, public))
            diag["dissent_green"] = bool(public and public.get("all_passed"))
            diag["dissent_new"] = code is not None and code not in early
        else:
            entries.append(sample(task, base))
    dissent_code = entries[-1][0] if diag["dissent"] else None
    greens = distinct_greens(entries)
    diag["greens"] = len(greens)
    if len(greens) >= 2 and task["k"] >= 5:
        diag["branch"] = "judge"
        chosen = judge(task, greens, diag)
        diag["picked_dissent"] = dissent_code is not None and chosen == dissent_code and chosen not in early
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
