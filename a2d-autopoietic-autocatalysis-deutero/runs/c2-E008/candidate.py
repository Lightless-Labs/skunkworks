#!/usr/bin/env python3
# lanes: judge=codex/gpt-6.1-sol:medium
"""E002 with only sample four changed to multi-draft synthesis."""
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
        prompt += (
            "\n\n## Previous attempt\n\n"
            "Your previous response contained no ```rust code block.\n"
        )
    else:
        prompt += (
            "\n\n## Previous attempt\n\n```rust\n"
            + code.rstrip()
            + "\n```\n\n## Public test results\n\n"
        )
        feedback = (
            public["output"]
            if public is not None
            else "(this task has no public tests)"
        )
        prompt += truncate(feedback, limit)
    prompt += (
        "\n\nFix the file. Respond with the whole corrected file "
        "in a single ```rust block.\n"
    )
    return prompt


def synthesis_prompt(task, entries):
    if not any(code is not None for code, _ in entries):
        return task["task_prompt"]

    prompt = task["task_prompt"]
    prompt += """
    
## Earlier candidate drafts

The drafts below are fallible attempts at this same task. Public results
only establish behavior on the visible tests.

Write one complete solution using the drafts as material for comparison.
Resolve differing choices directly from the original requirements.
Look for concrete omissions or defects that can be corrected by combining
sound parts of the drafts or replacing an unsound approach. Agreement
between drafts does not establish correctness. Check that the resulting
algorithm scales to the input domain allowed by the task.

Preserve the exact required public API. If a draft is already correct,
you may reuse it. Your final response must contain the whole solution in
one rust code block.
"""
    for i, (code, public) in enumerate(entries, 1):
        prompt += "\n### Draft " + str(i) + "\n"
        if code is None:
            prompt += "No Rust file was extracted.\n"
            continue
        prompt += "```rust\n" + code.rstrip() + "\n```\n"
        if public is not None:
            prompt += "\nPublic result:\n" + json.dumps({
                "compiled": public["compiled"],
                "passed": public["passed"],
                "expected": public["expected"],
                "all_passed": public["all_passed"],
            }) + "\n"
            if not public.get("all_passed"):
                prompt += truncate(
                    public["output"], task["max_feedback_chars"]
                ) + "\n"
    prompt += (
        "\nProduce the complete solution now, grounded in the original "
        "task. Respond in a single ```rust block.\n"
    )
    return prompt


def sample(task, prompt):
    reply = request("model", system=task["coder_system"], prompt=prompt)
    code = (
        request("extract", text=reply["text"])["code"]
        if reply.get("ok")
        else None
    )
    public = request("public_test", code=code) if code is not None else None
    return code, public


def rank(entry):
    _, public = entry
    return (public["passed"], public["compiled"]) if public else (0, False)


def judge(task, greens, diag):
    letters = "ABCDEFGH"[:len(greens)]
    order = list(range(len(greens)))
    random.Random(task["id"]).shuffle(order)
    shown = [greens[i] for i in order]
    diag["shown_order"] = order
    body = "TASK:\n" + task["task_prompt"] + "\n\nCANDIDATES:\n"
    for letter, code in zip(letters, shown):
        body += f"\n### {letter}.rs\n```rust\n{code.rstrip()}\n```\n"
    reply = request("model", lane="judge", system=JUDGE, prompt=body)
    if not reply.get("ok"):
        diag["fallback"] = (
            "judge call failed: " + str(reply.get("error", ""))[:120]
        )
        return greens[0]
    picks = re.findall(r'"pick"\s*:\s*"([^"]*)"', reply["text"])
    if (
        len(picks) != 1
        or picks[0].strip().upper() not in letters
        or len(picks[0].strip()) != 1
    ):
        diag["fallback"] = "unparseable pick: " + reply["text"][:120]
        return greens[0]
    letter = picks[0].strip().upper()
    diag["pick"] = letter
    return shown[letters.index(letter)]


def main():
    task = json.loads(sys.stdin.readline())["task"]
    base = task["task_prompt"]
    diag = {"task": task["id"]}
    entries = [sample(task, base) for _ in range(min(3, task["k"]))]

    if task["k"] >= 4:
        entries.append(sample(task, synthesis_prompt(task, entries)))
        diag["synthesis_green"] = bool(
            entries[-1][1] and entries[-1][1].get("all_passed")
        )
        diag["synthesis_distinct"] = (
            entries[-1][0] is not None
            and all(
                entries[-1][0] != code
                for code, _ in entries[:-1]
            )
        )

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
    codes = [code for code, _ in entries if code is not None]
    if entries and task["k"] >= 5:
        best = max(entries, key=rank)
        code, public = sample(
            task,
            repair_prompt(
                base, best[0], best[1], task["max_feedback_chars"]
            ),
        )
        if code is not None:
            codes.append(code)
    print("DIAG " + json.dumps(diag), file=sys.stderr, flush=True)
    request("done", codes=codes)


if __name__ == "__main__":
    main()
