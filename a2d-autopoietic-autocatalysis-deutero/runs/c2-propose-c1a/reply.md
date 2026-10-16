### CANDIDATE 1: sol-probe-tests-repair

Hypothesis: One Sol call that writes up to 8 executable probe tests from the spec alone (edge cases, literal examples, degenerate sizes, scale), run against Luna's samples via `public_test`, exposes the shared misreadings and timeouts that every green file has; when no green file passes them, one Luna repair driven by the failing probes produces a hidden-passing file often enough to beat E002.

Mechanism: The evidence says the remaining E002 failures are generation failures, not selection failures: the judge has 0 misses, and in 29/34 failing runs every green file shares the misreading. The visible tests are the only executable signal Luna gets, and they are exactly what misses these clauses. D's repair loop shows that Luna fixes code well when a concrete test fails (about 99% public-green). So the missing piece is a failing test that encodes the misread clause. Luna writing its own tests (S1, S1b) failed because its tests share its own reading; here the tests come from a stronger, independent reader who never sees the code, like the hidden tests. The probe brief targets the C2-FA2 classes directly: literal examples (3), boundaries (9), precedence (3), zero or huge declared dimensions and preallocation (5 + 3), and scale tests for the 8 timeouts. No pending candidate turns Sol's reading into something executable. E006 gives prose notes before sampling, which may be ignored or correlate the samples; E007 critiques one chosen draft in prose.
- Budget, every branch, k = 5: 1 Sol prober (lane "judge", the same Sol medium config, reused so no second lane header is needed; DIAG records `prober`) + 3 Luna samples + exactly one of: the Sol judge, one Luna probe-guided repair, the incumbent's no-green repair, or nothing. At most 5 calls on every path.
- Probes run by appending `#[cfg(test)] mod a2d_probes { use super::*; ... }` to a copy of each distinct green file and calling `public_test` (free). Green status always comes from the bare file. Submitted files are always bare Luna code: probes are never submitted, and a repair that still contains probe code is not submitted.
- Decision after the 3 samples:
  - If some green file passes every probe, or the probes are unusable (prober failed, no green file compiles with them, or the output has no per-probe lines), the incumbent logic runs unchanged on the 3 samples: the judge over all distinct greens if there are 2 or more, otherwise one-green or no-green repair. The judge sees all greens, not only clean ones, so a wrong probe cannot hide the right file from it.
  - If there are greens but none passes all probes (the signature of a shared misreading or a timeout), call 5 is a Luna repair of the green file that passes the most probes. It gets the probe source, the failing probe names and output, and an explicit rule: requirements and visible tests win over a probe that contradicts them. The repair is submitted only if it is public-green and passes strictly more probes than the original; otherwise the original is submitted.
- Why the parts are coupled, which the rules require stating: the parts cannot work separately. Probes without a probe-driven repair change nothing, because the judge already has 0 misses, so extra selection information has no value. A repair without probes is D's loop, which E002 already beats. The third sample is dropped only because the prober costs a call.
- Clearing the -3 pp sample tax: at k = 3, about 13% of runs (0.950 - 0.819) have a green file but no hidden-passing one. If correct probes catch half of those and a test-driven repair fixes half of the caught ones, that is about +3.3 pp. Timeouts are the most mechanically detectable class (8 of 34). Runs where a probe catches a misreading in only some greens still go to the judge, so nothing is lost there.

Expected: +2 to +6 pp all-pass. Main risk: wrong probe expectations. Sol writes the probes from the spec, and if a probe is wrong, every correct green fails it and the run moves from the judge to a repair that may follow the wrong test, which costs the judge's selection on that run. Mitigations: the brief forbids uncertain expectations and limits scale and degenerate probes to cheap, certain facts; the repair prompt makes requirements win. Second risk: `public_test` may not execute tests appended to the file, or may report them differently. The file then degrades to the incumbent's logic on 3 samples (about -3 pp). DIAG `probe_status` shows which case happened from the first smoke run.

```python
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
```

### CANDIDATE 2: worst-case-lessons-all-samples

Hypothesis: Appending one fixed, task-agnostic checklist distilled from the C2-FA2 failure patterns, led by worst-case cost and degenerate sizes, to every Luna coder prompt raises the per-sample hidden pass rate above 0.643 enough to lift E002's all-pass at an unchanged budget and judge.

Mechanism: 29/34 failures are misreadings shared by every sample. A selector cannot fix those; information in front of every sample can. The cheapest such information is the recurring, task-agnostic patterns behind the failures:
- unbounded or declared-size work and zero dimensions (5 cases);
- preallocation from caller-supplied counts (3);
- brute force with no worst-case estimate (2), which together with the first two drives the 8 timeouts;
- literal examples not checked (3);
- precedence not implemented in listed order (3);
- sentinels that are legal values (2);
- streaming state not equal to one-shot (2);
- template literals versus placeholders (2).

S4b's four researcher lessons gave +5.6 pp over D. These lessons are newer, aimed at the failures the incumbent still has, and lead with performance, which C2-FA2 notes no queued candidate targets.
- Single variable: the coder prompt (samples and repair) gets the checklist appended after the task; everything else is byte-identical to E002. That includes the judge, which still sees the unmodified `task_prompt`.
- Budget is unchanged: 4 Luna samples + at most 1 judge or repair call = 5, about 4.0 Luna + 0.9 Sol per task.
- Difference from E005: E005 gives three different reading angles to samples 2 to 4 to decorrelate them, and leaves sample 1 plain. This gives every sample the same failure-derived content, so it raises the per-sample rate rather than spreading the samples. The two compose later if both keep.
- Arithmetic: the any-pass@4 ceiling (0.850) moves with the per-sample rate. Turning a third of the 29 shared misreadings (mostly the timeouts and degenerate-size cases, which a sentence can prevent) into at least one passing green is worth +3 to +4 pp.

Expected: +2 to +5 pp all-pass. Main risk: the checklist makes Luna defensive, adding rejections, early returns or clamping that the requirements do not ask for. Hidden tests on normal behaviour would then fail, and the samples would become more alike, so the judge has less to choose from. The checklist tells Luna that requirements win over it. Secondary risk: the patterns came from dev-set failures (train-label rule), so part of any gain may not transfer. They are phrased task-agnostically to limit that.

```python
#!/usr/bin/env python3
# lanes: judge=codex/gpt-6.1-sol:medium
"""Nanoharness candidate: E002 plus a failure-derived worst-case checklist on every coder prompt.

Idea: one fixed, task-agnostic checklist distilled from the C2-FA2
failure patterns (worst-case cost and degenerate sizes first, then literal
examples, precedence, sentinels, streaming, output templates), appended to
every Luna coder prompt, raises the per-sample hidden pass rate and so E002's
all-pass at the same budget.

Single change vs E002: the coder prompt is task_prompt + CHECKLIST (samples
and the no-green repair). The judge still sees the unmodified task_prompt.
Budget per task (k = 5) unchanged: 4 coder samples, then 1 judge call (two or
more distinct green files), 1 repair call (no green file), or nothing.
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

CHECKLIST = """

## Before you answer: hidden acceptance tests

Passing the visible tests is not enough. Hidden tests check every numbered requirement, with edge cases and inputs much larger than the visible ones. Check your file against each point below. Where a point and a numbered requirement disagree, the requirement wins; do not add behaviour the requirements do not ask for.

1. Worst-case cost. For each loop or search, estimate the work on the largest input the requirements allow. If it can exceed about 10^8 steps, or if a search may find nothing and run through its whole range, use a better algorithm or skip ahead in coarse units (whole blocks, rows, days) before fine ones.
2. Work bounded by actual data. Iterate over the elements that exist, not over a declared size, capacity or dimension. If any dimension or count is zero, return the empty result at once, even when another dimension is huge.
3. No preallocation from untrusted sizes. Never call with_capacity, vec![x; n] or similar with a count the caller supplies unless a requirement bounds it; grow as data arrives.
4. Literal examples. Run every example written in the requirements through your code by hand. The output must match exactly, character for character; in output templates, keep literal words literal and substitute only the placeholders.
5. Precedence. When several conditions or errors can apply, check them in exactly the order the requirements list them.
6. No sentinels. Do not use a value that is also legal data (0, -1, MAX, an empty string) to mean "none" or "unset"; use Option or a separate flag.
7. Incremental equals one-shot. If input can arrive in pieces (chunks, repeated calls, streaming), processing it in pieces must give exactly the same result as processing it all at once; keep partial state between calls.
8. Boundaries and arithmetic. Inclusive versus exclusive exactly as written; empty, single-element and maximum inputs; checked or wider arithmetic wherever values can overflow.
"""


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


def main():
    task = json.loads(sys.stdin.readline())["task"]
    base = task["task_prompt"].rstrip() + CHECKLIST
    diag = {"task": task["id"], "checklist": True}
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
    best = max(entries, key=rank)
    codes = [c for c, _ in entries if c is not None]
    if task["k"] >= 5:
        code, public = sample(task, repair_prompt(base, best[0], best[1], task["max_feedback_chars"]))
        if code is not None:
            codes.append(code)
    print("DIAG " + json.dumps(diag), file=sys.stderr, flush=True)
    request("done", codes=codes)


if __name__ == "__main__":
    main()
```

### CANDIDATE 3: written-requirement-trace

Hypothesis: Asking every Luna sample to write a short plain-text requirement trace before its code (one line per numbered requirement with exact behaviour, edge inputs and exact output; the largest allowed input with the complexity it needs; the reading chosen for each ambiguous clause, checked against the visible tests and examples) raises the per-sample hidden pass rate, and so E002's all-pass at the same budget.

Mechanism: Luna runs at low effort, so it does little hidden reasoning before it writes code. Most of C2-FA2's classes are failures to think a clause through, rather than knowledge gaps: boundary 9, semantics or state model 9, performance 8, precedence 3, format 3. Making the reasoning an explicit output, a line per requirement, forces Luna to visit every clause, including the ones the visible tests never exercise. It also forces one statement of worst-case input size and required complexity, which targets the 8 timeouts. This is a process change, not content: it adds no lessons and no Sol information, so it is distinct from candidate 2 and E006, and it composes with both.
- Difference from E005: E005 gives one of three reading angles as an "Approach" paragraph to samples 2 to 4 and leaves sample 1 plain. Here all four samples must write their reading down before coding, a harness-side substitute for reasoning effort. Changing the effort itself was declined as out of scope (r1-1, which Sol expected to be worth +4 to +8 pp).
- Single variable: the coder prompt gets the trace instruction appended (samples and the no-green repair). The judge, branches and budget are byte-identical to E002: 4 Luna + at most 1 Sol or Luna call = 5. The judge sees the unmodified `task_prompt` and only the extracted code, never the traces.
- The trace must be plain text with no code fences, so the controller's `extract` still finds the single ```rust block. DIAG counts samples with no code block, so an extraction regression shows up at the screen.

Expected: +1 to +4 pp all-pass. Main risk: at low effort Luna may restate the requirements mechanically without reasoning, which gives no gain and costs output tokens. Or it may commit to its misreading in writing, which would correlate the samples more and shrink the judge's choice. Secondary risk: stray code fences in the trace confuse extraction (watched through `no_code` in DIAG).

```python
#!/usr/bin/env python3
# lanes: judge=codex/gpt-6.1-sol:medium
"""Nanoharness candidate: E002 plus a written requirement trace before the code on every coder prompt.

Idea: asking every Luna sample to write, before its code, a short
plain-text trace (one line per numbered requirement with exact behaviour,
edge inputs and exact output; the largest allowed input and the complexity it
needs; the reading chosen for each ambiguous clause) raises the per-sample
hidden pass rate and so E002's all-pass at the same budget.

Single change vs E002: the coder prompt is task_prompt + TRACE (samples and
the no-green repair). The judge still sees the unmodified task_prompt and only
the extracted code. Budget per task (k = 5) unchanged: 4 coder samples, then 1
judge call (two or more distinct green files), 1 repair call (no green file),
or nothing.
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

TRACE = """

## How to answer

Hidden acceptance tests check every numbered requirement, including edge cases and inputs much larger than the visible tests. Before writing any code, write a short requirement trace in plain text (no code blocks, no backtick fences):

1. One line per numbered requirement, in order: "R<n>: <exactly what it requires>; edge inputs: <the empty, zero, boundary, tie or invalid inputs it covers and the exact result for each>; output: <exact value or text, if it names one>."
2. One line: "Scale: <largest input the requirements allow> -> needs <time complexity>; <what in your design guarantees it, and what happens when a size or dimension is zero>."
3. One line per clause you found ambiguous: "Reading of R<n>: <the reading you chose>, because <the visible test or example that supports it>."

Then write the whole file, implementing exactly the trace, in a single ```rust block. The rust block must be the only code block in your reply.
"""


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


def main():
    task = json.loads(sys.stdin.readline())["task"]
    base = task["task_prompt"].rstrip() + TRACE
    diag = {"task": task["id"], "trace": True}
    entries = [sample(task, base) for _ in range(min(4, task["k"]))]
    diag["no_code"] = sum(1 for code, _ in entries if code is None)
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
    best = max(entries, key=rank)
    codes = [c for c, _ in entries if c is not None]
    if task["k"] >= 5:
        code, public = sample(task, repair_prompt(base, best[0], best[1], task["max_feedback_chars"]))
        if code is not None:
            codes.append(code)
    print("DIAG " + json.dumps(diag), file=sys.stderr, flush=True)
    request("done", codes=codes)


if __name__ == "__main__":
    main()
```
