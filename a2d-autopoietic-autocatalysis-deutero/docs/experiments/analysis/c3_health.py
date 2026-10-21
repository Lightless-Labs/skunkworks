"""Pre-launch API health probe for the confirmation-3 queue lines (operational; not the registered runner).

Sol "model at capacity" failures clustered on 2026-10-21 (LOG). Each failed judge call voids an X tuple
and counts 2 tool events under the registered rule, so the queue lines run this probe before invoking
`nano/confirm3.py generate`: on any failure they wait and retry later in the same UTC day (the
registration allows any start at or after 00:05 UTC). It uses API health only, never outcome data.

Standalone PONG calls with the provider's exact arguments (the disabled-feature list is read from the
frozen crates/a2d-providers/src/cli.rs) and an isolated auth-only CODEX_HOME in an empty temp cwd:
2 on codex/gpt-6-luna low and 3 on codex/gpt-6.1-sol medium. Healthy means every call exits 0 and its
event stream has turn.completed, no top-level error event and no turn.failed. Prints labels only.

usage: python3 -I docs/experiments/analysis/c3_health.py     (exit 0 healthy, 3 unhealthy)
"""
import json
import os
import pathlib
import re
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[3]
LANES = (("gpt-6-luna", "low", 2), ("gpt-6.1-sol", "medium", 3))
PROMPT = "Reply with exactly one word.\n\nReply with the single word PONG."


def features():
    src = (ROOT / "crates" / "a2d-providers" / "src" / "cli.rs").read_text()
    block = re.search(r"CODEX_DISABLED_FEATURES: &\[&str\] = &\[(.*?)\];", src, re.S).group(1)
    return re.findall(r'"([a-z0-9_]+)"', block)


def probe(model, effort):
    with tempfile.TemporaryDirectory(prefix="a2d-health-") as d:
        d = pathlib.Path(d)
        (d / "cwd").mkdir()
        (d / "home").mkdir()
        os.symlink(pathlib.Path.home() / ".codex" / "auth.json", d / "home" / "auth.json")
        args = ["codex", "exec", PROMPT, "-m", model, "-c", f'model_reasoning_effort="{effort}"', "--sandbox",
                "read-only", "--skip-git-repo-check", "--ephemeral", "--ignore-user-config", "--ignore-rules"]
        for f in features():
            args += ["--disable", f]
        args += ["-c", 'web_search="disabled"', "--json", "--output-last-message", "last.txt"]
        try:
            r = subprocess.run(args, cwd=d / "cwd", env={**os.environ, "CODEX_HOME": str(d / "home")},
                               capture_output=True, text=True, timeout=180, stdin=subprocess.DEVNULL)
        except subprocess.TimeoutExpired:
            return "timeout"
        kinds = set()
        for line in r.stdout.split("\n"):
            try:
                kinds.add(json.loads(line).get("type"))
            except (ValueError, AttributeError):
                continue
        if "error" in kinds or "turn.failed" in kinds:
            return "api-error"
        if r.returncode or "turn.completed" not in kinds:
            return "incomplete"
        return "ok"


def main():
    results = {f"{m}:{e}": [probe(m, e) for _ in range(n)] for m, e, n in LANES}
    healthy = all(x == "ok" for v in results.values() for x in v)
    print(json.dumps({"healthy": healthy, "results": results}), flush=True)
    sys.exit(0 if healthy else 3)


if __name__ == "__main__":
    main()
