#!/usr/bin/python3 -I
"""Fake `a2d lift` for the confirmation-3 tests. `validate` accepts every task; `run` (arm D)
writes a ledger, submissions and hidden output. Env hooks: C3_TEST_AUDIT_FAIL / C3_TEST_AUDIT_VOID
(space-separated task ids)."""
import json
import os
import sys
from pathlib import Path

args = sys.argv[1:]
suite = Path(args[2]) if args[:2] == ["lift", "validate"] else Path(args[args.index("--suite") + 1])
tasks = sorted(p.name for p in suite.iterdir() if p.name.startswith("c3-f"))
if args[:2] == ["lift", "validate"]:
    for t in tasks:
        print(f"{t}: OK (fake)")
    print(f"suite {'0' * 64} — {len(tasks)} tasks, {len(tasks)} accepted, 0 rejected")
    sys.exit(0)
out = Path(args[args.index("--out") + 1])
fail = set(os.environ.get("C3_TEST_AUDIT_FAIL", "").split())
void = set(os.environ.get("C3_TEST_AUDIT_VOID", "").split())
out.mkdir(parents=True)
lines = [{"kind": "registration"}]
for t in tasks:
    d = out / "submissions" / "D" / t
    d.mkdir(parents=True)
    (d / "r0.rs").write_text("// SECRET submission\n")
    lines.append({"kind": "submission", "arm": "D", "task": t, "void_reason": "provider" if t in void else None})
    if t in void:
        continue
    (d / "r0.hidden.txt").write_text("test oracle::hidden_a ... ok\n"
                                     + ("test oracle::hidden_b ... FAILED\n" if t in fail else ""))
    lines.append({"kind": "score", "arm": "D", "task": t, "result": {"all_passed": t not in fail}})
(out / "ledger.jsonl").write_text("".join(json.dumps(x) + "\n" for x in lines))
print("SECRET run output")
