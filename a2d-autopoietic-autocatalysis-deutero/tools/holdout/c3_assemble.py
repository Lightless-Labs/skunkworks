"""Assemble confirmation-3 contract files into the suite dir (plan §0.6 step 5).

Copies ONLY task.json, public_tests.rs, hidden_tests.rs, reference.rs and mutants/*.rs
from authoring/confirmation-3/<id>/out/<id>/<id>-lN/ into $A2D_HOLDOUT_DIR/confirmation-3/<id>-lN/.
A task is assembled only if every contract file exists and task.json carries the opaque
id and family. Prints only counts and opaque ids.

usage: python3 -I tools/holdout/c3_assemble.py
"""
import json
import shutil
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from c3lib import C3, SUITE, emit, gateway, read_ids  # noqa: E402

CONTRACT = ("task.json", "public_tests.rs", "hidden_tests.rs", "reference.rs")


def assemble_task(fid, level):
    tid = f"{fid}-l{level}"
    src = C3 / fid / "out" / fid / tid
    if not all((src / f).is_file() for f in CONTRACT):
        return False
    mutants = sorted((src / "mutants").glob("m*.rs")) if (src / "mutants").is_dir() else []
    if len(mutants) < 4:
        return False
    try:
        tj = json.loads((src / "task.json").read_text())
    except ValueError:
        return False
    if tj.get("id") != tid or tj.get("family") != fid:
        return False
    dst = SUITE / tid
    if dst.exists():
        shutil.rmtree(dst)
    (dst / "mutants").mkdir(parents=True)
    for f in CONTRACT:
        shutil.copy(src / f, dst / f)
    for m in mutants:
        shutil.copy(m, dst / "mutants" / m.name)
    return True


@gateway("assemble")
def main():
    ids = read_ids(C3 / "FAMILIES.txt")
    SUITE.mkdir(parents=True, exist_ok=True)
    ok_tasks, missing = 0, 0
    for fid in ids:
        n = sum(assemble_task(fid, lv) for lv in (1, 2, 3, 4))
        ok_tasks += n
        missing += 4 - n
        if n < 4:
            emit(stage="assemble", id=fid, tasks=n)
    emit(stage="assemble", families=len(ids), tasks=ok_tasks, missing=missing)


if __name__ == "__main__":
    main()
