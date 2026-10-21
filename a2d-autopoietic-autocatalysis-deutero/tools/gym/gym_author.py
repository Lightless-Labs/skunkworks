"""Gym v3 authoring driver (copy of tools/holdout/c3_author.py, adapted; plan 2026-10-21).

One Sol (high) job per active family in <gym root>/authoring/gym-v3/<id>/, inside the outer
sandbox, at most --parallel (<= 3) at once; 2 attempts per family, reserved before launch. A
family is done only when the driver has verified its output from the files: exit 0, zero
non-allowlisted events, no suspicious command, and exactly 4 complete task dirs (task.json,
tests, reference, m1..m4, rubric.json) with the opaque ids. Every launch passes the gym launch
gate (confirmation-3 isolation). Prints only gateway lines.

  --stop        request a stop; nothing new launches once STOP exists (never gated)
  --drained     drain check (STOP, no live driver, no live job process group; never gated)
  --reauthor ID re-author a family flagged by gym_similarity.py (consumes its 2nd attempt)
  --set dev|holdout [--wave 1|2 [--count N]]  author only that split slot (split.json)

usage: python3 -I tools/gym/gym_author.py [--parallel N] [--only g3-fNNN] [--set S --wave W] | --stop
       | --drained | --reauthor g3-fNNN
"""
import json
import shutil
import sys
import threading
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from gymlib import (AUTHORING_FILES, GYM, ID_RE, RECORDS, TASK_RE, DriverLock, LaunchGateClosed,  # noqa: E402
                    StopRequested, drained, emit, gate_or_exit, gateway, isolation_check, job_codex_home,
                    load_json, mutants_ok, prereqs_ok, request_stop, run_codex, safe_copy, safe_read_text,
                    safe_write_text, save_json, stop_requested, suspicious_commands, tree_hashes)

MAX_ATTEMPTS = 2
CONTRACT = ("task.json", "public_tests.rs", "hidden_tests.rs", "reference.rs", "rubric.json")
_lock = threading.Lock()


def active():
    act = load_json(RECORDS / "active.json")
    if not act or not all(ID_RE.match(i) for i in act["active"]):
        raise ValueError("no active list")
    return act


def complete(fid):
    """Verify the family's output from the files themselves (symlinks refused)."""
    base = GYM / fid / "out" / fid
    try:
        for lv in (1, 2, 3, 4):
            tid = f"{fid}-l{lv}"
            d = base / tid
            for f in CONTRACT:
                if not safe_read_text(d / f, GYM).strip():
                    return False
            if not mutants_ok(d / "mutants"):
                return False
            for i in (1, 2, 3, 4):
                safe_read_text(d / "mutants" / f"m{i}.rs", GYM)
            tj = json.loads(safe_read_text(d / "task.json", GYM))
            if tj.get("id") != tid or tj.get("family") != fid or not TASK_RE.match(tid):
                return False
            if not isinstance(json.loads(safe_read_text(d / "rubric.json", GYM)), dict):
                return False
        safe_read_text(base / "REPORT.md", GYM)
        return True
    except (OSError, PermissionError, ValueError):
        return False


def status(fid):
    return load_json(RECORDS / f"author-{fid}" / "status.json", {"attempts": 0, "done": False})


def prepare(fid, cls, reauthor=False):
    w = GYM / fid
    w.mkdir(parents=True, exist_ok=True)
    job_codex_home(w)
    for f in ("SPEC.md", "check.sh", "api_check.sh"):
        safe_copy(AUTHORING_FILES / f, w / f, AUTHORING_FILES, mode=0o755 if f.endswith(".sh") else None)
    safe_copy(GYM / "_select" / "families" / fid / "FAMILY.md", w / "FAMILY.md", GYM)
    text = (AUTHORING_FILES / "AUTHOR.md").read_text().replace("{FAMILY_ID}", fid).replace("{FAMILY_CLASS}", cls)
    if reauthor:
        if (w / "out").exists():
            shutil.rmtree(w / "out")
        text += ("\n5. A previous version of this family was too similar to an existing benchmark task. "
                 "Write it afresh: keep the brief's domain but choose different concrete rules, formats, "
                 "data and API details.\n")
    safe_write_text(w / "INSTRUCTIONS.md", text)
    return w


def one(fid, cls, reauthor=False):
    with _lock:
        if stop_requested():
            emit(stage="author", id=fid, status="stopped")
            return
        if status(fid)["attempts"] >= MAX_ATTEMPTS:
            emit(stage="author", id=fid, status="cap-exhausted")
            return
    w = prepare(fid, cls, reauthor)
    if not isolation_check(w):
        with _lock:
            emit(stage="author-isolation", id=fid, ok=False)
        return
    with _lock:
        if stop_requested():
            emit(stage="author", id=fid, status="stopped")
            return
        st = status(fid)
        st["attempts"] += 1
        st["done"] = False
        save_json(RECORDS / f"author-{fid}" / "status.json", st)
    rec = f"author-{fid}-a{st['attempts']}"
    try:
        code, other = run_codex(w, rec, effort="high", timeout=7200)
    except (StopRequested, LaunchGateClosed) as e:
        with _lock:
            st["attempts"] -= 1
            save_json(RECORDS / f"author-{fid}" / "status.json", st)
            emit(stage="author", id=fid, status="stopped" if isinstance(e, StopRequested) else "launch-gate-closed")
        return
    susp = suspicious_commands(rec, w)
    ok = code == 0 and other == 0 and susp == 0 and complete(fid)
    with _lock:
        st["done"] = ok
        st["hashes"] = tree_hashes(GYM / fid / "out" / fid) if ok else None
        save_json(RECORDS / f"author-{fid}" / "status.json", st)
        emit(stage="author", id=fid, exit=code, other_events=other, skipped=susp, ok=ok, round=st["attempts"])


@gateway("author")
def main():
    args = sys.argv[1:]
    if args[:1] == ["--stop"]:
        request_stop()
        emit(stage="author", status="stop-requested")
        return
    if args[:1] == ["--drained"]:
        emit(stage="author", ok=drained())
        return
    gate_or_exit("author")
    if not prereqs_ok():
        emit(stage="author", status="prereqs-not-met")
        sys.exit(2)
    with DriverLock():
        act = active()
        if args[:1] == ["--reauthor"]:
            fid = args[1]
            sim = load_json(RECORDS / "similarity.json", {})
            if not ID_RE.match(fid) or sim.get(fid) != "reauthor":
                raise ValueError("reauthor not flagged")
            one(fid, act["classes"][fid], reauthor=True)
            st = status(fid)
            if st["done"]:
                sim[fid] = "reauthor-done"
            elif st["attempts"] >= MAX_ATTEMPTS:
                sim[fid] = "reauthor-failed"
            save_json(RECORDS / "similarity.json", sim)
            return
        parallel = int(args[args.index("--parallel") + 1]) if "--parallel" in args else 3
        if not 1 <= parallel <= 3:
            raise ValueError("parallel")
        ids = list(act["active"])
        if "--set" in args:
            from gym_split import families_for, load_split
            sp = load_split()
            if not sp:
                raise ValueError("no split")
            wave = int(args[args.index("--wave") + 1]) if "--wave" in args else None
            count = int(args[args.index("--count") + 1]) if "--count" in args else None
            ids = families_for(sp, act, args[args.index("--set") + 1], wave, count)
        if "--only" in args:
            ids = [i for i in ids if i == args[args.index("--only") + 1]]
        pending = [i for i in ids if not status(i)["done"]]
        emit(stage="author", families=len(ids), pending=len(pending), done=len(ids) - len(pending))
        with ThreadPoolExecutor(max_workers=parallel) as ex:
            list(ex.map(lambda f: one(f, act["classes"][f]), pending))
        emit(stage="author", done=sum(status(i)["done"] for i in ids), families=len(ids))


if __name__ == "__main__":
    main()
