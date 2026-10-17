### CANDIDATE 1: review-before-fourth-sample

**Hypothesis:** Turning the fourth Luna sample into a revision guided by Sol’s review will fix specification violations that the incumbent can only select around.

**Mechanism:** Generate three independent Luna solutions, then use Sol to select a public-green candidate and identify concrete requirement violations. Luna produces the final revision, with the selected original retained as a public-test fallback. This is one coupled scheduling change: Sol must review before the fourth Luna generation to enable revision within five calls. If the first three samples all fail, preserve the incumbent’s fourth-sample-and-repair path.

**Expected:** Target +5–8 percentage points in all-pass rate, unmeasured; the main risks are reduced candidate diversity and revisions that introduce hidden regressions despite passing public tests.

```python
#!/usr/bin/env python3
# lanes: judge=codex/gpt-6.1-sol:medium
"""Three samples, Sol selection and review, then Luna revision.

Single coupled change from the incumbent: move the fourth Luna generation
after Sol's review and condition it on the selected candidate and findings.
If the first three samples have no public-green candidate, retain the
incumbent's fourth independent sample followed by repair if needed.

Every submitted file comes from Luna. Sol supplies selection and prose
feedback only. Maximum model calls: five.
"""
import json
import random
import re
import sys


REVIEWER = """You are reviewing candidate solutions to a programming task.

TASK contains the exact requirements and visible tests. Every file under
CANDIDATES passes the visible tests. Hidden acceptance tests check every
numbered requirement.

Select the candidate most likely to pass all hidden tests. Then audit that
selected candidate against every numbered requirement, including behavior
not exercised by visible tests.

Report only concrete violations supported by the task and the selected
file. For each finding, identify the requirement, a distinguishing input
or execution scenario, the required behavior, and why this file violates it.
Check that the proposed scenario is permitted by the specification.
Do not invent requirements, request stylistic changes, or assume unspecified
behavior. An empty findings list is appropriate when no violation is found.

A Luna programmer will receive the selected file and your findings.
Do not write solution code, replacement snippets, patches, or pseudocode.
Keep all findings in concise plain English.

Reply with ONE JSON object and nothing else:
{
  "pick": "<letter>",
  "reason": "<one sentence>",
  "findings": [
    {
      "requirement": "<requirement number and relevant obligation>",
      "scenario": "<concrete input or execution scenario>",
      "expected": "<required result>",
      "violation": "<why the selected file produces a different result>"
    }
  ]
}"""


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


def revision_prompt(base, code, feedback):
    return (
        base
        + "\n\n## Candidate passing all visible tests\n\n```rust\n"
        + code.rstrip()
        + "\n```\n\n## Reviewer assessment\n\n"
        + feedback
        + "\n\n## Revision instructions\n\n"
        "Audit the candidate against every numbered requirement, including "
        "cases absent from the visible tests. Treat reviewer findings as "
        "advisory: verify each claim against the task and the actual code "
        "before changing anything. Correct concrete specification violations "
        "with focused changes, preserving other required behavior. "
        "Do not add behavior unsupported by the task. If the file already "
        "satisfies the requirements, reproduce it unchanged. "
        "Respond with the whole final file in a single ```rust block.\n"
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


def green_codes(entries):
    greens = []
    for code, public in entries:
        if public and public.get("all_passed") and code not in greens:
            greens.append(code)
    return greens


def review(task, greens, diag):
    letters = "ABCDEFGH"[:len(greens)]
    order = list(range(len(greens)))
    random.Random(task["id"]).shuffle(order)
    shown = [greens[i] for i in order]
    diag["shown_order"] = order

    body = "TASK:\n" + task["task_prompt"] + "\n\nCANDIDATES:\n"
    for letter, code in zip(letters, shown):
        body += f"\n### {letter}.rs\n```rust\n{code.rstrip()}\n```\n"

    reply = request("model", lane="judge", system=REVIEWER, prompt=body)
    unavailable = (
        "No usable reviewer assessment is available. "
        "Independently audit the selected candidate against the task."
    )
    if not reply.get("ok"):
        diag["fallback"] = (
            "review call failed: " + str(reply.get("error", ""))[:120]
        )
        return greens[0], unavailable

    picks = re.findall(r'"pick"\s*:\s*"([^"]*)"', reply["text"])
    if (
        len(picks) != 1
        or len(picks[0].strip()) != 1
        or picks[0].strip().upper() not in letters
    ):
        diag["fallback"] = "unparseable pick: " + reply["text"][:120]
        return greens[0], unavailable

    letter = picks[0].strip().upper()
    diag["pick"] = letter
    return shown[letters.index(letter)], reply["text"]


def finish(diag, codes):
    print("DIAG " + json.dumps(diag), file=sys.stderr, flush=True)
    request("done", codes=codes)


def main():
    task = json.loads(sys.stdin.readline())["task"]
    base = task["task_prompt"]
    budget = task["k"]
    diag = {"task": task["id"]}

    initial_count = 3 if budget >= 5 else min(4, budget)
    entries = [sample(task, base) for _ in range(initial_count)]
    greens = green_codes(entries)
    diag["initial_greens"] = len(greens)

    if greens and budget >= 5:
        diag["branch"] = "review-then-revise"
        chosen, feedback = review(task, greens, diag)
        revised, public = sample(
            task, revision_prompt(base, chosen, feedback)
        )
        codes = [chosen]
        if revised is not None:
            codes.append(revised)
        diag["revision_green"] = bool(public and public.get("all_passed"))
        diag["revision_changed"] = revised is not None and revised != chosen
        finish(diag, codes)
        return

    # No green after three calls: reproduce the incumbent's remaining path.
    if budget >= 5:
        entries.append(sample(task, base))
        greens = green_codes(entries)

    diag["greens"] = len(greens)
    if greens:
        diag["branch"] = "one-green"
        finish(diag, [greens[0]])
        return

    diag["branch"] = "no-green-repair"
    codes = [code for code, _ in entries if code is not None]
    if entries and budget >= 5:
        best = max(entries, key=rank)
        code, _ = sample(
            task,
            repair_prompt(
                base, best[0], best[1], task["max_feedback_chars"]
            ),
        )
        if code is not None:
            codes.append(code)
    finish(diag, codes)


if __name__ == "__main__":
    main()
```