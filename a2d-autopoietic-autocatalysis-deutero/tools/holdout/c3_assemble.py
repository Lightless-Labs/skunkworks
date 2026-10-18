"""Assemble confirmation-3 contract files into the suite dir (plan §0.6 step 5).

Builds a FRESH staging suite from the active families only, copying (symlinks refused)
exactly task.json, public_tests.rs, hidden_tests.rs, reference.rs and mutants/m1.rs..m4.rs
from authoring/confirmation-3/<id>/out/<id>/<id>-lN/. A family enters the suite only with
all 4 tasks complete and carrying their opaque ids; then the staging dir atomically replaces
$A2D_HOLDOUT_DIR/confirmation-3. Tasks excluded by the validation caps (records/
excluded.json) are left out and their family is replaced or dropped (§0.11).
Prints only counts and opaque ids.

usage: python3 -I tools/holdout/c3_assemble.py
"""
import json
import os
import shutil
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from c3lib import (C3, RECORDS, SUITE, TASK_RE, DriverLock, emit, gateway, load_json,  # noqa: E402
                   replace_family, safe_copy, safe_read_text, tree_hashes)

CONTRACT = ("task.json", "public_tests.rs", "hidden_tests.rs", "reference.rs")
MUTANTS = tuple(f"m{i}.rs" for i in (1, 2, 3, 4))


def copy_task(fid, level, staging):
    tid = f"{fid}-l{level}"
    src = C3 / fid / "out" / fid / tid
    try:
        tj = json.loads(safe_read_text(src / "task.json", C3))
        if tj.get("id") != tid or tj.get("family") != fid or not TASK_RE.match(tid):
            return False
        if sorted(p.name for p in (src / "mutants").iterdir()) != list(MUTANTS):
            return False
        for f in CONTRACT:
            safe_copy(src / f, staging / tid / f, C3, staging)
        for m in MUTANTS:
            safe_copy(src / "mutants" / m, staging / tid / "mutants" / m, C3, staging)
        return True
    except (OSError, PermissionError, ValueError):
        return False


@gateway("assemble")
def main():
    with DriverLock():
        # recover an interrupted swap: the suite is missing but a backup exists
        backups = sorted(SUITE.parent.glob(".confirmation-3.old-*"))
        if not SUITE.exists() and backups:
            os.rename(backups[-1], SUITE)
            backups = backups[:-1]
        for b in backups + sorted(SUITE.parent.glob(".confirmation-3.staging-*")):
            shutil.rmtree(b)
        excluded = set(load_json(RECORDS / "excluded.json", {"tasks": []})["tasks"])
        for fam in sorted({t.rsplit("-l", 1)[0] for t in excluded}):
            spare = replace_family(fam, "validation-cap")
            emit(stage="assemble-exclude", id=fam, status="replaced" if spare else "dropped")
        active = load_json(RECORDS / "active.json")["active"]
        staging = SUITE.parent / f".confirmation-3.staging-{os.getpid()}-{time.time_ns()}"
        staging.mkdir(parents=True)
        complete, incomplete = 0, 0
        for fid in active:
            fam_dir = staging / "_tmp" / fid
            st = load_json(RECORDS / f"author-{fid}" / "status.json", {})
            try:
                verified = bool(st.get("done")) and st.get("hashes") == tree_hashes(C3 / fid / "out" / fid)
            except (OSError, PermissionError):
                verified = False
            ok = verified and all(copy_task(fid, lv, fam_dir) for lv in (1, 2, 3, 4))
            if ok:
                for tdir in fam_dir.iterdir():
                    shutil.move(str(tdir), str(staging / tdir.name))
                complete += 1
            else:
                incomplete += 1
                emit(stage="assemble-incomplete", id=fid)
        shutil.rmtree(staging / "_tmp", ignore_errors=True)
        old = SUITE.parent / f".confirmation-3.old-{time.time_ns()}"
        if SUITE.exists():
            os.rename(SUITE, old)
        os.rename(staging, SUITE)
        if old.exists():
            shutil.rmtree(old)
        emit(stage="assemble", families=complete, missing=incomplete, tasks=complete * 4)


if __name__ == "__main__":
    main()
