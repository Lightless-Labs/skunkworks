#!/usr/bin/env python3
# lanes: judge=codex/gpt-6.1-sol:medium
"""One Luna draft followed by up to two Sol-review/Luna-repair rounds."""
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
    diag = {"task": task["id"], "rounds": []}

    # Preserve the incumbent's behavior outside the five-call setting.
    if task["k"] < 5:
        entries = [
            sample(task, task["task_prompt"])
            for _ in range(min(3, task["k"]))
        ]
        codes = [code for code, _ in entries if code is not None]
        diag["branch"] = "short-budget"
        finish(codes, diag)
        return

    entries = [sample(task, task["task_prompt"])]
    used = 1
    codes = [code for code, _ in entries if code is not None]
    submitted = list(codes)

    for round_number in range(1, 3):
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

        if not pool:
            diag["branch"] = "no-code-repair"
            last = entries[-1]
            while used < min(task["k"], 5):
                last = sample(task, repair_prompt(task, last, []))
                used += 1
                if last[0] is not None:
                    codes.append(last[0])
            diag["calls"] = used
            finish(codes, diag)
            return

        round_diag = {
            "round": round_number,
            "greens": len(greens),
        }
        chosen, fixes = review(
            task, greens or pool, bool(greens), round_diag
        )
        used += 1
        diag["rounds"].append(round_diag)

        chosen_green = bool(
            chosen[1] and chosen[1].get("all_passed")
        )
        if chosen_green and not fixes:
            round_diag["branch"] = "review-keep"
            diag["calls"] = used
            finish([chosen[0]], diag)
            return

        round_diag["branch"] = "review-repair"
        repaired = sample(task, repair_prompt(task, chosen, fixes))
        used += 1
        entries.append(repaired)
        if repaired[0] is not None:
            codes.append(repaired[0])

        if chosen_green:
            submitted = [chosen[0]]
            if repaired[0] is not None:
                submitted.append(repaired[0])
        else:
            submitted = list(codes)

        round_diag["repair_green"] = bool(
            repaired[1] and repaired[1].get("all_passed")
        )

    diag["calls"] = used
    finish(submitted, diag)


if __name__ == "__main__":
    main()
