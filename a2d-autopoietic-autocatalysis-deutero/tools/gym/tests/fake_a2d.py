#!/usr/bin/python3 -I
"""Fake `a2d lift run` for the gym v3 tests (never calls a model). Writes a ledger in the
controller's shape: registration, one invocation per (arm, task) with a --json transcript that
the vendored detector accepts (host-off notice, allowlisted items, turn.completed), a submission
and a score; with --record-hidden-output, submissions/<arm>/<task>/r0.rs and r0.hidden.txt.
Env hooks (space-separated task ids): GYM_TEST_AUDIT_FAIL, GYM_TEST_AUDIT_VOID,
GYM_TEST_AUDIT_NOCOMPILE, GYM_TEST_PILOT_FAIL, GYM_TEST_PILOT_VOID; GYM_TEST_ENV_DUMP=<file>
records which isolation-relevant variables the child saw; GYM_TEST_LEAK=1 writes a hidden test
name into a submission (seal-breach test); GYM_TEST_A2D_TOUCH=<file> appends to that file (a codex
package file changing during the run); GYM_TEST_WAIT_ACCOUNTING=<repo ledger> keeps the run alive
until the driver has published every invocation there (got/want written to GYM_TEST_WAIT_RESULT)."""
import json
import os
import re
import sys
import time
from pathlib import Path

NOTICE = ("Code Mode is unavailable because code-mode host is disabled. Code mode will fail closed; "
          "enable `features.code_mode_host` and install `codex-code-mode-host`.")
MARKER = "\n--- codex stderr ---\n"

args = sys.argv[1:]
if args[:2] != ["lift", "run"]:
    sys.exit(2)


def opt(name):
    return args[args.index(name) + 1]


suite, out = Path(opt("--suite")), Path(opt("--out"))
arms = opt("--arms").split(",")
model = opt("--model")
record_hidden = "--record-hidden-output" in args
dump = os.environ.get("GYM_TEST_ENV_DUMP")
if dump:
    keys = sorted(k for k in os.environ if k in ("A2D_HOLDOUT_DIR", "A2D_CODEX_HOME_ROOT", "CARGO_TARGET_DIR")
                  or k.startswith("C3_"))
    Path(dump).write_text(json.dumps({k: os.environ[k] for k in keys}))
tasks = sorted(p.name for p in suite.iterdir() if (p / "task.json").exists())
if os.environ.get("GYM_TEST_A2D_TOUCH"):
    with open(os.environ["GYM_TEST_A2D_TOUCH"], "a") as f:
        f.write("changed\n")
pilot = "X:inc" in arms
fail = set(os.environ.get("GYM_TEST_PILOT_FAIL" if pilot else "GYM_TEST_AUDIT_FAIL", "").split())
void = set(os.environ.get("GYM_TEST_PILOT_VOID" if pilot else "GYM_TEST_AUDIT_VOID", "").split())
nocompile = set(os.environ.get("GYM_TEST_AUDIT_NOCOMPILE", "").split())
out.mkdir(parents=True)
now = int(time.time() * 1000)
lines = [{"kind": "registration", "arms": arms, "tasks": tasks}]
for arm in arms:
    for t in tasks:
        tj = json.loads((suite / t / "task.json").read_text())
        rel = Path("invocations") / arm.replace(":", "_") / t / "r0-i0"
        (out / rel.parent).mkdir(parents=True, exist_ok=True)
        (out / f"{rel}.prompt.txt").write_text(tj["requirements"] + "\n" + (suite / t / "public_tests.rs").read_text())
        events = [{"type": "thread.started"}, {"type": "item.completed", "item": {"type": "error", "message": NOTICE}},
                  {"type": "turn.started"}, {"type": "item.completed", "item": {"type": "agent_message", "text": "x"}},
                  {"type": "turn.completed"}]
        (out / f"{rel}.transcript.txt").write_text("".join(json.dumps(e) + "\n" for e in events) + MARKER + "stderr\n")
        lines.append({"kind": "invocation", "arm": arm, "task": t, "provider": model, "ok": True, "tool_calls": 0,
                      "prompt_path": str(out / f"{rel}.prompt.txt"), "ts_ms": now, "duration_ms": 1000})
        sub = out / "submissions" / arm / t
        sub.mkdir(parents=True)
        body = "// SECRET submission\n"
        if os.environ.get("GYM_TEST_LEAK"):
            body += "// " + re.findall(r"fn (h_[0-9a-f]+)\(", (suite / t / "hidden_tests.rs").read_text())[0] + "\n"
        (sub / "r0.rs").write_text(body)
        lines.append({"kind": "submission", "arm": arm, "task": t, "invocations": 1, "tool_calls": 0,
                      "void_reason": "provider" if t in void else None})
        if t in void:
            continue
        names = re.findall(r"fn (h_[0-9a-f]+)\(", (suite / t / "hidden_tests.rs").read_text())
        if record_hidden:
            if t in nocompile:
                text = "error[E0425]: cannot find value\n"
            else:
                text = "".join(f"test a2d_hidden_0123456789abcdef::{n} ... "
                               f"{'FAILED' if t in fail and i == 0 else 'ok'}\n" for i, n in enumerate(names))
            (sub / "r0.hidden.txt").write_text(text)
        passed = t not in fail and t not in nocompile
        lines.append({"kind": "score", "arm": arm, "task": t, "family": tj["family"],
                      "result": {"all_passed": passed, "compiled": t not in nocompile,
                                 "passed": len(names) - (0 if passed else 1), "expected": len(names)}})
wait = os.environ.get("GYM_TEST_WAIT_ACCOUNTING")
if wait:
    # in-flight visibility: write everything but the last record, then wait (up to 30 s) until the
    # driver has published every invocation to the repo's accounting ledger while this run is alive
    (out / "ledger.jsonl").write_text("".join(json.dumps(x) + "\n" for x in lines))
    want, got, end = sum(1 for x in lines if x["kind"] == "invocation"), 0, time.time() + 30
    while time.time() < end:
        try:
            got = sum(1 for x in Path(wait).read_text().split("\n") if '"invocation"' in x)
        except FileNotFoundError:
            got = 0
        if got >= want:
            break
        time.sleep(0.2)
    Path(os.environ["GYM_TEST_WAIT_RESULT"]).write_text(f"{got}/{want}")
    with open(out / "ledger.jsonl", "a") as f:
        f.write(json.dumps({"kind": "generation_complete", "aborted": False, "ts_ms": now}) + "\n")
else:
    lines.append({"kind": "generation_complete", "aborted": False, "ts_ms": now})
    (out / "ledger.jsonl").write_text("".join(json.dumps(x) + "\n" for x in lines))
(out / "summary.json").write_text(json.dumps({"tasks": len(tasks)}))
print("SECRET run output")
