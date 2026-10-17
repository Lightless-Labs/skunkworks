#!/usr/bin/env python3
# lanes: judge=codex/gpt-6.1-sol:medium
"""Three samples, then Sol selection/audit and a Luna correction.

Single policy change against the incumbent: when the first three samples
contain a public-green file, replace sample-four-plus-judge with
review-plus-correction. Otherwise retain the incumbent's fourth sample
and, if needed, fifth-call repair.

Every submitted source file comes from a default-lane Luna call.
Sol provides only selection and prose feedback.
"""
import json
import random
import sys


REVIEW = """You are reviewing candidate solutions to a programming task.

TASK contains the complete authoritative specification and visible tests.
Every file under CANDIDATES passes the visible tests. Hidden acceptance
tests check all requirements.

First select the candidate most likely to satisfy the entire specification.
Then independently audit that selected file against EVERY numbered
requirement. Do not restrict attention to differences between candidates:
a defect shared by all candidates is especially useful to identify.

Trace actual execution for concrete boundary inputs and operation sequences.
Check error precedence, state transitions, empty inputs and shapes, numeric
limits, exact arithmetic, termination, and runtime bounds wherever required.
Derive numeric bounds from the specification rather than trusting constants.
Check that private helpers actually maintain the state their callers need.

Report at most six concrete defects in the SELECTED file, ordered by impact.
For each defect cite the requirement, describe a witness input or operation
sequence, and distinguish the required result from the file's actual behavior.
Do not report speculative concerns, invented requirements, or behavior the
specification explicitly permits. If no concrete defect is found, use [].

Do not write solution code, Rust snippets, patches, pseudocode, or replacement
expressions. Give behavioral corrections in ordinary prose only. A separate
programmer will verify your claims and implement any necessary changes.

Reply with exactly one JSON object:
{
  "pick": "<shown letter>",
  "issues": [
    {
      "requirement": "<requirement identifiers>",
      "witness": "<concrete input or operation sequence in prose>",
      "expected": "<required observable behavior>",
      "actual": "<what this candidate does and why>",
      "correction": "<minimal behavioral correction in prose>"
    }
  ]
}
"""


ISSUE_FIELDS = (
    "requirement", "witness", "expected", "actual", "correction"
)


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


def correction_prompt(base, code, issues):
    return (
        base
        + "\n\n## Baseline file\n\n```rust\n"
        + code.rstrip()
        + "\n```\n\nThis file passes the visible tests.\n"
        + "\n## Independent review\n\n"
        + json.dumps(issues, ensure_ascii=False, indent=2)
        + "\n\nThe review is advisory; the original task is authoritative. "
        "Verify each claimed defect by tracing the baseline on the stated "
        "witness and checking the cited requirements. Correct confirmed "
        "defects with the smallest necessary changes. Reject incorrect "
        "claims. Check interactions with the remaining requirements, "
        "preserving required error precedence, arithmetic order, state "
        "semantics, and empty-input behavior. Implement every necessary "
        "helper completely; do not leave placeholders. If no claim is "
        "valid, return the baseline unchanged.\n\n"
        "Respond with the whole corrected source file in a single "
        "```rust block.\n"
    )


def sample(task, prompt):
    reply = request(
        "model", system=task["coder_system"], prompt=prompt
    )
    code = (
        request("extract", text=reply["text"])["code"]
        if reply.get("ok")
        else None
    )
    public = (
        request("public_test", code=code)
        if code is not None
        else None
    )
    return code, public


def rank(entry):
    _, public = entry
    return (
        (public["passed"], public["compiled"])
        if public
        else (0, False)
    )


def green_codes(entries):
    greens = []
    for code, public in entries:
        if public and public.get("all_passed") and code not in greens:
            greens.append(code)
    return greens


def review(task, greens, diag):
    order = list(range(len(greens)))
    random.Random(task["id"]).shuffle(order)
    shown = [greens[i] for i in order]
    letters = "ABCDEFGH"[:len(shown)]
    diag["shown_order"] = order

    body = "TASK:\n" + task["task_prompt"] + "\n\nCANDIDATES:\n"
    for letter, code in zip(letters, shown):
        body += (
            "\n### " + letter + ".rs\n```rust\n"
            + code.rstrip() + "\n```\n"
        )

    reply = request(
        "model", lane="judge", system=REVIEW, prompt=body
    )
    if not reply.get("ok"):
        diag["fallback"] = (
            "review call failed: "
            + str(reply.get("error", ""))[:120]
        )
        return greens[0], []

    text = reply.get("text", "").strip()
    if text.startswith("```") and text.endswith("```"):
        text = "\n".join(text.splitlines()[1:-1]).strip()

    try:
        result = json.loads(text)
    except (ValueError, TypeError):
        diag["fallback"] = "unparseable review"
        return greens[0], []

    if not isinstance(result, dict):
        diag["fallback"] = "review is not an object"
        return greens[0], []

    pick = result.get("pick")
    if not isinstance(pick, str):
        diag["fallback"] = "missing review pick"
        return greens[0], []

    pick = pick.strip().upper()
    if len(pick) != 1 or pick not in letters:
        diag["fallback"] = "invalid review pick"
        return greens[0], []

    chosen = shown[letters.index(pick)]
    diag["pick"] = pick

    raw_issues = result.get("issues")
    if not isinstance(raw_issues, list):
        diag["fallback"] = "invalid issue list"
        return chosen, []

    issues = []
    for issue in raw_issues[:6]:
        if not isinstance(issue, dict):
            continue
        if not all(
            isinstance(issue.get(field), str)
            and issue[field].strip()
            for field in ISSUE_FIELDS
        ):
            continue
        issues.append({
            field: issue[field].strip()
            for field in ISSUE_FIELDS
        })

    diag["issues"] = len(issues)
    return chosen, issues


def finish(diag, codes):
    print(
        "DIAG " + json.dumps(diag),
        file=sys.stderr,
        flush=True,
    )
    request("done", codes=codes)


def main():
    task = json.loads(sys.stdin.readline())["task"]
    base = task["task_prompt"]
    budget = task["k"]
    diag = {"task": task["id"]}

    entries = [
        sample(task, base)
        for _ in range(min(3, budget))
    ]
    greens = green_codes(entries)

    if greens and budget >= 5:
        diag["branch"] = "review-then-edit"
        diag["greens"] = len(greens)
        chosen, issues = review(task, greens, diag)
        codes = [chosen]

        if issues:
            code, public = sample(
                task, correction_prompt(base, chosen, issues)
            )
            edit_green = bool(
                public and public.get("all_passed")
            )
            diag["edit_green"] = edit_green
            if code is not None and edit_green:
                # The controller prefers the later file on a public tie.
                codes.append(code)

        finish(diag, codes)
        return

    # Preserve the incumbent's fourth sample and no-green repair path.
    while len(entries) < min(4, budget):
        entries.append(sample(task, base))

    greens = green_codes(entries)
    diag["greens"] = len(greens)
    if greens:
        diag["branch"] = "green-after-sampling"
        finish(diag, [greens[0]])
        return

    diag["branch"] = "no-green-repair"
    codes = [code for code, _ in entries if code is not None]
    if budget >= 5 and entries:
        best = max(entries, key=rank)
        code, public = sample(
            task,
            repair_prompt(
                base,
                best[0],
                best[1],
                task["max_feedback_chars"],
            ),
        )
        if code is not None:
            codes.append(code)

    finish(diag, codes)


if __name__ == "__main__":
    main()
