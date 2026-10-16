#!/usr/bin/env python3
# lanes: judge=codex/gpt-6.1-sol:medium
"""Nanoharness candidate: Sol probe tests from the spec, 3 Luna samples, judge or probe-guided repair.

Idea: one Sol call that writes up to 8 executable probe tests from the
spec alone exposes the shared misreadings and timeouts that every green Luna
file has. When no green file passes the probes, one Luna repair driven by the
failing probes beats E002's fourth sample.

Budget per task (k = 5): 1 Sol prober (controller lane "judge", the same Sol
medium config, used for a second purpose; DIAG says which) + 3 Luna samples
+ one of: the Sol judge (some green passes every probe, or the probes are
unusable, and 2 or more distinct greens), one Luna probe-guided repair (greens
exist, none passes every probe), the incumbent's no-green repair, or nothing.
Probes run by appending a test module to a copy of each green file and calling
public_test (free). Submitted files are always bare Luna code. If the probes
cannot be run, the incumbent's decision logic runs unchanged on the 3 samples.
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

PROBER = """You write extra acceptance tests for a programming task, from its specification alone.

The task, exactly as the programmer receives it (numbered requirements and the visible tests), is under TASK. You will not see any solution. Your tests will be run against several independent solutions to catch requirements that they misread. A test with a wrong expectation does more harm than a missing test.

Write at most 8 Rust test functions that the visible tests do not already cover. Aim them at the clauses most likely to be misread:
- literal examples stated in the requirements (exact values, exact output text, literal words versus placeholders);
- boundaries: empty, zero, one, maximum, inclusive versus exclusive limits, ties, values that look like sentinels;
- order of checks and precedence exactly as the requirements list them;
- incremental or chunked input versus all at once, where the requirements say they must agree;
- degenerate shapes: a zero dimension or count next to a huge one, or a huge declared size with little actual data (assert only what the requirements state, so a correct solution returns at once);
- at most two scale tests, sized so that an efficient solution finishes in well under one second in a debug build while a quadratic or exhaustive one does not. Assert only facts that are cheap and certain.

Rules:
- Assert only behaviour that a numbered requirement or a stated example fixes unambiguously. If you are not certain of an expected value, leave that test out.
- Use only the API that the visible tests use (the same function names, types, constructors, methods and argument shapes). Write std types with full paths (std::collections::HashMap and so on).
- Name every test function probe_r<requirement number>_<short_name>, put #[test] on each, and start each body with a one-line comment quoting the requirement it checks.
- Do not write a module wrapper, a `use super::*;` line, or any solution code.

Reply with the test functions in ONE ```rust block and nothing else."""

CALLS = {"used": 0}


def request(op, **fields):
    if op == "model":
        CALLS["used"] += 1
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


def write_probes(task, diag):
    """One Sol call; returns (test functions source, probe names) or (None, [])."""
    reply = request("model", lane="judge", system=PROBER, prompt="TASK:\n" + task["task_prompt"])
    if not reply.get("ok"):
        diag["prober"] = "call failed: " + str(reply.get("error", ""))[:120]
        return None, []
    body = request("extract", text=reply["text"])["code"]
    if not body:
        diag["prober"] = "no rust block"
        return None, []
    kept = []
    for line in body.splitlines():
        if re.match(r"\s*use\s+super::\*\s*;\s*$", line):
            continue
        kept.append(line)
    body = "\n".join(kept).strip("\n")
    names = []
    for name in re.findall(r"fn\s+(probe_\w+)\s*\(", body):
        if name not in names:
            names.append(name)
    if not names:
        diag["prober"] = "no probe_ functions"
        return None, []
    diag["prober"] = "ok"
    diag["probes"] = len(names)
    return body, names


def with_probes(code, body):
    return (
        code.rstrip()
        + "\n\n#[cfg(test)]\n#[allow(unused_imports, unused_variables, unused_mut, dead_code)]\n"
        + "mod a2d_probes {\n    use super::*;\n\n"
        + body
        + "\n}\n"
    )


def run_probes(code, body, names):
    """Returns {"status": "ran"|"nocompile"|"unknown", "passed", "failed", "output"}."""
    res = request("public_test", code=with_probes(code, body))
    output = res.get("output", "") or ""
    if not res.get("compiled"):
        return {"status": "nocompile", "passed": 0, "failed": list(names), "output": output}
    seen = {}
    for name, status in re.findall(r"test\s+(?:[\w:]*::)?(probe_\w+)\s+\.\.\.\s+(ok|FAILED|ignored)", output):
        seen[name] = status
    if not seen:
        timed_out = re.search(r"(?i)timed?\s*out|timeout", output) is not None
        other_tests = re.search(r"test\s+\S+\s+\.\.\.\s+(ok|FAILED)", output) is not None
        if other_tests or not timed_out:
            return {"status": "unknown", "passed": 0, "failed": [], "output": output}
    passed = [n for n in names if seen.get(n) == "ok"]
    failed = [n for n in names if seen.get(n) != "ok"]
    return {"status": "ran", "passed": len(passed), "failed": failed, "output": output}


def strip_probes(code):
    """Remove an a2d_probes module if the repair copied it; None if probe code remains."""
    if code is None:
        return None
    lines = code.splitlines()
    for i, line in enumerate(lines):
        if re.match(r"\s*mod\s+a2d_probes\b", line):
            start = i
            while start > 0 and lines[start - 1].strip().startswith("#["):
                start -= 1
            code = "\n".join(lines[:start]).rstrip() + "\n"
            break
    if "a2d_probes" in code or re.search(r"fn\s+probe_\w+\s*\(", code):
        return None
    return code


def probe_repair_prompt(base, code, body, result, limit):
    prompt = base
    prompt += "\n\n## Your previous attempt\n\n```rust\n" + code.rstrip() + "\n```\n\n"
    prompt += "It passes the visible tests.\n\n## Additional tests\n\n"
    prompt += (
        "A reviewer wrote these extra tests from the numbered requirements, without seeing your code. "
        "They were appended to your file inside `mod a2d_probes { use super::*; ... }`:\n\n"
    )
    prompt += "```rust\n" + body.rstrip() + "\n```\n\n"
    prompt += "## Results of the additional tests\n\n"
    prompt += "Failed or did not finish: " + ", ".join(result["failed"]) + "\n\n"
    prompt += truncate(result["output"], limit)
    prompt += (
        "\n\nFor each failing test, re-read the requirement it quotes. If the test matches the requirement, "
        "fix your code (a test that did not finish means your code is too slow or does unbounded work on that "
        "input). If a test contradicts a numbered requirement or a visible test, the requirement and the "
        "visible tests win: keep that behaviour. The file must still pass the visible tests.\n\n"
        "Respond with the whole corrected file in a single ```rust block. Do not include the additional tests.\n"
    )
    return prompt


def finish(diag, codes):
    print("DIAG " + json.dumps(diag), file=sys.stderr, flush=True)
    request("done", codes=codes)


def main():
    task = json.loads(sys.stdin.readline())["task"]
    base = task["task_prompt"]
    k = task["k"]
    diag = {"task": task["id"]}
    body, names = (None, [])
    use_prober = k >= 5
    if use_prober:
        body, names = write_probes(task, diag)
    n_samples = 3 if use_prober else min(4, k)
    entries = [sample(task, base) for _ in range(n_samples)]
    greens = []
    for code, public in entries:
        if public and public.get("all_passed") and code not in greens:
            greens.append(code)
    diag["greens"] = len(greens)

    results = []
    if body is not None:
        results = [run_probes(code, body, names) for code in greens]
    ran = [i for i, r in enumerate(results) if r["status"] == "ran"]
    clean = [i for i in ran if not results[i]["failed"]]
    diag["probe_status"] = [r["status"] for r in results]
    diag["probe_passed"] = [r["passed"] for r in results]
    left = k - CALLS["used"]

    if greens and ran and not clean and left >= 1:
        diag["branch"] = "probe-repair"
        target = max(ran, key=lambda i: (results[i]["passed"], -i))
        original = greens[target]
        before = results[target]
        diag["repair_target"] = target
        prompt = probe_repair_prompt(base, original, body, before, task["max_feedback_chars"])
        code, public = sample(task, prompt)
        code = strip_probes(code)
        chosen = original
        if code is not None and code != original:
            public = request("public_test", code=code)
            if public.get("all_passed"):
                after = run_probes(code, body, names)
                diag["repair_probe"] = [after["status"], after["passed"]]
                if after["status"] == "ran" and after["passed"] > before["passed"]:
                    chosen = code
                    diag["submitted"] = "repair"
            else:
                diag["repair_public"] = [public.get("passed"), public.get("expected")]
        diag.setdefault("submitted", "original")
        finish(diag, [chosen])
        return

    if len(greens) >= 2 and left >= 1:
        diag["branch"] = "judge"
        finish(diag, [judge(task, greens, diag)])
        return
    if greens:
        diag["branch"] = "one-green"
        finish(diag, [greens[0]])
        return
    diag["branch"] = "no-green-repair"
    best = max(entries, key=rank)
    codes = [c for c, _ in entries if c is not None]
    if left >= 1:
        code, public = sample(task, repair_prompt(base, best[0], best[1], task["max_feedback_chars"]))
        if code is not None:
            codes.append(code)
    finish(diag, codes)


if __name__ == "__main__":
    main()
