### CANDIDATE 1: selected-candidate-revision

Hypothesis: Replacing the fourth independent Luna sample with a Luna revision guided by the existing Sol reviewer will repair shared specification mistakes that selection alone cannot resolve.

Mechanism: Use three unchanged Luna samples, then ask Sol to select and audit the strongest candidate, then let Luna make targeted corrections. Moving the review before the replacement sample and returning actionable criticism are inseparable parts of this intervention: revision requires both feedback and a writer call. Sol supplies prose only; every submitted file comes from Luna. Retain the original candidates so public regressions fall back automatically.

Expected: An unmeasured gain of approximately +5–8 percentage points in all-pass; the main risk is losing fourth-sample coverage and introducing revisions that pass public tests while regressing hidden behavior.

```python
#!/usr/bin/env python3
# lanes: judge=codex/gpt-6.1-sol:medium
"""Three independent Luna samples, Sol selection/audit, Luna revision.

Single intervention: replace the fourth independent sample with a revision
of the Sol-selected candidate. The existing reviewer necessarily runs
before that replacement sample and supplies actionable prose feedback.
All submitted source files are extracted from Luna responses.
"""
import json
import random
import re
import sys


REVIEW = """You are reviewing candidate solutions to a programming task.

TASK contains exactly the specification and visible tests supplied to the
programmer. Candidate files and their public test results follow.

First select the existing candidate most likely to satisfy EVERY numbered
requirement. Public-green files are not necessarily correct. Do not favor
an inferior candidate merely because it looks easier to repair.

Then audit the selected file against the entire specification, including
requirements implemented identically by every candidate. Trace concrete
boundary cases rather than merely comparing files. Check error precedence,
state transitions, end-of-input behavior, atomicity, numeric bounds,
degenerate inputs, deterministic ordering, and termination where relevant.
Only report defects justified by the actual task and selected source.

Your feedback will go to the original programmer for a targeted revision.
For each defect, identify the requirement, a concrete counterexample or
execution trace, the discrepancy, and the necessary conceptual correction.
Do not invent stricter requirements. Do not report speculative concerns as
confirmed defects. If you find no concrete defect, return an empty list.

Do NOT write Rust source, replacement functions, patches, or pseudocode.
All feedback must be prose.

Reply with exactly one JSON object:
{
  "pick": "<one shown letter>",
  "reason": "<one sentence>",
  "issues": [
    {
      "requirement": "<requirement reference>",
      "counterexample": "<concrete input or trace and required behavior>",
      "defect": "<what the selected file does differently>",
      "correction": "<conceptual correction in prose>"
    }
  ]
}
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


def sample(task, prompt):
    reply = request("model", system=task["coder_system"], prompt=prompt)
    code = (
        request("extract", text=reply.get("text", ""))["code"]
        if reply.get("ok")
        else None
    )
    public = request("public_test", code=code) if code is not None else None
    return code, public


def rank(entry):
    _, public = entry
    return (public["passed"], public["compiled"]) if public else (0, False)


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
        feedback = public["output"] if public is not None else "(unavailable)"
        prompt += truncate(feedback, limit)
    prompt += (
        "\n\nFix the file. Respond with the whole corrected file "
        "in a single ```rust block.\n"
    )
    return prompt


def parse_review(text):
    stripped = text.strip()
    if stripped.startswith("```"):
        lines = stripped.splitlines()
        if len(lines) >= 3 and lines[-1].strip() == "```":
            stripped = "\n".join(lines[1:-1])
    try:
        value = json.loads(stripped)
    except (ValueError, TypeError):
        return None
    return value if isinstance(value, dict) else None


def review(task, candidates, diag):
    order = list(range(len(candidates)))
    random.Random(task["id"]).shuffle(order)
    shown = [candidates[i] for i in order]
    letters = "ABCDEFGHIJKLMNOPQRSTUVWXYZ"[:len(shown)]
    diag["shown_order"] = order

    body = "TASK:\n" + task["task_prompt"] + "\n\nCANDIDATES:\n"
    for letter, (code, public) in zip(letters, shown):
        status = {
            "compiled": public.get("compiled", False),
            "passed": public.get("passed", 0),
            "expected": public.get("expected", 0),
            "all_passed": public.get("all_passed", False),
        }
        body += (
            "\n### " + letter + ".rs\n"
            + "Public result: " + json.dumps(status)
            + "\n```rust\n" + code.rstrip() + "\n```\n"
        )
        if not public.get("all_passed"):
            body += (
                "\nPublic feedback:\n"
                + truncate(public.get("output", ""), task["max_feedback_chars"])
                + "\n"
            )

    reply = request("model", lane="judge", system=REVIEW, prompt=body)
    fallback = candidates[0]
    if not reply.get("ok"):
        diag["review_fallback"] = "model-error"
        return fallback, None

    text = reply.get("text", "")
    parsed = parse_review(text)
    if parsed is not None and isinstance(parsed.get("pick"), str):
        pick = parsed["pick"].strip().upper()
    else:
        picks = re.findall(r'"pick"\s*:\s*"([^"]*)"', text)
        pick = picks[0].strip().upper() if len(picks) == 1 else ""

    if len(pick) != 1 or pick not in letters:
        diag["review_fallback"] = "invalid-pick"
        return fallback, None

    diag["pick"] = pick
    selected = shown[letters.index(pick)]
    issues = parsed.get("issues") if parsed is not None else None
    if not isinstance(issues, list):
        diag["review_fallback"] = "invalid-issues"
        return selected, None
    diag["issues"] = len(issues)
    return selected, issues


def revision_prompt(task, selected, issues):
    code, public = selected
    if issues is None:
        audit = (
            "Reviewer feedback was unavailable. Independently audit this "
            "file against every requirement before deciding what to fix."
        )
    else:
        audit = json.dumps({"issues": issues}, ensure_ascii=False)

    feedback = (
        "Public test results:\n"
        + public.get("output", "")
        + "\n\nReviewer feedback (advisory):\n"
        + audit
    )
    return (
        task["task_prompt"]
        + "\n\n## Selected previous implementation\n\n```rust\n"
        + code.rstrip()
        + "\n```\n\n## Review and public feedback\n\n"
        + truncate(feedback, task["max_feedback_chars"])
        + "\n\nVerify each reviewer claim against the specification and source. "
        "Correct confirmed defects with targeted changes, preserving correct "
        "behavior elsewhere. Check interacting requirements after each change. "
        "Do not rewrite working code for style. If no correction is justified, "
        "return the previous file unchanged. Respond with the whole final file "
        "in one ```rust block.\n"
    )


def finish(task, entries, selected, revision, diag):
    codes = [code for code, _ in entries if code is not None]
    if selected is not None:
        chosen = selected[0]
        codes = [code for code in codes if code != chosen]
        codes.append(chosen)
    if revision is not None and revision[0] is not None:
        codes.append(revision[0])
    print("DIAG " + json.dumps(diag), file=sys.stderr, flush=True)
    request("done", codes=codes)


def main():
    task = json.loads(sys.stdin.readline())["task"]
    budget = max(0, task["k"])
    diag = {"task": task["id"], "policy": "selected-candidate-revision"}

    entries = [
        sample(task, task["task_prompt"])
        for _ in range(min(3, budget))
    ]
    used = len(entries)

    unique = []
    seen = set()
    for code, public in entries:
        if code is not None and code not in seen:
            seen.add(code)
            unique.append((code, public))

    greens = [entry for entry in unique if entry[1].get("all_passed")]
    diag["greens"] = len(greens)

    if not unique:
        diag["branch"] = "no-source-repair"
        last = entries[-1] if entries else (None, None)
        while used < budget:
            last = sample(
                task,
                repair_prompt(
                    task["task_prompt"], last[0], last[1],
                    task["max_feedback_chars"],
                ),
            )
            used += 1
            entries.append(last)
            if last[1] is not None and last[1].get("all_passed"):
                break
        diag["calls"] = used
        finish(task, entries, None, None, diag)
        return

    selected = greens[0] if greens else max(unique, key=rank)
    revision = None

    if budget - used >= 2:
        diag["branch"] = "review-revise"
        candidates = greens if greens else sorted(
            unique, key=rank, reverse=True
        )
        selected, issues = review(task, candidates, diag)
        used += 1
        revision = sample(task, revision_prompt(task, selected, issues))
        used += 1
        diag["revision_green"] = bool(
            revision[1] and revision[1].get("all_passed")
        )
    else:
        diag["branch"] = "limited-budget"
        if used < budget and not greens:
            revision = sample(
                task,
                repair_prompt(
                    task["task_prompt"], selected[0], selected[1],
                    task["max_feedback_chars"],
                ),
            )
            used += 1

    diag["calls"] = used
    finish(task, entries, selected, revision, diag)


if __name__ == "__main__":
    main()
```