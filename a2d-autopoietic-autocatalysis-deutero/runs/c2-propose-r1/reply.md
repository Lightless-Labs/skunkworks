Expected gains below are speculative absolute percentage points over E002. All three preserve Luna authorship and the five-call cap.

### CANDIDATE 1: higher-effort-luna

Hypothesis: Raising Luna’s reasoning effort from low to high, with the sampling and judge policy unchanged, improves candidate coverage enough to raise all-pass by roughly 4–8 percentage points.

Mechanism: E002’s judge already reaches its observed candidate ceiling; the remaining opportunity is better files. Higher reasoning effort can address shared semantic mistakes, algorithmic complexity, and compatibility before each file is generated. This changes only the Luna effort setting, including the fallback repair.

Expected: **+4–8 pp**; the main risk is that additional reasoning reproduces the same misunderstandings. Token use and latency will increase, so this remains a call-cap comparison.

```python
#!/usr/bin/env python3
# lanes: writer=codex/gpt-6-luna:high judge=codex/gpt-6.1-sol:medium
"""E002 with one change: all Luna calls use high reasoning effort."""
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


def sample(task, prompt):
    reply = request(
        "model",
        lane="writer",
        system=task["coder_system"],
        prompt=prompt,
    )
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
    diag = {"task": task["id"], "writer_effort": "high"}
    entries = [sample(task, base) for _ in range(min(4, task["k"]))]
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

### CANDIDATE 2: sol-directed-repair

Hypothesis: Three independent Luna drafts followed by Sol selection and concrete critique, then one Luna repair, outperform four drafts followed by selection alone.

Mechanism: Thirty-four of the forty incumbent failures occur when the available green files all fail hidden tests. Sol can identify a missing requirement even when it cannot select a correct file. Selection and critique share one call; freeing the repair call necessarily replaces the fourth independent sample. The measured three-to-four-sample ceiling gain is 3.1 pp, which is the approximate coverage cost this intervention must overcome.

Expected: **+2–6 pp**; the main risks are losing a uniquely correct fourth sample and introducing hidden regressions during repair. A repaired file supersedes a green original only when it also passes public tests.

```python
#!/usr/bin/env python3
# lanes: judge=codex/gpt-6.1-sol:medium
"""Three Luna drafts, one Sol selection/critique, one Luna repair."""
import json
import random
import sys

REVIEW = """You are selecting and reviewing Rust solutions.

TASK is exactly the programmer's task. Hidden acceptance tests check every
numbered requirement. Candidate files are fallible even when public-green.

Choose the candidate most likely to become fully correct with a small,
well-founded repair. If public-green candidates are supplied, select among
them. Audit the chosen file against the full task, including required API
compatibility and algorithmic scaling.

Identify only concrete defects supported by the task and the actual code.
For each defect, name the relevant requirement, explain how the code
violates it, and describe a triggering input or input class when possible.
Do not invent requirements. Do not request stylistic changes.
Do not write Rust code, replacement functions, patches, or pseudocode.

Return exactly one JSON object:
{"pick":"<letter>","fixes":["<concrete defect and required correction>", ...]}

Use an empty fixes list if you cannot establish a concrete defect.
Selection and feedback must refer to the same candidate."""


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


def repair_prompt(task, entry, fixes):
    code, public = entry
    prompt = task["task_prompt"] + "\n\n## Previous attempt\n\n"
    if code is None:
        prompt += "Your previous response contained no ```rust code block.\n"
    else:
        prompt += "```rust\n" + code.rstrip() + "\n```\n"
        prompt += "\n## Public test results\n\n"
        feedback = (
            public["output"]
            if public is not None
            else "(this task has no public tests)"
        )
        prompt += truncate(feedback, task["max_feedback_chars"])
    if fixes:
        prompt += "\n\n## Reviewer findings about this file\n\n"
        for i, finding in enumerate(fixes, 1):
            prompt += str(i) + ". " + finding + "\n"
        prompt += (
            "\nCheck these findings against the original requirements. "
            "Correct supported defects while preserving required behavior "
            "and the public API. Avoid unrelated rewrites.\n"
        )
    prompt += (
        "\nFix the file. Respond with the whole corrected file "
        "in a single ```rust block.\n"
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


def review(task, pool, all_green, diag):
    letters = "ABCDEFGH"[:len(pool)]
    order = list(range(len(pool)))
    random.Random(task["id"]).shuffle(order)
    shown = [pool[i] for i in order]
    diag["shown_order"] = order
    fallback = pool[0] if all_green else max(pool, key=rank)

    body = "TASK:\n" + task["task_prompt"] + "\n\nCANDIDATES:\n"
    body += (
        "All supplied files pass the public tests.\n"
        if all_green
        else "No supplied file passes all public tests.\n"
    )
    for letter, (code, public) in zip(letters, shown):
        body += f"\n### {letter}.rs\n```rust\n{code.rstrip()}\n```\n"
        if not all_green and public is not None:
            body += "\nPublic result:\n" + json.dumps({
                "compiled": public["compiled"],
                "passed": public["passed"],
                "expected": public["expected"],
            })
            body += "\n" + truncate(
                public["output"], task["max_feedback_chars"]
            ) + "\n"

    reply = request("model", lane="judge", system=REVIEW, prompt=body)
    if not reply.get("ok"):
        diag["fallback"] = (
            "review failed: " + str(reply.get("error", ""))[:120]
        )
        return fallback, []

    try:
        result = json.loads(reply["text"])
    except (ValueError, TypeError):
        diag["fallback"] = "review was not a JSON object"
        return fallback, []
    if not isinstance(result, dict):
        diag["fallback"] = "review was not a JSON object"
        return fallback, []

    pick = result.get("pick")
    fixes = result.get("fixes")
    if (
        not isinstance(pick, str)
        or len(pick.strip()) != 1
        or pick.strip().upper() not in letters
        or not isinstance(fixes, list)
        or any(not isinstance(item, str) for item in fixes)
    ):
        diag["fallback"] = "invalid review fields"
        return fallback, []

    pick = pick.strip().upper()
    fixes = [item.strip() for item in fixes if item.strip()]
    diag["pick"] = pick
    diag["findings"] = len(fixes)
    return shown[letters.index(pick)], fixes


def finish(codes, diag):
    print("DIAG " + json.dumps(diag), file=sys.stderr, flush=True)
    request("done", codes=codes)


def main():
    task = json.loads(sys.stdin.readline())["task"]
    entries = [
        sample(task, task["task_prompt"])
        for _ in range(min(3, task["k"]))
    ]
    codes = [code for code, _ in entries if code is not None]
    diag = {"task": task["id"]}

    if task["k"] < 5:
        diag["branch"] = "short-budget"
        finish(codes, diag)
        return

    greens = []
    pool = []
    for entry in entries:
        code, public = entry
        if code is not None and all(code != old[0] for old in pool):
            pool.append(entry)
        if (
            public
            and public.get("all_passed")
            and all(code != old[0] for old in greens)
        ):
            greens.append(entry)
    diag["greens"] = len(greens)

    if not pool:
        diag["branch"] = "no-code-repair"
        last = (None, None)
        for _ in range(task["k"] - len(entries)):
            last = sample(task, repair_prompt(task, last, []))
            if last[0] is not None:
                codes.append(last[0])
        finish(codes, diag)
        return

    chosen, fixes = review(task, greens or pool, bool(greens), diag)
    chosen_green = bool(chosen[1] and chosen[1].get("all_passed"))

    if chosen_green and not fixes:
        diag["branch"] = "review-keep"
        finish([chosen[0]], diag)
        return

    diag["branch"] = "review-repair"
    repaired = sample(task, repair_prompt(task, chosen, fixes))

    if chosen_green:
        submitted = [chosen[0]]
        if repaired[0] is not None:
            submitted.append(repaired[0])
    else:
        submitted = codes
        if repaired[0] is not None:
            submitted.append(repaired[0])

    diag["repair_green"] = bool(
        repaired[1] and repaired[1].get("all_passed")
    )
    finish(submitted, diag)


if __name__ == "__main__":
    main()
```

### CANDIDATE 3: fourth-sample-synthesis

Hypothesis: Replacing only the fourth independent Luna sample with a solution synthesized from the first three drafts produces useful candidates that independent sampling misses.

Mechanism: The first three drafts provide concrete implementation alternatives and public feedback. Luna can reconcile omissions and conflicting interpretations while Sol retains the final selection. Unlike the failed Luna self-review approach, the synthesized file competes against preserved originals under the stronger judge; it never automatically replaces them.

Expected: **+1–5 pp**; the main risk is synthesis reinforcing shared mistakes and sacrificing independent diversity. This is the weakest of the three bets, but its upper range could clear the keep threshold.

```python
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
```