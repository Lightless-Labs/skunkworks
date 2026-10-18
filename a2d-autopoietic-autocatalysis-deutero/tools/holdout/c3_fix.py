"""Blind repair of rejected confirmation-3 tasks (plan §0.10/§0.11: at most 2 rounds per family).

For each active family with FIX-VERDICTS.txt from the LATEST validation (records/
validate.json), fewer than 2 rounds used, and no repair yet bound to that validation stamp,
reserve a round in the records, then run one Sol (high) FIX job in the family dir inside
the outer sandbox. Re-assemble and re-validate between rounds. Prints only opaque ids,
exit codes and round numbers.

usage: python3 -I tools/holdout/c3_fix.py [--parallel N]
"""
import sys
import threading
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from c3lib import (AUTHORING_FILES, C3, RECORDS, DriverLock, emit, gateway, isolation_check,  # noqa: E402
                   load_json, prereqs_ok, run_codex, safe_write_text, save_json, stop_requested)

MAX_ROUNDS = 2
_lock = threading.Lock()


def one(fam, stamp):
    with _lock:
        if stop_requested():
            emit(stage="fix", id=fam, status="stopped")
            return
        rec = load_json(RECORDS / f"fix-{fam}" / "rounds.json", {"rounds": 0, "stamps": []})
        if rec["rounds"] >= MAX_ROUNDS or stamp in rec["stamps"]:
            emit(stage="fix", id=fam, status="skipped", round=rec["rounds"])
            return
    w = C3 / fam
    safe_write_text(w / "INSTRUCTIONS.md", (AUTHORING_FILES / "FIX.md").read_text())
    if not isolation_check(w):
        with _lock:
            emit(stage="fix-isolation", id=fam, ok=False)
        return
    with _lock:
        rec["rounds"] += 1                      # reserved before launch
        rec["stamps"].append(stamp)
        save_json(RECORDS / f"fix-{fam}" / "rounds.json", rec)
    code, other = run_codex(w, f"fix-{fam}-r{rec['rounds']}", effort="high", timeout=5400)
    with _lock:
        emit(stage="fix", id=fam, exit=code, other_events=other, round=rec["rounds"])


@gateway("fix")
def main():
    args = sys.argv[1:]
    parallel = int(args[args.index("--parallel") + 1]) if "--parallel" in args else 3
    if not 1 <= parallel <= 3:
        raise ValueError("parallel")
    if not prereqs_ok():
        emit(stage="fix", status="prereqs-not-met")
        sys.exit(2)
    with DriverLock():
        val = load_json(RECORDS / "validate.json")
        if not val:
            raise ValueError("no validation")
        active = set(load_json(RECORDS / "active.json")["active"])
        fams = sorted(f.parent.name for f in C3.glob("c3-f*/FIX-VERDICTS.txt") if f.parent.name in active)
        emit(stage="fix", families=len(fams))
        with ThreadPoolExecutor(max_workers=parallel) as ex:
            list(ex.map(lambda f: one(f, val["stamp"]), fams))


if __name__ == "__main__":
    main()
