#!/usr/bin/env python3
"""Nanoharness: the coder harness as one editable file (A²D cycle 2).

v0 is the plain repair loop (lift arm D), byte for byte: the same system
prompt, task prompt, repair prompt, feedback truncation and stopping rule.
Every later change is one experiment, kept only if it beats the incumbent
above the A/A threshold (docs/experiments/LOG.md, Cycle 2).

The controller runs this file as an external arm (`--external name=path`)
and speaks JSON lines on stdin/stdout:
  first line in:   {"task": {"id", "family", "task_prompt", "coder_system",
                             "k", "max_feedback_chars"}}
  requests out:    {"op": "model", "system", "prompt"}   -> {"ok", "text"|"error"}
                   {"op": "extract", "text"}              -> {"code"}
                   {"op": "public_test", "code"}          -> TestScore
                   {"op": "done", "codes": [...]}         (ends the episode)
Model calls are metered by the controller (budget k per task). Hidden tests
are never reachable from here.
"""
import json
import sys


def request(op, **fields):
    sys.stdout.write(json.dumps({"op": op, **fields}) + "\n")
    sys.stdout.flush()
    if op == "done":
        return None
    return json.loads(sys.stdin.readline())


def truncate(text, limit):
    """As arms.rs `truncate`: a byte limit, cut back to a char boundary."""
    data = text.encode("utf-8")
    if len(data) <= limit:
        return text
    end = limit
    while end > 0 and (data[end] & 0xC0) == 0x80:
        end -= 1
    return data[:end].decode("utf-8") + "\n... [truncated]"


def repair_prompt(base, code, public, limit):
    """As arms.rs `repair_prompt`."""
    prompt = base
    if code is None:
        prompt += "\n\n## Previous attempt\n\nYour previous response contained no ```rust code block.\n"
    else:
        prompt += "\n\n## Previous attempt\n\n```rust\n" + code.rstrip() + "\n```\n\n## Public test results\n\n"
        feedback = public["output"] if public is not None else "(this task has no public tests)"
        prompt += truncate(feedback, limit)
    prompt += "\n\nFix the file. Respond with the whole corrected file in a single ```rust block.\n"
    return prompt


def main():
    task = json.loads(sys.stdin.readline())["task"]
    base = task["task_prompt"]
    codes = []
    last = None  # (code, public score) of the previous attempt
    for _ in range(task["k"]):
        prompt = base if last is None else repair_prompt(base, last[0], last[1], task["max_feedback_chars"])
        reply = request("model", system=task["coder_system"], prompt=prompt)
        code = request("extract", text=reply["text"])["code"] if reply.get("ok") else None
        public = request("public_test", code=code) if code is not None else None
        if code is not None:
            codes.append(code)
        last = (code, public)
        if public is not None and public.get("all_passed"):
            break
    request("done", codes=codes)


if __name__ == "__main__":
    main()
