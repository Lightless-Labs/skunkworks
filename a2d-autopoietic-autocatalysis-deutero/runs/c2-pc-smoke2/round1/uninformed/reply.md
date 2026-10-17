### CANDIDATE 1: review-before-final-sample

Hypothesis: Spending the fourth Luna call on a concrete Sol-directed repair will improve hidden all-pass rates more than spending it on another independent sample.

Mechanism: Generate three candidates, then let Sol select and audit the best public-green candidate against every requirement. Reserve the final call for Luna to repair specific violations identified by Sol. Moving review earlier and reserving a repair call are inseparable parts of this budget reallocation; a terminal review cannot correct defects. Sol supplies prose feedback only.

Expected: An unmeasured +5–8 percentage points in all-pass rate. The main risks are incorrect review feedback causing regressions and losing the fourth independent draw.

```python
#!/usr/bin/env python3
# lanes: judge=codex/gpt-6.1-sol:medium
"""Three Luna samples, Sol selection/audit, optional Luna repair.

Single experimental change: reserve the fourth Luna generation for a
review-directed revision instead of an independent sample. Every submitted
file comes from Luna. Sol produces selection and prose feedback only.
"""
import json
import random
import re
import sys


REVIEW = """You are selecting and auditing solutions to a programming task.

TASK contains the exact numbered requirements and visible tests.
CANDIDATES contains independently generated Rust files and their public
results. If any files passed all visible tests, only those files are shown.
Hidden acceptance tests check every numbered requirement.

Select the file most likely to pass all hidden tests. Audit that selected
file against every requirement, including boundary cases, ordering,
state transitions, error behavior, and exact output semantics where relevant.

Report only definite violations supported by the task and the selected
file. For each issue, identify the requirement, a concrete triggering input
or execution sequence, expected behavior, actual behavior, and a focused
repair direction. Do not invent requirements, infer unspecified behavior,
or request cosmetic changes. An empty issues list is appropriate when you
cannot establish a violation.

You must not write solution code, replacement functions, patches, or Rust
snippets. Feedback must be prose only. Another programmer will verify and
implement it.

Reply with exactly one JSON object:
{"pick": "<letter>", "reason": "<one sentence>",
 "issues": ["<concrete violation and prose repair direction>", ...]}
"""


def request(op, **fields):
    sys.stdout.write(json.dumps({"op": op, **fields}) + "\n")
    sys.stdout.flush()
    if op == "done":
        return None
    return json.loads(sys.stdin.readline())


def truncate(text, limit):
    data = text.encode("utf-8")
    limit = max(0, limit)
    if len(data) <= limit:
        return text
    end = limit
    while end > 0 and (data[end] & 0xC0) == 0x80:
        end -= 1
    return data[:end].decode("utf-8") + "\n... [truncated]"


def repair_prompt(base, code, public, limit, issues=None):
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

    if issues:
        prompt += "\n\n## Independent requirement audit\n\n"
        prompt += "\n".join(
            str(i + 1) + ". " + issue
            for i, issue in enumerate(issues)
        )
        prompt += (
            "\n\nVerify these claims against the original requirements and "
            "the file. Repair confirmed violations, preserving behavior "
            "that already satisfies the task. Disregard unsupported claims."
        )

    prompt += (
        "\n\nFix the file. Respond with the whole corrected file "
        "in a single ```rust block.\n"
    )
    return prompt


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
    return (public["passed"], public["compiled"]) if public else (0, False)


def is_green(entry):
    return bool(entry[1] and entry[1].get("all_passed"))


def distinct(entries):
    result = []
    seen = set()
    for entry in entries:
        code = entry[0]
        if code is not None and code not in seen:
            seen.add(code)
            result.append(entry)
    return result


def review(task, pool, diag):
    order = list(range(len(pool)))
    random.Random(task["id"]).shuffle(order)
    shown = [pool[i] for i in order]
    letters = "ABCDEFGHIJKLMNOPQRSTUVWXYZ"[:len(shown)]
    diag["shown_order"] = order

    body = "TASK:\n" + task["task_prompt"] + "\n\nCANDIDATES:\n"
    for letter, (code, public) in zip(letters, shown):
        body += "\n### " + letter + ".rs\n"
        body += "Public result: " + json.dumps({
            "compiled": public["compiled"],
            "passed": public["passed"],
            "expected": public["expected"],
            "all_passed": public["all_passed"],
        }) + "\n"
        if not public.get("all_passed"):
            body += truncate(
                public["output"], task["max_feedback_chars"]
            ) + "\n"
        body += "```rust\n" + code.rstrip() + "\n```\n"

    fallback = pool[0] if is_green(pool[0]) else max(pool, key=rank)
    reply = request("model", lane="judge", system=REVIEW, prompt=body)
    if not reply.get("ok"):
        diag["fallback"] = "review call failed"
        return fallback, []

    text = reply["text"].strip()
    text = re.sub(r"^```(?:json)?\s*", "", text)
    text = re.sub(r"\s*```$", "", text)
    try:
        parsed = json.loads(text)
    except (ValueError, TypeError):
        diag["fallback"] = "invalid review JSON"
        return fallback, []

    if not isinstance(parsed, dict):
        diag["fallback"] = "review is not an object"
        return fallback, []

    pick = parsed.get("pick")
    if not isinstance(pick, str):
        diag["fallback"] = "missing pick"
        return fallback, []
    pick = pick.strip().upper()
    if len(pick) != 1 or pick not in letters:
        diag["fallback"] = "invalid pick"
        return fallback, []

    issues = parsed.get("issues", [])
    if not isinstance(issues, list) or any(
        not isinstance(issue, str) for issue in issues
    ):
        diag["fallback"] = "invalid issues"
        return fallback, []

    issues = [issue.strip() for issue in issues if issue.strip()]
    diag["pick"] = pick
    diag["issues"] = len(issues)
    return shown[letters.index(pick)], issues


def finish(task, diag, codes):
    diag["task"] = task["id"]
    print("DIAG " + json.dumps(diag), file=sys.stderr, flush=True)
    request("done", codes=codes)


def main():
    task = json.loads(sys.stdin.readline())["task"]
    base = task["task_prompt"]
    budget = max(0, task["k"])
    entries = [sample(task, base) for _ in range(min(3, budget))]
    used = len(entries)
    diag = {}

    if not entries:
        finish(task, diag, [])
        return

    greens = distinct([entry for entry in entries if is_green(entry)])
    diag["greens"] = len(greens)
    pool = greens if greens else distinct(entries)

    if pool and budget - used >= 2:
        diag["branch"] = "review-before-final-sample"
        selected, issues = review(task, pool, diag)
        used += 1

        # Keep the reviewer's selection authoritative among green originals.
        # With no green original, preserve every original as a fallback.
        codes = (
            [selected[0]]
            if greens
            else [code for code, _ in entries if code is not None]
        )

        if issues or not is_green(selected):
            prompt = repair_prompt(
                base, selected[0], selected[1],
                task["max_feedback_chars"], issues
            )
            revised = sample(task, prompt)
            used += 1
            if revised[0] is not None:
                codes.append(revised[0])
            diag["revision_green"] = is_green(revised)

        diag["calls"] = used
        finish(task, diag, codes)
        return

    # Handles extraction failures and smaller-than-normal call budgets.
    diag["branch"] = "public-repair-fallback"
    codes = [code for code, _ in entries if code is not None]
    while used < budget and not any(is_green(e) for e in entries):
        best = max(entries, key=rank)
        entry = sample(task, repair_prompt(
            base, best[0], best[1], task["max_feedback_chars"]
        ))
        used += 1
        entries.append(entry)
        if entry[0] is not None:
            codes.append(entry[0])

    diag["calls"] = used
    finish(task, diag, codes)


if __name__ == "__main__":
    main()
```