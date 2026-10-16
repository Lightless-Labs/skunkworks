### CANDIDATE 1: two-readings-per-call

Hypothesis: If each of the four Luna sample calls returns two complete files, one following its most likely reading and a hedge that takes the other reading where it is least sure, the judged pool will contain the correct reading on tasks where four independent samples all share a misreading. That should raise all-pass over E002 at the same 5-call cap.

Mechanism:
- **Selection is not the bottleneck; the pool is.** E002's all-pass (0.857) equals the any-pass ceiling of its own samples. The judge never picked a hidden-failing file when a passing green existed (0 misses in 249 runs), and a Claude judge got 66/68.
- **Most failures are shared misreadings.** In 29 of 34 diagnosed failures, every file shares the same misreading.
- **More independent samples help less and less.** Any-pass rises +12.1, +5.5 and then +3.1 pp per extra sample, because each new independent sample mostly draws the same reading again.
- **The hedge differs by construction.** It is written in the same reply and is told to differ exactly where the coder is unsure:
  - boundaries and empty cases;
  - order of checks;
  - exact text;
  - worst-case algorithm.
- **It costs no call.** One call now yields two files, and the judge already takes up to 8 letters.
- **More greens also help the weakest branches.** Extra green files move one-green runs (13 of 23 fail) and no-green runs (6 of 8 fail) into the judge branch, where E002 fails only 8% of the time.
- **Why the bullet list is coupled to the hedge, not a second variable.** The short list written before the code defines *where* Solution B differs from Solution A. Without it, B cannot be aimed at the coder's uncertain points and becomes a correlated second draft.
  - The list may also improve Solution A, since it is a pre-code check of the C2-FA2 classes.
  - Attribution control if this candidate is kept: the same bullet list with a single file per call.
- **What stays as in E002:**
  - the judge prompt;
  - the shuffle;
  - parsing of the judge's pick;
  - all branches;
  - the no-green repair (one file, unchanged).

Expected: about +3 to +5 pp all-pass (plausible range 0 to +6).
- Main risk: Solution B rarely carries the needed reading, because many shared misreadings are not points the coder sees as uncertain. Meanwhile the longer reply could lower Solution A's quality.
- Second risk: judging up to 8 near-identical files may be less accurate than judging 2 to 4. Claude was 37/38 at 4 files; 8 files has not been measured.
- Smoke check: DIAG `files_per_call` must mostly show 2. Otherwise the candidate quietly turns back into E002.
- Cost: Luna output tokens rise by roughly 60–90%, and the judge prompt about doubles.

```python
#!/usr/bin/env python3
# lanes: judge=codex/gpt-6.1-sol:medium
"""Nanoharness candidate: two readings per coder call, then E002's judge.

One change vs E002 (nano/harness.py): each of the four Luna sample calls asks
for TWO complete files in one reply. Solution A follows the readings the coder
thinks most likely; Solution B takes the other reading on the one to three
points it is least sure of (or, if it is sure of every point, a different
algorithm or data structure). Every parsed file is public-tested (free), and
the unchanged Sol judge picks among up to 8 distinct public-green files.

Why: E002's all-pass equals the any-pass ceiling of its own samples (C2-FA1),
the judge never misses when a passing green exists, and 29 of 34 failures are a
misreading shared by every file (C2-FA2). Independent samples keep re-drawing
the same reading; a hedge written in the same reply differs by construction,
and costs no extra call.

Budget per task (k = 5): 4 coder calls (up to two files each), then 1 judge call
(two or more distinct green files), 1 repair call (no green file; E002's
single-file repair, unchanged), or nothing (one green file). One DIAG line per
task: branch, files parsed per call, origin of each green and of the pick
(call number and slot A/B), shown order, fallback reason.
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

TWO_FILES = """

## Answer format for this request: two complete files

Hidden acceptance tests check every numbered requirement. A file that passes the visible tests but fails hidden ones has usually read some requirement the wrong way, so hedge:

1. First, in at most six short bullet points, list the places where the requirements could be read two ways or are easy to get wrong (order of checks, boundaries, empty or zero cases, exact output text, overflow, cost on the largest sizes allowed), and say which reading you take.
2. Then a line `### Solution A`, followed by the whole file in one ```rust block, following the readings you think most likely.
3. Then a line `### Solution B`, followed by the whole file in one ```rust block, taking the other reading on the one to three points you are least sure of. If you are sure of every point, make B an independent implementation with a different algorithm or data structure instead.

Both files must be complete, build on their own and pass the visible tests. Use no other code blocks.
"""

SYSTEM_NOTE = (
    "\n\nFor this request only: reply with two complete files in two ```rust blocks "
    "(Solution A, then Solution B), as the prompt describes."
)

# A fenced rust block whose fences sit on their own lines, so that a doc-comment
# fence inside the code (three backticks after "///") does not end the block.
FENCE = r"^[ \t]*```(?:rust|rs)[^\n]*\n(.*?)^[ \t]*```[ \t]*$"
LETTERS = "ABCDEFGH"


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
    """E002's single-file coder call (used only for the no-green repair)."""
    reply = request("model", system=task["coder_system"], prompt=prompt)
    code = request("extract", text=reply["text"])["code"] if reply.get("ok") else None
    public = request("public_test", code=code) if code is not None else None
    return code, public


def parse_files(text):
    """The fenced rust blocks of a reply: at most two (the longest), in reply order."""
    blocks = []
    for body in re.findall(FENCE, text, re.S | re.M):
        if body.strip() and body not in blocks:
            blocks.append(body)
    if len(blocks) > 2:
        longest = sorted(range(len(blocks)), key=lambda i: len(blocks[i]), reverse=True)[:2]
        blocks = [blocks[i] for i in sorted(longest)]
    return blocks


def sample_pair(task, prompt, call_no, diag):
    """One coder call that should return two files; each parsed file is public-tested."""
    reply = request("model", system=task["coder_system"] + SYSTEM_NOTE, prompt=prompt)
    if not reply.get("ok"):
        diag["files_per_call"].append(0)
        return []
    text = reply.get("text") or ""
    codes = parse_files(text)
    if not codes:
        code = request("extract", text=text)["code"]
        codes = [code] if code is not None else []
    diag["files_per_call"].append(len(codes))
    out = []
    for slot, code in enumerate(codes):
        public = request("public_test", code=code)
        out.append((code, public, str(call_no) + "AB"[slot]))
    return out


def rank(entry):
    public = entry[1]
    return (public["passed"], public["compiled"]) if public else (0, False)


def judge(task, greens, diag):
    letters = LETTERS[: len(greens)]
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
    base = task["task_prompt"]
    diag = {"task": task["id"], "files_per_call": []}
    entries = []
    for call_no in range(1, min(4, task["k"]) + 1):
        entries.extend(sample_pair(task, base + TWO_FILES, call_no, diag))
    greens = []
    origins = []
    for code, public, origin in entries:
        if public and public.get("all_passed") and code not in greens and len(greens) < len(LETTERS):
            greens.append(code)
            origins.append(origin)
    diag["greens"] = len(greens)
    diag["green_origins"] = origins
    if len(greens) >= 2 and task["k"] >= 5:
        diag["branch"] = "judge"
        chosen = judge(task, greens, diag)
        diag["picked_origin"] = origins[greens.index(chosen)]
        print("DIAG " + json.dumps(diag), file=sys.stderr, flush=True)
        request("done", codes=[chosen])
        return
    if greens:
        diag["branch"] = "one-green"
        print("DIAG " + json.dumps(diag), file=sys.stderr, flush=True)
        request("done", codes=[greens[0]])
        return
    diag["branch"] = "no-green-repair"
    codes = [entry[0] for entry in entries]
    if task["k"] >= 5:
        if entries:
            best = max(entries, key=rank)
            code, public = sample(task, repair_prompt(base, best[0], best[1], task["max_feedback_chars"]))
        else:
            code, public = sample(task, repair_prompt(base, None, None, task["max_feedback_chars"]))
        if code is not None:
            codes.append(code)
    print("DIAG " + json.dumps(diag), file=sys.stderr, flush=True)
    request("done", codes=codes)


if __name__ == "__main__":
    main()
```

### CANDIDATE 2: failure-class-checklist

Hypothesis: Appending one task-agnostic checklist to every Luna coder prompt raises the per-sample hidden pass rate, and with it all-pass over E002. The checklist covers the failure classes C2-FA2 found:
- work bounded by the data actually present;
- worst-case cost;
- the listed order of checks;
- boundaries and overflow;
- sentinels that are also legal values;
- incremental state equal to one-shot;
- exact text;
- tracing literal examples;
- API names and derives.

Mechanism:
- **The gap to close.** A sample is public-green 0.815 of the time but passes all hidden tests only 0.643 of the time, so about a fifth of green files fail hidden tests.
- **The failures fall into recurring, task-agnostic classes** (C2-FA2):

  | Class | Cases |
  |---|---|
  | Boundary / edge case | 9 |
  | Semantics / state model | 9 |
  | Performance (all timeouts) | 8 |
  | Precedence | 3 |
  | Exact format | 3 |
  | API | 1 |
  | Overflow | 1 |

- **Performance is untargeted.** The LOG says no queued candidate targets it:
  - the 8 timeouts;
  - 5 loops over a declared dimension when another is zero;
  - 3 preallocations from caller-supplied counts;
  - 2 unbounded searches.
- **Hidden compile failures.** The 2 hidden compile failures and the 1 API-contract case point to missing derives or names (item 9).
- **Prior evidence.** In cycle 1, S4b's four researcher lessons gave +5.6 pp over D.
- **Why a per-sample gain counts in full here.** The judge is at its ceiling, so any per-sample gain on a shared misreading lifts any-pass@4 directly. No pending candidate supplies this information to every sample: E005 diversifies angles, E006 uses task-specific Sol notes.
- **What stays as in E002:**
  - the judge sees the original task prompt, without the checklist;
  - every branch, budget and DIAG line.

Expected: about +2 to +3 pp all-pass.
- Main risk: the checklist was mined from E002's failures on this same dev set, so the dev estimate is optimistic and needs a held-out check.
- Second risk: a uniform prompt adds no diversity. Over-defensive code (extra validation, derives that do not build) can cost a green or break the API.

```python
#!/usr/bin/env python3
# lanes: judge=codex/gpt-6.1-sol:medium
"""Nanoharness candidate: E002 plus a failure-class checklist in every coder prompt.

One change vs E002 (nano/harness.py): the coder's task prompt (the four samples
and the no-green repair) gets one appended section listing the task-agnostic
failure classes found in E002's failing runs (C2-FA2): work not bounded by the
data present, worst-case cost, listed order of checks, boundaries and
overflow, sentinels that are legal data, incremental state equal to one-shot,
exact text, tracing literal examples, API names and derives. The judge sees the
original task prompt (without the checklist); every branch, the budget and the
DIAG line are E002's.

Disclosure: the classes were mined from E002 failures on the dev set and are
measured on the dev set, so the dev estimate is optimistic. The text is
task-agnostic (no task, family or test content).
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

## Before you answer: where files that pass the visible tests usually fail

Hidden acceptance tests check every numbered requirement, with larger and more degenerate data than the visible tests. Check your file against each point below before you reply.

1. Work bounded by the data present. Never loop or allocate over a declared size or a caller-supplied count (dimensions, capacity, n) when another dimension may be zero or the count may be huge: iterate over the elements that actually exist, return early on empty cases, and never pass an unchecked caller value to `with_capacity` or `vec![x; n]`.
2. Worst-case cost. Estimate the largest arguments the requirements allow and keep the algorithm near-linear on them. Every search or retry loop needs a bound that still holds when no answer exists.
3. Order of checks. Where the requirements list rules, validations or precedence, apply them in exactly the listed order, and resolve ties exactly as stated.
4. Boundaries. Inclusive versus exclusive ends, zero, one element, empty strings, minimum and maximum values; use checked or wider arithmetic wherever a value can overflow.
5. Sentinels. Never use a value as a marker (0, -1, MAX, an empty string) if it can also be legal data; use `Option` or an enum.
6. Incremental state. If data can arrive in pieces (chunks, repeated calls), the result must equal processing it all at once, for every split point.
7. Exact text. Reproduce literal words, punctuation, spacing and case exactly; in templates, tell fixed words apart from placeholders.
8. Examples. Trace every literal example in the requirements through your code and confirm the exact result.
9. API. Use exactly the names and signatures the requirements and tests use, and derive `Debug`, `Clone` and `PartialEq` on public types where all their fields allow it, so tests can compare and print them.
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
    base = task["task_prompt"]
    coder_base = base + CHECKLIST
    diag = {"task": task["id"], "checklist": True}
    entries = [sample(task, coder_base) for _ in range(min(4, task["k"]))]
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
    best = max(entries, key=rank) if entries else (None, None)
    codes = [c for c, _ in entries if c is not None]
    if task["k"] >= 5:
        code, public = sample(task, repair_prompt(coder_base, best[0], best[1], task["max_feedback_chars"]))
        if code is not None:
            codes.append(code)
    print("DIAG " + json.dumps(diag), file=sys.stderr, flush=True)
    request("done", codes=codes)


if __name__ == "__main__":
    main()
```

### CANDIDATE 3: route-sol-by-greens

Hypothesis: Choose the Sol call's job after three Luna samples. With two or more distinct greens, keep E002 (a fourth sample, then the judge). With at most one green, Sol writes no-code notes from the task and the three drafts, and one Luna call repairs the best draft with those notes. This raises all-pass, because the judge has nothing to select on exactly the runs where E002 fails most.

Mechanism:
- **Failures concentrate where the judge cannot act.** E002's one-green and no-green branches are 31 of 280 runs but 19 of its 40 failures: 61% fail there, against 8% in the judge branch.
- **The judge is useless on those runs by construction.** With at most one green there is nothing to select.
- **E002 wastes a call there.** In the one-green branch it leaves the fifth call unused.
- **Routing works on an observed, free signal:** the number of public-green files after three samples.
- **Easy runs are unchanged.** On roughly 85% of runs the samples, the judge and the submission are exactly E002's.
- **Hard runs get a stronger reader.** Sol sees all three drafts and their visible test results, which is the most information available when Luna is struggling. The fifth call is a Luna repair of the best draft (the green one if there is one), carrying the notes and its public feedback; D's repair loop reliably reaches green.
- **Submission:** the drafts plus the repair, under the controller's rule, so a green repair supersedes.
- **Sol writes no code.** Code blocks in its reply are removed before Luna sees it, and every submitted file is Luna's.
- **Overlap with pending candidates (disclosed).** The notes call resembles E006's notes and E007's critique. The new variable is the routing: it gives up the fourth sample only where E002's fourth sample plus the judge earns little, instead of on every task.

Expected: about +1.5 to +3 pp all-pass.
- Main risk: some runs would have reached two greens with the fourth sample and been judged well; they now go to the notes route.
- Second risk: wrong notes push a correct single green into a wrong repair, which then wins the controller's tie.
- Confound: Sol authored the tasks, the same disclosure as E006.

```python
#!/usr/bin/env python3
# lanes: judge=codex/gpt-6.1-sol:medium
"""Nanoharness candidate: route the Sol call by how many greens three samples give.

One change vs E002 (nano/harness.py): what the Sol call is for is decided after
three Luna samples instead of four.
- Two or more distinct public-green files after three samples: E002 exactly
  (a fourth sample, then the unchanged judge over the distinct greens).
- At most one green after three samples: the Sol call (same lane, no code)
  writes numbered notes on what the requirements demand where the drafts go
  wrong or are at risk, given the task and all three drafts with their visible
  test results; the fifth call is one Luna repair of the best draft (the green
  one if there is one) with those notes. Submission: the drafts plus the
  repair, under the controller's public-rank rule, so a green repair supersedes.

Why: E002's one-green and no-green branches are 31 of 280 runs but 19 of its
40 failures; the judge cannot help with at most one green, and in the
one-green branch E002 leaves the fifth call unused. Routing keeps E002 where
selection works and spends Sol's reading where it does not. Overlap: the notes
call resembles E006's notes and E007's critique; the new variable is the
routing on an observed signal.

Budget per task (k = 5): 3 samples + (1 sample + 1 judge | 1 notes + 1 repair).
Sol writes no code: code blocks in its reply are removed before Luna sees it.
One DIAG line per task: branch, greens after 3, notes size, pick or fallback.
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

ANALYST = """You are a senior reviewer. A programmer is struggling with the task below: after three independent attempts, at most one passes the visible tests. Do not write code.

The task, exactly as the programmer received it (numbered requirements and the visible tests), is under TASK. The attempts so far, each with its visible test results, are under ATTEMPTS. Hidden acceptance tests check every numbered requirement, with larger and more degenerate data than the visible tests.

Write at most 10 numbered notes for the programmer's next attempt. Each note cites the requirement numbers it concerns and states exactly what they demand where the attempts go wrong or are at risk: first the cause of each visible test failure, then misread requirements, order of checks, boundaries and empty cases, exact output text, and cost on the largest sizes allowed. Be concrete (exact values, exact order, exact text). Plain prose only: no code and no code blocks."""

ATTEMPT_FEEDBACK = 2000  # bytes of visible-test output per draft shown to Sol
NOTES_LIMIT = 5000  # bytes of notes passed to the coder


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


def noted_repair_prompt(base, code, public, notes, limit):
    prompt = base
    if code is None:
        prompt += "\n\n## Previous attempt\n\nYour previous response contained no ```rust code block.\n"
    else:
        prompt += "\n\n## Previous attempt\n\n```rust\n" + code.rstrip() + "\n```\n\n## Public test results\n\n"
        feedback = public["output"] if public is not None else "(this task has no public tests)"
        prompt += truncate(feedback, limit)
    prompt += (
        "\n\n## Reviewer notes\n\n"
        "A senior reviewer read the requirements and every attempt so far. Hidden acceptance tests check "
        "every numbered requirement; where your previous attempt and a note disagree, follow the note's "
        "reading of the requirements.\n\n"
    )
    prompt += notes
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


def distinct_greens(entries):
    greens = []
    for code, public in entries:
        if public and public.get("all_passed") and code not in greens:
            greens.append(code)
    return greens


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


def attempts_text(entries):
    out = ""
    for number, (code, public) in enumerate(entries, 1):
        out += "\n### Attempt " + str(number) + "\n"
        if code is None:
            out += "(the reply contained no ```rust code block)\n"
            continue
        out += "```rust\n" + code.rstrip() + "\n```\n"
        if public is None:
            out += "Visible tests: not run.\n"
        elif public.get("all_passed"):
            out += "Visible tests: all pass.\n"
        else:
            out += "Visible tests: " + str(public.get("passed")) + " of " + str(public.get("expected"))
            out += " pass. Output:\n" + truncate(public.get("output") or "", ATTEMPT_FEEDBACK) + "\n"
    return out


def clean_notes(text):
    """Strip any code blocks (Sol must not write code that reaches the coder), then cap."""
    text = re.sub(r"```.*?```", "[code removed]", text, flags=re.S)
    text = re.sub(r"```.*", "[code removed]", text, flags=re.S)
    return truncate(text.strip(), NOTES_LIMIT)


def emit(diag):
    print("DIAG " + json.dumps(diag), file=sys.stderr, flush=True)


def main():
    task = json.loads(sys.stdin.readline())["task"]
    base = task["task_prompt"]
    limit = task["max_feedback_chars"]
    diag = {"task": task["id"]}
    entries = [sample(task, base) for _ in range(min(3, task["k"]))]
    greens = distinct_greens(entries)
    diag["greens_after_3"] = len(greens)
    if task["k"] < 5:
        while len(entries) < min(4, task["k"]):
            entries.append(sample(task, base))
        greens = distinct_greens(entries)
        diag["branch"] = "small-k"
        emit(diag)
        request("done", codes=greens[:1] if greens else [c for c, _ in entries if c is not None])
        return
    if len(greens) >= 2:
        entries.append(sample(task, base))
        greens = distinct_greens(entries)
        diag["greens"] = len(greens)
        diag["branch"] = "judge"
        chosen = judge(task, greens, diag)
        emit(diag)
        request("done", codes=[chosen])
        return
    diag["branch"] = "notes-repair"
    body = "TASK:\n" + base + "\n\nATTEMPTS:\n" + attempts_text(entries)
    reply = request("model", lane="judge", system=ANALYST, prompt=body)
    notes = clean_notes(reply.get("text") or "") if reply.get("ok") else ""
    diag["notes_bytes"] = len(notes.encode("utf-8"))
    if greens:
        target = next(e for e in entries if e[0] == greens[0])
    else:
        target = max(entries, key=rank)
    diag["target_green"] = bool(greens)
    if notes:
        prompt = noted_repair_prompt(base, target[0], target[1], notes, limit)
    else:
        diag["fallback"] = "no notes: " + str(reply.get("error", ""))[:120]
        prompt = base if greens else repair_prompt(base, target[0], target[1], limit)
    code, public = sample(task, prompt)
    diag["repair_green"] = bool(public and public.get("all_passed"))
    codes = [c for c, _ in entries if c is not None]
    if code is not None:
        codes.append(code)
    emit(diag)
    request("done", codes=codes)


if __name__ == "__main__":
    main()
```
