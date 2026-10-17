#!/usr/bin/env python3
# lanes: judge=codex/gpt-6.1-sol:medium
"""Nanoharness candidate E005: E002 with diverse sampling angles (seed 8).

Change vs the incumbent (E002): samples 2-4 get one extra paragraph after the
task, each a different approach angle (requirement-by-requirement reading,
adversarial edge cases, literal precedence). Sample 1 and everything else
(judge, branches, repair) are unchanged.

Hypothesis: E002 sits at the any-pass ceiling of its 4 samples (the judge
never missed a hidden-passing green on the dev set); its failures are tasks
where every sample shares one misreading. Different reading angles
decorrelate those misreadings and raise any-pass@4, hence all-pass.

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


ANGLES = [
    "",
    "\n\n## Approach\n\nBefore writing code, go through the numbered requirements one at a time and settle exactly what each implies: the order of checks, which error wins when several apply, boundary values (zero, empty, maximum), overflow, ties and output order. Then write the file so every requirement is implemented exactly where its rule applies. Respond with only the code block.\n",
    "\n\n## Approach\n\nAssume the hidden tests probe every edge case the requirements mention: empty input, zero, maximum values and overflow, duplicates, ties, invalid input at every position, and the precedence between errors. Make the code correct on those cases first; use checked arithmetic wherever a value could overflow. Respond with only the code block.\n",
    "\n\n## Approach\n\nFollow the requirements literally and in order. Where they fix an order of checks, a precedence, a format or a tie-break, implement exactly that rule even if another seems more natural, and add no behaviour they do not ask for. Respond with only the code block.\n",
]


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
    entries = [sample(task, base + ANGLES[i]) for i in range(min(4, task["k"]))]
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
