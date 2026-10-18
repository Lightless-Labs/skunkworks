"""Confirmation-3 authoring driver (plan §0.6 step 4).

One Sol (high) job per family in $A2D_HOLDOUT_DIR/authoring/confirmation-3/<id>/,
each inside the outer sandbox, at most --parallel jobs at once (default 3). Resumable:
a family whose exit.txt is 0 is skipped. Prints only gateway lines (opaque id, exit
code, count of non-allowlisted events). Per-family cap: one authoring job (repairs are
c3_fix.py, at most 2 rounds per task).

Draining: `--stop` creates STOP; the driver launches nothing new once STOP exists and
returns when its running jobs finish. `--drained` reports whether every recorded pid
has exited (kill -0).

usage: python3 -I tools/holdout/c3_author.py [--parallel N] [--only c3-fNN] | --stop | --drained
"""
import shutil
import sys
import threading
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from c3lib import (AUTHORING_FILES, C3, drained, emit, gateway, isolation_check, read_ids,  # noqa: E402
                   run_codex)

PIDS = C3 / "pids.txt"
STOP = C3 / "STOP"
_lock = threading.Lock()


def prepare(fid):
    w = C3 / fid
    w.mkdir(parents=True, exist_ok=True)
    for f in ("SPEC.md", "check.sh", "api_check.sh"):
        shutil.copy(AUTHORING_FILES / f, w / f)
        if f.endswith(".sh"):
            (w / f).chmod(0o755)
    shutil.copy(C3 / "_select" / "families" / fid / "FAMILY.md", w / "FAMILY.md")
    (w / "INSTRUCTIONS.md").write_text((AUTHORING_FILES / "AUTHOR.md").read_text().replace("{FAMILY_ID}", fid))
    return w


def done(fid):
    ex = C3 / fid / "exit.txt"
    return ex.exists() and ex.read_text().strip() == "0" and (C3 / fid / "out" / fid).is_dir()


def one(fid):
    if STOP.exists():
        with _lock:
            emit(stage="author", id=fid, status="skipped")
        return
    w = prepare(fid)
    ok = isolation_check(w)
    if not ok:
        with _lock:
            emit(stage="author-isolation", id=fid, ok=False)
        return
    code, other = run_codex(w, effort="high", timeout=7200, pidfile=PIDS)
    with _lock:
        emit(stage="author", id=fid, exit=code, other_events=other)


@gateway("author")
def main():
    args = sys.argv[1:]
    if args[:1] == ["--stop"]:
        STOP.write_text("stop\n")
        emit(stage="author", status="stop-requested")
        return
    if args[:1] == ["--drained"]:
        emit(stage="author", ok=drained(PIDS))
        return
    parallel = 3
    only = None
    if "--parallel" in args:
        parallel = int(args[args.index("--parallel") + 1])
    if "--only" in args:
        only = args[args.index("--only") + 1]
    if parallel < 1 or parallel > 3:
        raise ValueError("parallel")
    ids = read_ids(C3 / "FAMILIES.txt")
    if only:
        ids = [i for i in ids if i == only]
    pending = [i for i in ids if not done(i)]
    emit(stage="author", families=len(ids), pending=len(pending), done=len(ids) - len(pending))
    with ThreadPoolExecutor(max_workers=parallel) as ex:
        list(ex.map(one, pending))
    emit(stage="author", done=sum(done(i) for i in ids), families=len(ids))


if __name__ == "__main__":
    main()
