#!/usr/bin/env python3
# lanes: judge=codex/gpt-6.1-sol:medium analyst=codex/gpt-6.1-sol:medium
"""Nanoharness candidate E006: Sol reads the spec first; 3 samples + judge.

Change vs the incumbent (E002): the 4th coder sample is replaced by one Sol
call BEFORE sampling (lane "analyst", no code): numbered notes on every place
the requirements could be misread. The notes are appended to the task for
all three Luna samples (and the no-green repair). Judge, branches and repair
logic are unchanged. Budget: 1 analyst + 3 samples + 1 judge or repair = 5.

Hypothesis: E002's remaining failures are tasks where every sample shares one
misreading (the judge never missed a hidden-passing green on the dev set).
A stronger reader flagging the traps up front raises each sample's hidden
pass rate by more than the ~4 pp any-pass@4 -> @3 loss of one sample.
Risk: wrong notes correlate all three samples (and Sol authored the tasks:
authorship confound, disclosed).

E002's original description follows.

Nanoharness candidate E002: sample 4, judge 1 (from C2-E1).

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


ANALYST = """You are a senior reviewer helping a programmer before they write code.

Below is a programming task exactly as the programmer will receive it: numbered requirements and the visible tests. Hidden acceptance tests check every numbered requirement.

List every point where a careful programmer could still misread or under-implement the requirements: the exact order of checks, which error or result wins when several apply, boundary values (zero, empty, maximum), overflow, ties and ordering, exact formats, and behaviour the visible tests do not exercise. Cite the requirement number in each note and state the correct reading precisely.

Do not write code. Reply with at most 12 short numbered notes and nothing else."""

NOTES_LIMIT = 5000


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
    reply = request("model", lane="analyst", system=ANALYST, prompt=task["task_prompt"])
    if reply.get("ok") and reply["text"].strip():
        base = base + "\n\n## Reviewer notes on the requirements\n\n" + truncate(reply["text"].strip(), NOTES_LIMIT) + "\n"
        diag["notes_chars"] = len(reply["text"])
    else:
        diag["notes"] = "analyst call failed: " + str(reply.get("error", ""))[:120]
    entries = [sample(task, base) for _ in range(min(3, task["k"] - 2))]
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
