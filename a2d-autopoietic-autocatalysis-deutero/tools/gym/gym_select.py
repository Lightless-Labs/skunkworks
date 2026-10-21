"""Gym v3 family selection (copy of tools/holdout/c3_select.py, adapted; plan 2026-10-21).

1. INVENTORY.txt: every family name in the read-only prior corpus (repo tasks/ and the holdout
   sets; confirmation-3 only after its decision.json), written only inside the gym root.
2. Sol (high) writes (QUOTA + SPARES) x 5 opaque-id briefs (g3-f001 ...), each with one rubric
   class; the class quotas are equal and fixed in code before any brief (gymlib.CLASSES).
3. A second, independent Sol session marks each brief OK or CONFLICT.
4. Briefs are kept mechanically: per class, the first QUOTA OK briefs in id order; the class's
   remaining OK briefs are its spares (a replacement keeps its class). FAMILIES.txt is
   write-once; RECORDS/active.json starts equal to it. A short class fails the selection
   (one --reset allowed); below MIN_FAMILIES in all, stop.

Every step fails closed and passes the launch gate first. Prints only gateway lines.

usage: python3 -I tools/gym/gym_select.py [--reset]
"""
import shutil
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from gymlib import (AUTHORING_FILES, CLASSES, GYM, ID_RE, QUOTA_PER_CLASS, RECORDS, SPARES_PER_CLASS,  # noqa: E402
                    DriverLock, LaunchGateClosed, PriorCorpus, StopRequested, emit, gate_or_exit, gateway,
                    isolation_check, load_json, opaque_id, prereqs_ok, run_codex, safe_copy, safe_read_text,
                    safe_write_text, save_json, stop_requested, suspicious_commands, tree_hashes)

PER_CLASS = QUOTA_PER_CLASS + SPARES_PER_CLASS
N_TOTAL = PER_CLASS * len(CLASSES)
N_FAMILIES = QUOTA_PER_CLASS * len(CLASSES)
MIN_FAMILIES = (N_FAMILIES * 4) // 5
IDS = [opaque_id(i) for i in range(1, N_TOTAL + 1)]


def prepare(workdir, instructions, inventory):
    workdir.mkdir(parents=True, exist_ok=False)
    safe_copy(AUTHORING_FILES / "SPEC.md", workdir / "SPEC.md", AUTHORING_FILES)
    text = (AUTHORING_FILES / instructions).read_text().replace("{PER_CLASS}", str(PER_CLASS)) \
        .replace("{TOTAL}", str(N_TOTAL))
    safe_write_text(workdir / "INSTRUCTIONS.md", text)
    safe_write_text(workdir / "INVENTORY.txt", "\n".join(inventory) + "\n")


def read_classes(fam_dir):
    """{id: class} from CLASSES.tsv: exactly the ids, known classes, PER_CLASS of each; else None."""
    out = {}
    try:
        for line in safe_read_text(fam_dir / "CLASSES.tsv", GYM).splitlines():
            if not line.strip():
                continue
            parts = line.strip().split("\t")
            if len(parts) != 2 or not ID_RE.match(parts[0]) or parts[1] not in CLASSES or parts[0] in out:
                return None
            out[parts[0]] = parts[1]
    except (OSError, PermissionError):
        return None
    if sorted(out) != IDS or any(list(out.values()).count(c) != PER_CLASS for c in CLASSES):
        return None
    return out


def briefs_complete(fam_dir):
    try:
        present = sorted(p.name for p in fam_dir.iterdir() if p.name.startswith("g3-f"))
        if present != IDS:
            return False
        for i in IDS:
            if not safe_read_text(fam_dir / i / "FAMILY.md", GYM).strip():
                return False
        names = [x for x in safe_read_text(fam_dir / "NAMES.tsv", GYM).splitlines() if x.strip()]
        return len(names) == N_TOTAL and read_classes(fam_dir) is not None
    except (OSError, PermissionError):
        return False


def generation():
    return load_json(RECORDS / "reselect.json", {"count": 0})["count"]


def guarded_run(w, record, timeout):
    try:
        return run_codex(w, record, effort="high", timeout=timeout)
    except (StopRequested, LaunchGateClosed):
        shutil.rmtree(w)
        emit(stage=record, status="stopped")
        sys.exit(2)


def select(inventory):
    w = GYM / "_select"
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
    if ok:
        save_json(RECORDS / "select.json", {"ok": True, "generation": generation(),
                                            "hashes": tree_hashes(w / "families")})
    return ok


def keep(classes, oks):
    """Per class, the first QUOTA OK ids in id order; the rest of each class's OK ids are spares."""
    chosen, spares = [], {}
    for c in CLASSES:
        ids = [i for i in IDS if i in oks and classes[i] == c]
        chosen += ids[:QUOTA_PER_CLASS]
        spares[c] = ids[QUOTA_PER_CLASS:]
    return sorted(chosen), spares


def check(inventory):
    if (GYM / "FAMILIES.txt").exists():
        raise RuntimeError("FAMILIES.txt exists (write-once)")
    rec = load_json(RECORDS / "select.json", {})
    if not rec.get("ok") or rec.get("generation") != generation() \
            or rec.get("hashes") != tree_hashes(GYM / "_select" / "families"):
        raise RuntimeError("no successful selection record for these briefs")
    w = GYM / "_namecheck"
    if w.exists():
        raise RuntimeError("namecheck exists (write-once)")
    prepare(w, "NAMECHECK.md", inventory)
    for i in IDS:
        safe_copy(GYM / "_select" / "families" / i / "FAMILY.md", w / "families" / i / "FAMILY.md", GYM)
    ok = isolation_check(w)
    emit(stage="namecheck-isolation", ok=ok)
    if not ok:
        sys.exit(2)
    code, other = guarded_run(w, "namecheck", 3600)
    other += suspicious_commands("namecheck", w)
    verdicts = {}
    try:
        for line in safe_read_text(w / "VERDICTS.tsv", GYM).splitlines():
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
    oks = {i for i in IDS if verdicts.get(i) == "OK"}
    emit(stage="namecheck", exit=code, other_events=other, ok=valid,
         conflicts=sum(v == "CONFLICT" for v in verdicts.values()), count=len(oks))
    if not valid:
        return False
    classes = read_classes(GYM / "_select" / "families")
    chosen, spares = keep(classes, oks)
    short = [c for c in CLASSES if sum(1 for i in chosen if classes[i] == c) < QUOTA_PER_CLASS]
    if len(chosen) < MIN_FAMILIES:
        emit(stage="namecheck", status="too-few-families", families=len(chosen))
        return False
    if short and generation() < 1:
        emit(stage="namecheck", status="short-reselect-allowed", families=len(chosen), classes=len(short))
        return False
    safe_write_text(GYM / "FAMILIES.txt", "\n".join(chosen) + "\n")
    save_json(RECORDS / "active.json", {"active": chosen, "spares": spares,
                                        "classes": {i: classes[i] for i in sorted(oks)}, "events": []})
    emit(stage="namecheck", families=len(chosen), classes=len(short))
    return True


@gateway("select")
def main():
    gate_or_exit("select")
    GYM.mkdir(parents=True, exist_ok=True)
    if not prereqs_ok():
        emit(stage="select", status="prereqs-not-met")
        sys.exit(2)
    if stop_requested():
        emit(stage="select", status="stopped")
        sys.exit(2)
    with DriverLock():
        if "--reset" in sys.argv[1:]:
            n = generation()
            if n >= 1 or (GYM / "FAMILIES.txt").exists():
                raise RuntimeError("reselection not allowed")
            for d in ("_select", "_namecheck"):
                if (GYM / d).exists():
                    shutil.move(str(GYM / d), str(GYM / f"{d}-failed{n + 1}"))
            for r in ("select.json", "select", "namecheck"):
                if (RECORDS / r).exists():
                    shutil.move(str(RECORDS / r), str(RECORDS / f"failed{n + 1}-{r}"))
            save_json(RECORDS / "reselect.json", {"count": n + 1})
        corpus = PriorCorpus()
        rows, counts = corpus.tasks()
        for k in sorted(counts):
            emit(stage="inventory", status=k, n=counts[k])
        inventory = sorted({fam for fam, _ in rows})
        emit(stage="inventory", families=len(inventory))
        save_json(RECORDS / "prior_manifest.json", corpus.manifest())
        if not (GYM / "_select").exists() and not select(inventory):
            emit(stage="select", status="incomplete")
            sys.exit(1)
        rec = load_json(RECORDS / "select.json", {})
        if not rec.get("ok") or rec.get("generation") != generation() \
                or not briefs_complete(GYM / "_select" / "families"):
            emit(stage="select", status="incomplete")
            sys.exit(1)
        if not check(inventory):
            emit(stage="namecheck", status="incomplete")
            sys.exit(1)
        emit(stage="select", status="ok")


if __name__ == "__main__":
    main()
