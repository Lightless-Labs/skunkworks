"""Validate the assembled confirmation-3 suite and route rejections to blind repair (§0.6 step 5).

Runs `a2d lift validate` on $A2D_HOLDOUT_DIR/confirmation-3 (reference and mutants are
scored by the controller's sandboxed oracle; its API-surface check compiles without running,
disclosed). The full output goes to records/validate-<stamp>.txt (never printed). The parse
is strict: every suite task must get exactly one OK or REJECT line, or nothing changes.
Then each family with rejected tasks gets FIX-VERDICTS.txt (its own REJECT lines only) and
records/validate.json stores the stamp and the rejected set; tasks still rejected after a
family's 2nd repair round are written to records/excluded.json (the assembly then replaces
or drops the family, §0.11). Prints only counts and opaque task ids.

usage: python3 -I tools/holdout/c3_validate.py [--binary <a2d>]
"""
import hashlib
import re
import subprocess
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from c3lib import (C3, RECORDS, REPO, SUITE, TASK_RE, DriverLock, emit, gateway, load_json,  # noqa: E402
                   safe_write_text, save_json)

LINE = re.compile(r"^(c3-f\d{2}-l[1-4]): (OK|REJECT)\b")
SUMMARY = re.compile(r"^suite [0-9a-f]{64} \u2014 (\d+) tasks, (\d+) accepted, (\d+) rejected$")
MAX_FIX_ROUNDS = 2


@gateway("validate")
def main():
    args = sys.argv[1:]
    binary = args[args.index("--binary") + 1] if "--binary" in args else str(REPO / "target" / "debug" / "a2d")
    with DriverLock():
        expected = sorted(p.name for p in SUITE.iterdir() if TASK_RE.match(p.name))
        if not expected:
            raise ValueError("empty suite")
        r = subprocess.run([binary, "lift", "validate", str(SUITE), "--jobs", "4"], capture_output=True, text=True)
        stamp = time.strftime("%Y%m%dT%H%M%SZ", time.gmtime())
        full = r.stdout + "\n--- stderr ---\n" + r.stderr
        safe_write_text(RECORDS / f"validate-{stamp}.txt", full)
        sha = hashlib.sha256(full.encode()).hexdigest()
        summary = [SUMMARY.match(x) for x in r.stdout.splitlines() if SUMMARY.match(x)]
        verdict, lines, unparsed = {}, {}, 0
        for line in r.stdout.splitlines():
            m = LINE.match(line)
            if not m:
                if line.startswith("c3-f"):
                    unparsed += 1
                continue
            if m.group(1) in verdict:
                raise ValueError("duplicate verdict")
            verdict[m.group(1)] = m.group(2)
            lines[m.group(1)] = line
        ok_summary = len(summary) == 1 and int(summary[0].group(1)) == len(expected) \
            and int(summary[0].group(2)) + int(summary[0].group(3)) == len(expected) \
            and int(summary[0].group(3)) == sum(v == "REJECT" for v in verdict.values())
        if sorted(verdict) != expected or unparsed or not ok_summary:
            emit(stage="validate", status="incomplete-parse", tasks=len(expected), count=len(verdict),
                 unparsed=unparsed)
            sys.exit(1)
        rejected = sorted(t for t, v in verdict.items() if v == "REJECT")
        per_family = {}
        for t in rejected:
            per_family.setdefault(t.rsplit("-l", 1)[0], []).append(lines[t])
        excluded = set(load_json(RECORDS / "excluded.json", {"tasks": []})["tasks"])
        for fam_dir in C3.glob("c3-f*"):
            fam = fam_dir.name
            target = fam_dir / "FIX-VERDICTS.txt"
            rounds = load_json(RECORDS / f"fix-{fam}" / "rounds.json", {"rounds": 0})["rounds"]
            if fam in per_family and rounds >= MAX_FIX_ROUNDS:
                excluded.update(t for t in rejected if t.startswith(fam + "-"))
            if fam in per_family and rounds < MAX_FIX_ROUNDS:
                safe_write_text(target, "\n".join(per_family[fam]) + "\n")
            elif target.exists() or target.is_symlink():
                target.unlink()
        save_json(RECORDS / "validate.json", {"stamp": stamp, "sha256": sha, "rejected": rejected,
                                              "exit": r.returncode})
        save_json(RECORDS / "excluded.json", {"tasks": sorted(excluded)})
        emit(stage="validate", exit=r.returncode, tasks=len(expected), count=len(expected) - len(rejected),
             missing=len(rejected), skipped=len(excluded))
        for t in rejected:
            emit(stage="validate-reject", id=t)


if __name__ == "__main__":
    main()
