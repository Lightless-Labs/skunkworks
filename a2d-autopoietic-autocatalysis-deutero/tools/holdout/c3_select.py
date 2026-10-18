"""Confirmation-3 family selection (plan §0.6 step 4, Option B).

1. Build INVENTORY.txt: every family name used anywhere (repo task sets plus the
   holdout sets), read by this script and written only inside the holdout dir.
2. Sol (high) writes 40 opaque-id briefs (c3-f01..c3-f40) inside the outer sandbox.
3. A second, independent Sol session marks each brief OK or CONFLICT against the
   inventory and lower-numbered briefs.
4. FAMILIES.txt = the first 36 OK ids in order (spares replace conflicts in order).

Prints only gateway lines (counts and opaque ids).

usage: python3 -I tools/holdout/c3_select.py [--select-only|--check-only]
"""
import json
import shutil
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from c3lib import (AUTHORING_FILES, C3, HOLDOUT, emit, gateway, isolation_check, opaque_id,  # noqa: E402
                   run_codex)

REPO = Path(__file__).resolve().parents[2]
N_FAMILIES, N_TOTAL = 36, 40


def inventory():
    names = set()
    roots = [REPO / "tasks"] + [HOLDOUT / s for s in ("confirmation-1", "confirmation-2", "dev-holdout",
                                                      "c1-validation")]
    for root in roots:
        for tj in root.rglob("task.json"):
            try:
                fam = json.loads(tj.read_text()).get("family")
            except (OSError, ValueError):
                continue
            if fam:
                names.add(fam)
    names.add("spreadsheet-eval")  # excluded from confirmation-2 but authored; still used
    return sorted(names)


def prepare(workdir, instructions, inv):
    workdir.mkdir(parents=True, exist_ok=True)
    shutil.copy(AUTHORING_FILES / "SPEC.md", workdir / "SPEC.md")
    shutil.copy(AUTHORING_FILES / instructions, workdir / "INSTRUCTIONS.md")
    (workdir / "INVENTORY.txt").write_text("\n".join(inv) + "\n")


def select(inv):
    w = C3 / "_select"
    prepare(w, "SELECT.md", inv)
    ok = isolation_check(w)
    emit(stage="select-isolation", ok=ok)
    if not ok:
        sys.exit(2)
    code, other = run_codex(w, effort="high", timeout=5400, pidfile=C3 / "pids.txt")
    briefs = sorted((w / "families").glob("c3-f*/FAMILY.md"))
    emit(stage="select", exit=code, other_events=other, count=len(briefs), n=len(inv))
    return code == 0 and len(briefs) == N_TOTAL


def check(inv):
    src = C3 / "_select" / "families"
    w = C3 / "_namecheck"
    if w.exists():
        shutil.rmtree(w)
    prepare(w, "NAMECHECK.md", inv)
    shutil.copytree(src, w / "families", ignore=shutil.ignore_patterns("NAMES.tsv"))
    ok = isolation_check(w)
    emit(stage="namecheck-isolation", ok=ok)
    if not ok:
        sys.exit(2)
    code, other = run_codex(w, effort="high", timeout=3600, pidfile=C3 / "pids.txt")
    verdicts = {}
    vf = w / "VERDICTS.tsv"
    if vf.exists():
        for line in vf.read_text().splitlines():
            parts = line.strip().split("\t")
            if len(parts) == 2 and parts[1] in ("OK", "CONFLICT"):
                verdicts[parts[0]] = parts[1]
    ids = [opaque_id(i) for i in range(1, N_TOTAL + 1)]
    chosen = [i for i in ids if verdicts.get(i) == "OK"][:N_FAMILIES]
    (C3 / "FAMILIES.txt").write_text("\n".join(chosen) + "\n")
    emit(stage="namecheck", exit=code, other_events=other, count=len(verdicts),
         conflicts=sum(v == "CONFLICT" for v in verdicts.values()), families=len(chosen))
    return code == 0 and len(chosen) == N_FAMILIES


@gateway("select")
def main():
    C3.mkdir(parents=True, exist_ok=True)
    inv = inventory()
    mode = sys.argv[1] if len(sys.argv) > 1 else ""
    if mode != "--check-only" and not select(inv):
        emit(stage="select", status="incomplete")
        sys.exit(1)
    if mode != "--select-only" and not check(inv):
        emit(stage="namecheck", status="incomplete")
        sys.exit(1)
    emit(stage="select", status="ok")


if __name__ == "__main__":
    main()
