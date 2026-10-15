#!/usr/bin/env python3
"""G0: stateless retries at the configured effort, selecting the first visible pass."""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

FEEDBACK_HEADER = (
    "\n\n---\nA previous attempt at this task was rejected by the verifier. "
    "Start again from the original code. Verifier output from that attempt:\n"
)


def configured_effort() -> str:
    text = (Path(__file__).with_name("genome.toml")).read_text()
    match = re.search(r'^\s*effort\s*=\s*["\']([^"\']+)["\']', text, flags=re.MULTILINE)
    return match.group(1) if match else "high"


def render_verify(result: dict) -> str:
    lines = []
    for test in result.get("tests", []):
        if test.get("outcome") != "pass":
            lines.append(f"[{test.get('outcome', 'unknown')}] {test.get('name', test.get('index', '?'))}")
            message = test.get("message", "")
            if message:
                lines.append(str(message))
    if result.get("rejected"):
        lines.extend(f"[rejected] {reason}" for reason in result["rejected"])
    if result.get("error"):
        lines.append(f"[error] {result['error']}")
    return "\n".join(lines) or "No passing visible tests were reported."


def choose_action(state: dict, effort: str | None = None) -> dict:
    effort = effort or configured_effort()
    candidates = state.get("candidates", [])
    if not candidates:
        return {"agent": {"base": "fresh", "prompt": state["problem_statement"], "effort": effort}}
    last = candidates[-1]
    if last.get("verify", {}).get("passed"):
        return {"submit": {"candidate": last["index"]}}
    if state.get("budget", {}).get("agent_calls_left", 0) > 0:
        prompt = state["problem_statement"] + FEEDBACK_HEADER + render_verify(last.get("verify", {}))
        return {"agent": {"base": "fresh", "prompt": prompt, "effort": effort}}
    best = max(candidates, key=lambda candidate: (
        sum(test.get("outcome") == "pass" for test in candidate.get("verify", {}).get("tests", [])),
        candidate["index"],
    ))
    return {"submit": {"candidate": best["index"]}}


def main() -> int:
    state = json.loads(sys.stdin.read())
    sys.stdout.write(json.dumps(choose_action(state), separators=(",", ":")) + "\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
