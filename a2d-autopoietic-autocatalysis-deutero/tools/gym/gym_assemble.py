"""Assemble the gym v3 suite (copy of tools/holdout/c3_assemble.py, adapted; plan 2026-10-21).

Builds a FRESH staging suite from the active families only, copying (symlinks refused) exactly
task.json, public_tests.rs, hidden_tests.rs, reference.rs, rubric.json and mutants/m1..m4.rs
from authoring/gym-v3/<id>/out/<id>/<id>-lN/. A family enters only with all 4 tasks complete
and verified (status hashes match); then the staging dir atomically replaces <gym root>/gym-v3,
which is sealed under the gym root (hidden tests and references never leave it). Excluded tasks
(records/excluded.json; reasons in gym_rubric.EXCLUSION_REASONS, never a solver failure) and
families whose 2 authoring attempts are used up are replaced by a same-class spare that inherits
the split slot, or dropped. records/assembled.json maps each admitted family to its attempt.
Prints only counts and opaque ids.

usage: python3 -I tools/gym/gym_assemble.py
"""
import json
import os
import shutil
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from gymlib import (GYM, RECORDS, SUITE, TASK_RE, DriverLock, emit, gate_or_exit, gateway,  # noqa: E402
                    hashes_digest, load_json, mutants_ok, parse_task_json, replace_family, safe_copy, safe_read_bytes,
                    save_json, tree_hashes)

MAX_ATTEMPTS = 2
CONTRACT = ("task.json", "public_tests.rs", "hidden_tests.rs", "reference.rs", "rubric.json")
MUTANTS = tuple(f"m{i}.rs" for i in (1, 2, 3, 4))


def copy_task(fid, level, staging):
    tid = f"{fid}-l{level}"
    src = GYM / fid / "out" / fid / tid
    try:
        tj = parse_task_json(safe_read_bytes(src / "task.json", GYM))
        if tj["id"] != tid or tj["family"] != fid or not TASK_RE.match(tid):
            return False
        if not mutants_ok(src / "mutants"):
            return False
        for f in CONTRACT:
            safe_copy(src / f, staging / tid / f, GYM, staging)
        for m in MUTANTS:
            safe_copy(src / "mutants" / m, staging / tid / "mutants" / m, GYM, staging)
        return True
    except (OSError, PermissionError, ValueError):
        return False


@gateway("assemble")
def main():
    gate_or_exit("assemble")
    with DriverLock():
        backups = sorted(SUITE.parent.glob(".gym-v3.old-*"))
        if not SUITE.exists() and backups:
            os.rename(backups[-1], SUITE)
            backups = backups[:-1]
        for b in backups + sorted(SUITE.parent.glob(".gym-v3.staging-*")):
            shutil.rmtree(b)
        excluded = load_json(RECORDS / "excluded.json", {"tasks": [], "reasons": {}})
        for fam in sorted({t.rsplit("-l", 1)[0] for t in excluded["tasks"]}):
            if fam not in load_json(RECORDS / "active.json")["active"]:
                continue                                    # replaced or dropped at an earlier assembly
            reason = excluded.get("reasons", {}).get(fam, "validation-cap")
            spare = replace_family(fam, reason)
            emit(stage="assemble-exclude", id=fam, status="replaced" if spare else "dropped")
        for fam in list(load_json(RECORDS / "active.json")["active"]):
            st = load_json(RECORDS / f"author-{fam}" / "status.json", {"attempts": 0})
            if st["attempts"] >= MAX_ATTEMPTS and not st.get("done"):
                spare = replace_family(fam, "author-exhausted")
                emit(stage="assemble-exhausted", id=fam, status="replaced" if spare else "dropped")
        active = load_json(RECORDS / "active.json")["active"]
        assembled, admitted = {}, {}
        staging = SUITE.parent / f".gym-v3.staging-{os.getpid()}-{time.time_ns()}"
        staging.mkdir(parents=True)
        complete, incomplete = 0, 0
        for fid in active:
            fam_dir = staging / "_tmp" / fid
            st = load_json(RECORDS / f"author-{fid}" / "status.json", {})
            try:
                verified = bool(st.get("done")) and st.get("hashes") == tree_hashes(GYM / fid / "out" / fid)
            except (OSError, PermissionError):
                verified = False
            ok = verified and all(copy_task(fid, lv, fam_dir) for lv in (1, 2, 3, 4))
            if ok:
                for tdir in fam_dir.iterdir():
                    shutil.move(str(tdir), str(staging / tdir.name))
                assembled[fid] = st["attempts"]
                admitted[fid] = hashes_digest(st["hashes"])
                complete += 1
            else:
                incomplete += 1
                emit(stage="assemble-incomplete", id=fid)
        shutil.rmtree(staging / "_tmp", ignore_errors=True)
        old = SUITE.parent / f".gym-v3.old-{time.time_ns()}"
        if SUITE.exists():
            os.rename(SUITE, old)
        os.rename(staging, SUITE)
        if old.exists():
            shutil.rmtree(old)
        save_json(RECORDS / "assembled.json", assembled)
        save_json(RECORDS / "assembled-hashes.json", admitted)
        emit(stage="assemble", families=complete, missing=incomplete, tasks=complete * 4)


if __name__ == "__main__":
    main()
