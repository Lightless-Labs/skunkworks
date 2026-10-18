"""Blind repair of rejected confirmation-3 tasks (plan §0.10: at most 2 repair rounds per family).

For each family with FIX-VERDICTS.txt (written by c3_validate.py) and fewer than 2
completed repair rounds, run one Sol (high) FIX job in the family dir inside the outer
sandbox. Prints only opaque ids, exit codes and round numbers. After 2 rounds, still
rejected tasks are excluded before the hash (c3_exclude in the registration step).

usage: python3 -I tools/holdout/c3_fix.py [--parallel N]
"""
import json
import sys
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from c3lib import AUTHORING_FILES, C3, emit, gateway, isolation_check, run_codex  # noqa: E402

MAX_ROUNDS = 2


def rounds(fam):
    f = fam / "fix_rounds.json"
    return json.loads(f.read_text())["rounds"] if f.exists() else 0


def one(fam):
    r = rounds(fam)
    if r >= MAX_ROUNDS:
        emit(stage="fix", id=fam.name, status="cap-exhausted", round=r)
        return
    (fam / "INSTRUCTIONS.md").write_text((AUTHORING_FILES / "FIX.md").read_text())
    if not isolation_check(fam):
        emit(stage="fix-isolation", id=fam.name, ok=False)
        return
    code, other = run_codex(fam, effort="high", timeout=5400, pidfile=C3 / "pids.txt")
    (fam / "fix_rounds.json").write_text(json.dumps({"rounds": r + 1}) + "\n")
    emit(stage="fix", id=fam.name, exit=code, other_events=other, round=r + 1)


@gateway("fix")
def main():
    args = sys.argv[1:]
    parallel = int(args[args.index("--parallel") + 1]) if "--parallel" in args else 3
    if not 1 <= parallel <= 3:
        raise ValueError("parallel")
    fams = sorted(f.parent for f in C3.glob("c3-f*/FIX-VERDICTS.txt"))
    emit(stage="fix", families=len(fams))
    with ThreadPoolExecutor(max_workers=parallel) as ex:
        list(ex.map(one, fams))


if __name__ == "__main__":
    main()
