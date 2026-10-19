"""Confirmation-3 family selection (plan §0.6 step 4, Option B; rules frozen in §0.10/§0.11).

1. INVENTORY.txt: every family name used in any prior set (repo tasks/ and everything in
   the holdout dir except confirmation-3), written only inside the holdout dir.
2. Sol (high) writes 40 opaque-id briefs (c3-f01..c3-f40) in the outer sandbox.
3. A second, independent Sol session (fresh workdir and CODEX_HOME, sees only the
   inventory and the briefs, not the first session) marks each brief OK or CONFLICT.
4. FAMILIES.txt (write-once) = the first 36 OK ids in order; RECORDS/active.json starts
   equal to it. Fewer than 36 OK: the selection fails (§0.11: one fresh re-selection via
   --reset, then a smaller set, disclosed, if still short; below 30 families, stop).

Every step fails closed: exact counts, unique ids, exit 0 and zero non-allowlisted events.
Prints only gateway lines.

usage: python3 -I tools/holdout/c3_select.py [--reset]
"""
import shutil
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from c3lib import (AUTHORING_FILES, C3, ID_RE, RECORDS, DriverLock, StopRequested, emit, gateway,  # noqa: E402
                   isolation_check, load_json, opaque_id, prereqs_ok, prior_manifest, prior_tasks, run_codex,
                   safe_copy, safe_read_text, safe_write_text, save_json, stop_requested, suspicious_commands,
                   tree_hashes)

N_FAMILIES, N_TOTAL, MIN_FAMILIES = 36, 40, 30
IDS = [opaque_id(i) for i in range(1, N_TOTAL + 1)]


def prepare(workdir, instructions, inventory):
    workdir.mkdir(parents=True, exist_ok=False)
    safe_copy(AUTHORING_FILES / "SPEC.md", workdir / "SPEC.md", AUTHORING_FILES)
    safe_copy(AUTHORING_FILES / instructions, workdir / "INSTRUCTIONS.md", AUTHORING_FILES)
    safe_write_text(workdir / "INVENTORY.txt", "\n".join(inventory) + "\n")


def briefs_complete(fam_dir):
    """Exactly c3-f01..c3-f40, each a regular non-empty FAMILY.md, and a 40-line NAMES.tsv."""
    try:
        present = sorted(p.name for p in fam_dir.iterdir() if p.name.startswith("c3-f"))
        if present != IDS:
            return False
        for i in IDS:
            if not safe_read_text(fam_dir / i / "FAMILY.md", C3).strip():
                return False
        names = [x for x in safe_read_text(fam_dir / "NAMES.tsv", C3).splitlines() if x.strip()]
        return len(names) == N_TOTAL
    except (OSError, PermissionError):
        return False


def generation():
    """Selection generation: 0, or 1 after the single --reset (§0.11)."""
    return load_json(RECORDS / "reselect.json", {"count": 0})["count"]


def guarded_run(w, record, timeout):
    """run_codex, but a STOP at the launch gate removes the unlaunched workdir (nothing ran,
    so the write-once selection is not consumed) and exits."""
    try:
        return run_codex(w, record, effort="high", timeout=timeout)
    except StopRequested:
        shutil.rmtree(w)
        emit(stage=record, status="stopped")
        sys.exit(2)


def select(inventory):
    w = C3 / "_select"
    if w.exists():
        raise RuntimeError("selection exists (write-once)")
    prepare(w, "SELECT.md", inventory)
    ok = isolation_check(w)
    emit(stage="select-isolation", ok=ok)
    if not ok:
        sys.exit(2)
    code, other = guarded_run(w, "select", 5400)
    susp = suspicious_commands("select", w)
    complete = briefs_complete(w / "families")
    ok = code == 0 and other == 0 and susp == 0 and complete
    emit(stage="select", exit=code, other_events=other, skipped=susp, ok=ok)
    if ok:   # bound to this generation's successful job (a reset retires it)
        save_json(RECORDS / "select.json", {"ok": True, "generation": generation(),
                                            "hashes": tree_hashes(w / "families")})
    return ok


def check(inventory):
    if (C3 / "FAMILIES.txt").exists():
        raise RuntimeError("FAMILIES.txt exists (write-once)")
    rec = load_json(RECORDS / "select.json", {})
    if not rec.get("ok") or rec.get("generation") != generation() \
            or rec.get("hashes") != tree_hashes(C3 / "_select" / "families"):
        raise RuntimeError("no successful selection record for these briefs")
    w = C3 / "_namecheck"
    if w.exists():
        raise RuntimeError("namecheck exists (write-once)")
    prepare(w, "NAMECHECK.md", inventory)
    for i in IDS:
        safe_copy(C3 / "_select" / "families" / i / "FAMILY.md", w / "families" / i / "FAMILY.md", C3)
    ok = isolation_check(w)
    emit(stage="namecheck-isolation", ok=ok)
    if not ok:
        sys.exit(2)
    code, other = guarded_run(w, "namecheck", 3600)
    other += suspicious_commands("namecheck", w)
    verdicts = {}
    try:
        for line in safe_read_text(w / "VERDICTS.tsv", C3).splitlines():
            if not line.strip():
                continue
            parts = line.strip().split("\t")
            if len(parts) != 2 or not ID_RE.match(parts[0]) or parts[1] not in ("OK", "CONFLICT") \
                    or parts[0] in verdicts:
                raise ValueError("bad verdict line")
            verdicts[parts[0]] = parts[1]
    except (OSError, PermissionError, ValueError):
        verdicts = {}
    valid = sorted(verdicts) == IDS and code == 0 and other == 0
    oks = [i for i in IDS if verdicts.get(i) == "OK"]
    emit(stage="namecheck", exit=code, other_events=other, ok=valid,
         conflicts=sum(v == "CONFLICT" for v in verdicts.values()), count=len(oks))
    if not valid:
        return False
    chosen = oks[:N_FAMILIES]
    if len(chosen) < MIN_FAMILIES:
        emit(stage="namecheck", status="too-few-families", families=len(chosen))
        return False
    if len(chosen) < N_FAMILIES and load_json(RECORDS / "reselect.json", {}).get("count", 0) < 1:
        emit(stage="namecheck", status="short-reselect-allowed", families=len(chosen))
        return False
    safe_write_text(C3 / "FAMILIES.txt", "\n".join(chosen) + "\n")
    save_json(RECORDS / "active.json", {"active": chosen, "spares": [i for i in oks if i not in chosen],
                                        "events": []})
    emit(stage="namecheck", families=len(chosen))
    return True


@gateway("select")
def main():
    C3.mkdir(parents=True, exist_ok=True)
    if not prereqs_ok():
        emit(stage="select", status="prereqs-not-met")
        sys.exit(2)
    if stop_requested():
        emit(stage="select", status="stopped")
        sys.exit(2)
    with DriverLock():
        if "--reset" in sys.argv[1:]:
            n = generation()
            if n >= 1 or (C3 / "FAMILIES.txt").exists():
                raise RuntimeError("reselection not allowed")
            for d in ("_select", "_namecheck"):
                if (C3 / d).exists():
                    shutil.move(str(C3 / d), str(C3 / f"{d}-failed{n + 1}"))
            # retire the failed generation's success record and job records with it
            for r in ("select.json", "select", "namecheck"):
                if (RECORDS / r).exists():
                    shutil.move(str(RECORDS / r), str(RECORDS / f"failed{n + 1}-{r}"))
            save_json(RECORDS / "reselect.json", {"count": n + 1})
        rows, counts = prior_tasks()
        for k in sorted(counts):
            emit(stage="inventory", status=k, n=counts[k])
        inventory = sorted({fam for fam, _ in rows} | {"spreadsheet-eval"})
        emit(stage="inventory", families=len(inventory))
        save_json(RECORDS / "prior_manifest.json", prior_manifest())
        if not (C3 / "_select").exists() and not select(inventory):
            emit(stage="select", status="incomplete")
            sys.exit(1)
        rec = load_json(RECORDS / "select.json", {})
        if not rec.get("ok") or rec.get("generation") != generation() \
                or not briefs_complete(C3 / "_select" / "families"):
            emit(stage="select", status="incomplete")
            sys.exit(1)
        if not check(inventory):
            emit(stage="namecheck", status="incomplete")
            sys.exit(1)
        emit(stage="select", status="ok")


if __name__ == "__main__":
    main()
