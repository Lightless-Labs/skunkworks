"""Confirmation-3 authoring driver (plan §0.6 step 4; caps frozen in §0.11).

One Sol (high) job per active family in $A2D_HOLDOUT_DIR/authoring/confirmation-3/<id>/,
inside the outer sandbox, at most --parallel (≤ 3) at once. Caps: 2 authoring attempts per
family (an attempt is reserved in the records before launch; a re-author after a
similarity conflict uses the second). A family is done only when the driver has verified
its output: exit 0, zero non-allowlisted events, and exactly 4 complete task dirs with the
opaque ids. Prints only gateway lines.

  --stop        request a stop; nothing new launches once STOP exists
  --drained     §0.10 drain check (STOP, no live driver, no live job process group)
  --reauthor ID re-author a family flagged by c3_similarity.py (consumes its 2nd attempt)

usage: python3 -I tools/holdout/c3_author.py [--parallel N] [--only c3-fNN] | --stop | --drained | --reauthor c3-fNN
"""
import json
import shutil
import sys
import threading
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from c3lib import (AUTHORING_FILES, C3, ID_RE, RECORDS, TASK_RE, DriverLock, StopRequested, drained,  # noqa: E402
                   emit, gateway, isolation_check, load_json, prereqs_ok, request_stop, run_codex, safe_copy,
                   safe_read_text, safe_write_text, save_json, stop_requested, suspicious_commands,
                   tree_hashes)

MAX_ATTEMPTS = 2
CONTRACT = ("task.json", "public_tests.rs", "hidden_tests.rs", "reference.rs")
_lock = threading.Lock()


def active_ids():
    act = load_json(RECORDS / "active.json")
    if not act or not all(ID_RE.match(i) for i in act["active"]):
        raise ValueError("no active list")
    return act["active"]


def complete(fid):
    """Verify the family's output from the files themselves (symlinks refused)."""
    base = C3 / fid / "out" / fid
    try:
        for lv in (1, 2, 3, 4):
            tid = f"{fid}-l{lv}"
            d = base / tid
            for f in CONTRACT:
                if not safe_read_text(d / f, C3).strip():
                    return False
            if sorted(p.name for p in (d / "mutants").iterdir()) != [f"m{i}.rs" for i in (1, 2, 3, 4)]:
                return False
            for i in (1, 2, 3, 4):
                safe_read_text(d / "mutants" / f"m{i}.rs", C3)
            tj = json.loads(safe_read_text(d / "task.json", C3))
            if tj.get("id") != tid or tj.get("family") != fid or not TASK_RE.match(tid):
                return False
        safe_read_text(base / "REPORT.md", C3)
        return True
    except (OSError, PermissionError, ValueError):
        return False


def status(fid):
    return load_json(RECORDS / f"author-{fid}" / "status.json", {"attempts": 0, "done": False})


def prepare(fid, reauthor=False):
    w = C3 / fid
    w.mkdir(parents=True, exist_ok=True)
    for f in ("SPEC.md", "check.sh", "api_check.sh"):
        safe_copy(AUTHORING_FILES / f, w / f, AUTHORING_FILES, mode=0o755 if f.endswith(".sh") else None)
    safe_copy(C3 / "_select" / "families" / fid / "FAMILY.md", w / "FAMILY.md", C3)
    text = (AUTHORING_FILES / "AUTHOR.md").read_text().replace("{FAMILY_ID}", fid)
    if reauthor:
        if (w / "out").exists():
            shutil.rmtree(w / "out")
        text += ("\n5. A previous version of this family was too similar to an existing benchmark task. "
                 "Write it afresh: keep the brief's domain but choose different concrete rules, formats, "
                 "data and API details.\n")
    safe_write_text(w / "INSTRUCTIONS.md", text)
    return w


def one(fid, reauthor=False):
    with _lock:
        if stop_requested():
            emit(stage="author", id=fid, status="stopped")
            return
        if status(fid)["attempts"] >= MAX_ATTEMPTS:
            emit(stage="author", id=fid, status="cap-exhausted")
            return
    w = prepare(fid, reauthor)
    if not isolation_check(w):
        with _lock:
            emit(stage="author-isolation", id=fid, ok=False)
        return
    with _lock:
        if stop_requested():
            emit(stage="author", id=fid, status="stopped")
            return
        st = status(fid)
        st["attempts"] += 1           # reserved before launch
        st["done"] = False
        save_json(RECORDS / f"author-{fid}" / "status.json", st)
    rec = f"author-{fid}-a{st['attempts']}"
    try:
        code, other = run_codex(w, rec, effort="high", timeout=7200)
    except StopRequested:             # STOP won the launch gate: nothing ran, release the attempt
        with _lock:
            st["attempts"] -= 1
            save_json(RECORDS / f"author-{fid}" / "status.json", st)
            emit(stage="author", id=fid, status="stopped")
        return
    susp = suspicious_commands(rec, w)
    ok = code == 0 and other == 0 and susp == 0 and complete(fid)
    with _lock:
        st["done"] = ok
        st["hashes"] = tree_hashes(C3 / fid / "out" / fid) if ok else None
        save_json(RECORDS / f"author-{fid}" / "status.json", st)
        emit(stage="author", id=fid, exit=code, other_events=other, skipped=susp, ok=ok, round=st["attempts"])


@gateway("author")
def main():
    args = sys.argv[1:]
    if args[:1] == ["--stop"]:
        request_stop()                 # through the launch gate: no job starts after this
        emit(stage="author", status="stop-requested")
        return
    if args[:1] == ["--drained"]:
        emit(stage="author", ok=drained())
        return
    if not prereqs_ok():
        emit(stage="author", status="prereqs-not-met")
        sys.exit(2)
    with DriverLock():
        if args[:1] == ["--reauthor"]:
            fid = args[1]
            sim = load_json(RECORDS / "similarity.json", {})
            if not ID_RE.match(fid) or sim.get(fid) != "reauthor":
                raise ValueError("reauthor not flagged")
            one(fid, reauthor=True)
            st = status(fid)
            if st["done"]:            # cleared only by the similarity re-check on the re-assembled suite
                sim[fid] = "reauthor-done"
            elif st["attempts"] >= MAX_ATTEMPTS:
                sim[fid] = "reauthor-failed"   # terminal: the assembly replaces the family
            save_json(RECORDS / "similarity.json", sim)
            return
        parallel = int(args[args.index("--parallel") + 1]) if "--parallel" in args else 3
        if not 1 <= parallel <= 3:
            raise ValueError("parallel")
        ids = active_ids()
        if "--only" in args:
            ids = [i for i in ids if i == args[args.index("--only") + 1]]
        pending = [i for i in ids if not status(i)["done"]]
        emit(stage="author", families=len(ids), pending=len(pending), done=len(ids) - len(pending))
        with ThreadPoolExecutor(max_workers=parallel) as ex:
            list(ex.map(one, pending))
        emit(stage="author", done=sum(status(i)["done"] for i in ids), families=len(ids))


if __name__ == "__main__":
    main()
