#!/usr/bin/env python3
# lanes: judge=codex/gpt-6.1-sol:medium
"""Nanoharness candidate: route the Sol call by how many greens three samples give.

One change vs E002 (nano/harness.py): what the Sol call is for is decided after
three Luna samples instead of four.
- Two or more distinct public-green files after three samples: E002 exactly
  (a fourth sample, then the unchanged judge over the distinct greens).
- At most one green after three samples: the Sol call (same lane, no code)
  writes numbered notes on what the requirements demand where the drafts go
  wrong or are at risk, given the task and all three drafts with their visible
  test results; the fifth call is one Luna repair of the best draft (the green
  one if there is one) with those notes. Submission: the drafts plus the
  repair, under the controller's public-rank rule, so a green repair supersedes.

Why: E002's one-green and no-green branches are 31 of 280 runs but 19 of its
40 failures; the judge cannot help with at most one green, and in the
one-green branch E002 leaves the fifth call unused. Routing keeps E002 where
selection works and spends Sol's reading where it does not. Overlap: the notes
call resembles E006's notes and E007's critique; the new variable is the
routing on an observed signal.

Budget per task (k = 5): 3 samples + (1 sample + 1 judge | 1 notes + 1 repair).
Sol writes no code: code blocks in its reply are removed before Luna sees it.
One DIAG line per task: branch, greens after 3, notes size, pick or fallback.
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

ANALYST = """You are a senior reviewer. A programmer is struggling with the task below: after three independent attempts, at most one passes the visible tests. Do not write code.

The task, exactly as the programmer received it (numbered requirements and the visible tests), is under TASK. The attempts so far, each with its visible test results, are under ATTEMPTS. Hidden acceptance tests check every numbered requirement, with larger and more degenerate data than the visible tests.

Write at most 10 numbered notes for the programmer's next attempt. Each note cites the requirement numbers it concerns and states exactly what they demand where the attempts go wrong or are at risk: first the cause of each visible test failure, then misread requirements, order of checks, boundaries and empty cases, exact output text, and cost on the largest sizes allowed. Be concrete (exact values, exact order, exact text). Plain prose only: no code and no code blocks."""

ATTEMPT_FEEDBACK = 2000  # bytes of visible-test output per draft shown to Sol
NOTES_LIMIT = 5000  # bytes of notes passed to the coder


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


def noted_repair_prompt(base, code, public, notes, limit):
    prompt = base
    if code is None:
        prompt += "\n\n## Previous attempt\n\nYour previous response contained no ```rust code block.\n"
    else:
        prompt += "\n\n## Previous attempt\n\n```rust\n" + code.rstrip() + "\n```\n\n## Public test results\n\n"
        feedback = public["output"] if public is not None else "(this task has no public tests)"
        prompt += truncate(feedback, limit)
    prompt += (
        "\n\n## Reviewer notes\n\n"
        "A senior reviewer read the requirements and every attempt so far. Hidden acceptance tests check "
        "every numbered requirement; where your previous attempt and a note disagree, follow the note's "
        "reading of the requirements.\n\n"
    )
    prompt += notes
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


def distinct_greens(entries):
    greens = []
    for code, public in entries:
        if public and public.get("all_passed") and code not in greens:
            greens.append(code)
    return greens


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


def attempts_text(entries):
    out = ""
    for number, (code, public) in enumerate(entries, 1):
        out += "\n### Attempt " + str(number) + "\n"
        if code is None:
            out += "(the reply contained no ```rust code block)\n"
            continue
        out += "```rust\n" + code.rstrip() + "\n```\n"
        if public is None:
            out += "Visible tests: not run.\n"
        elif public.get("all_passed"):
            out += "Visible tests: all pass.\n"
        else:
            out += "Visible tests: " + str(public.get("passed")) + " of " + str(public.get("expected"))
            out += " pass. Output:\n" + truncate(public.get("output") or "", ATTEMPT_FEEDBACK) + "\n"
    return out


def clean_notes(text):
    """Strip any code blocks (Sol must not write code that reaches the coder), then cap."""
    text = re.sub(r"```.*?```", "[code removed]", text, flags=re.S)
    text = re.sub(r"```.*", "[code removed]", text, flags=re.S)
    return truncate(text.strip(), NOTES_LIMIT)


def emit(diag):
    print("DIAG " + json.dumps(diag), file=sys.stderr, flush=True)


def main():
    task = json.loads(sys.stdin.readline())["task"]
    base = task["task_prompt"]
    limit = task["max_feedback_chars"]
    diag = {"task": task["id"]}
    entries = [sample(task, base) for _ in range(min(3, task["k"]))]
    greens = distinct_greens(entries)
    diag["greens_after_3"] = len(greens)
    if task["k"] < 5:
        while len(entries) < min(4, task["k"]):
            entries.append(sample(task, base))
        greens = distinct_greens(entries)
        diag["branch"] = "small-k"
        emit(diag)
        request("done", codes=greens[:1] if greens else [c for c, _ in entries if c is not None])
        return
    if len(greens) >= 2:
        entries.append(sample(task, base))
        greens = distinct_greens(entries)
        diag["greens"] = len(greens)
        diag["branch"] = "judge"
        chosen = judge(task, greens, diag)
        emit(diag)
        request("done", codes=[chosen])
        return
    diag["branch"] = "notes-repair"
    body = "TASK:\n" + base + "\n\nATTEMPTS:\n" + attempts_text(entries)
    reply = request("model", lane="judge", system=ANALYST, prompt=body)
    notes = clean_notes(reply.get("text") or "") if reply.get("ok") else ""
    diag["notes_bytes"] = len(notes.encode("utf-8"))
    if greens:
        target = next(e for e in entries if e[0] == greens[0])
    else:
        target = max(entries, key=rank)
    diag["target_green"] = bool(greens)
    if notes:
        prompt = noted_repair_prompt(base, target[0], target[1], notes, limit)
    else:
        diag["fallback"] = "no notes: " + str(reply.get("error", ""))[:120]
        prompt = base if greens else repair_prompt(base, target[0], target[1], limit)
    code, public = sample(task, prompt)
    diag["repair_green"] = bool(public and public.get("all_passed"))
    codes = [c for c, _ in entries if c is not None]
    if code is not None:
        codes.append(code)
    emit(diag)
    request("done", codes=codes)


if __name__ == "__main__":
    main()
