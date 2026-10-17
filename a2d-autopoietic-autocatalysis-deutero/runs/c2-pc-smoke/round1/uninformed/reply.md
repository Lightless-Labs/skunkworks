### CANDIDATE 1: shared-spec-audit

Hypothesis: Replacing one independent Luna sample with a Sol requirements audit will prevent enough shared specification mistakes to outweigh losing one candidate.

Mechanism: Spend the first sampling slot on a Sol audit of easily missed requirements, then give that audit to three independent Luna calls. Keep the incumbent’s public testing, final judging, and no-green repair behavior. The audit and its inclusion in Luna’s prompts are inseparable: together they replace one sampling slot with informed generation. Every submitted file still comes from Luna.

Expected: An unmeasured +6–8 percentage-point all-pass gain; the main risk is that incorrect shared advice correlates failures while reducing candidate diversity.

```python
#!/usr/bin/env python3
# lanes: judge=codex/gpt-6.1-sol:medium
"""Three independent Luna samples informed by one shared Sol spec audit.

Single experimental change: replace the first of the incumbent's four
sampling slots with a requirements audit, shared by the remaining samples.

At k=5:
  1 Sol audit + 3 Luna samples + at most 1 final call.
The final call retains the incumbent policy:
  multiple distinct public-green files -> Sol judge;
  no public-green file -> Luna repair;
  one distinct public-green file -> stop.

Only Luna responses are extracted into submitted code.
"""
import json
import random
import re
import sys


AUDIT = """You are auditing a programming specification before implementation.

TASK contains exactly the requirements and visible tests the programmer receives.
Hidden tests check the numbered requirements.

Produce at most 16 concise checklist bullets identifying the precise obligations
most likely to be overlooked by a fast programmer. Prioritize interactions among
requirements, boundary behavior, ordering, validation precedence, state changes,
and exact output semantics when the task specifies them.

Ground every bullet in the task. Preserve exact identifiers and required values
where useful. Do not invent behavior, add requirements, or treat an ambiguity as
settled. If an ambiguity matters, identify it explicitly. Visible examples do not
replace the numbered requirements.

Give analysis only: no solution code, Rust blocks, or complete implementation.
The checklist will be shared with independent programmers."""


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


def audit_prompt(task, diag):
    reply = request(
        "model",
        lane="judge",
        system=AUDIT,
        prompt="TASK:\n" + task["task_prompt"],
    )
    notes = reply.get("text", "").strip() if reply.get("ok") else ""
    diag["audit_ok"] = bool(notes)
    if not notes:
        return task["task_prompt"]
    return (
        task["task_prompt"]
        + "\n\n## Independent requirements audit\n\n"
        + notes
        + "\n\nThe audit is advisory; the original task is authoritative. "
        "Check the applicable points while implementing the requested file. "
        "Respond with the whole file in a single ```rust block.\n"
    )


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

    slots = min(4, task["k"])
    prompt = base
    if slots >= 2:
        prompt = audit_prompt(task, diag)
        slots -= 1
    entries = [sample(task, prompt) for _ in range(slots)]
    diag["samples"] = len(entries)

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
```