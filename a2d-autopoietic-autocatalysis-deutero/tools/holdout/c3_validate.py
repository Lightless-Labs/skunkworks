"""Validate the assembled confirmation-3 suite and route rejections to blind repair.

Runs `a2d lift validate` on $A2D_HOLDOUT_DIR/confirmation-3. The full output is saved
under authoring/confirmation-3/ (never printed); each family with rejected tasks gets
FIX-VERDICTS.txt containing only its own `<id>: REJECT ...` lines, for c3_fix.py.
Prints only counts and opaque task ids.

usage: python3 -I tools/holdout/c3_validate.py [--binary target/debug/a2d]
"""
import re
import subprocess
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from c3lib import C3, SUITE, emit, gateway  # noqa: E402

REPO = Path(__file__).resolve().parents[2]
LINE = re.compile(r"^(c3-f\d{2})-l([1-4]): (OK|REJECT)\b")


@gateway("validate")
def main():
    args = sys.argv[1:]
    binary = args[args.index("--binary") + 1] if "--binary" in args else str(REPO / "target" / "debug" / "a2d")
    r = subprocess.run([binary, "lift", "validate", str(SUITE), "--jobs", "4"], capture_output=True, text=True)
    stamp = time.strftime("%Y%m%dT%H%M%SZ", time.gmtime())
    (C3 / f"validate-{stamp}.txt").write_text(r.stdout + "\n--- stderr ---\n" + r.stderr)
    per_family, ok, rej = {}, 0, []
    for line in r.stdout.splitlines():
        m = LINE.match(line)
        if not m:
            continue
        if m.group(3) == "OK":
            ok += 1
        else:
            rej.append(f"{m.group(1)}-l{m.group(2)}")
            per_family.setdefault(m.group(1), []).append(line)
    for fam in C3.glob("c3-f*"):
        f = fam / "FIX-VERDICTS.txt"
        if fam.name in per_family:
            f.write_text("\n".join(per_family[fam.name]) + "\n")
        elif f.exists():
            f.unlink()
    emit(stage="validate", exit=r.returncode, ok=ok == len(list(SUITE.glob("c3-f*-l*"))), tasks=ok + len(rej),
         count=ok, missing=len(rej))
    for t in rej:
        emit(stage="validate-reject", id=t)


if __name__ == "__main__":
    main()
