"""Blind repair of rejected gym v3 tasks (copy of tools/holdout/c3_fix.py, adapted; at most 2
rounds per family).

For each active family with FIX-VERDICTS.txt from the LATEST validation (records/validate.json),
fewer than 2 rounds used, and no repair yet bound to that validation, reserve a round in the
records, then run one Sol (high) FIX job in the family dir inside the outer sandbox (through the
launch gate). A failed or unclean round restores the verified files. Re-assemble and re-validate
between rounds. Prints only opaque ids, exit codes and round numbers.

usage: python3 -I tools/gym/gym_fix.py [--parallel N]
"""
import shutil
import sys
import threading
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from gymlib import (AUTHORING_FILES, GYM, RECORDS, DriverLock, LaunchGateClosed, StopRequested, emit,  # noqa: E402
                    gate_or_exit, gateway, isolation_check, job_codex_home, load_json, prereqs_ok, run_codex,
                    safe_read_bytes,
                    safe_write_text, save_json, stop_requested, suspicious_commands, tree_hashes)

MAX_ROUNDS = 2
_lock = threading.Lock()


def complete(fam):
    from gym_author import complete as author_complete
    return author_complete(fam)


def audited(fam):
    return any(t.rsplit("-l", 1)[0] == fam and rounds for t, rounds in load_json(RECORDS / "audit.json", {}).items())


def hidden_tests_kept(fam, snapshot, out):
    """Hidden tests are never edited after the audit: once any task of the family has an audit
    round, a FIX must leave every hidden_tests.rs byte-identical (else the round fails and the
    verified files are restored)."""
    if not audited(fam):
        return True
    for lv in (1, 2, 3, 4):
        rel = Path(f"{fam}-l{lv}") / "hidden_tests.rs"
        try:
            if safe_read_bytes(out / rel, GYM) != safe_read_bytes(snapshot / rel, RECORDS):
                return False
        except OSError:
            return False
    return True


def one(fam, stamp):
    with _lock:
        if stop_requested():
            emit(stage="fix", id=fam, status="stopped")
            return
        rec = load_json(RECORDS / f"fix-{fam}" / "rounds.json", {"rounds": 0, "stamps": []})
        if rec["rounds"] >= MAX_ROUNDS or stamp in rec["stamps"]:
            emit(stage="fix", id=fam, status="skipped", round=rec["rounds"])
            return
    w = GYM / fam
    safe_write_text(w / "INSTRUCTIONS.md", (AUTHORING_FILES / "FIX.md").read_text())
    job_codex_home(w)
    if not isolation_check(w):
        with _lock:
            emit(stage="fix-isolation", id=fam, ok=False)
        return
    status_path = RECORDS / f"author-{fam}" / "status.json"
    st = load_json(status_path, {})
    out = GYM / fam / "out" / fam
    if not st.get("done") or st.get("hashes") != tree_hashes(out):
        with _lock:
            emit(stage="fix", id=fam, status="unverified-artifacts")
        return
    with _lock:
        rec["rounds"] += 1
        rec["stamps"].append(stamp)
        save_json(RECORDS / f"fix-{fam}" / "rounds.json", rec)
    record = f"fix-{fam}-r{rec['rounds']}"
    snapshot = RECORDS / record / "snapshot"
    if snapshot.exists():
        shutil.rmtree(snapshot)
    shutil.copytree(out, snapshot, symlinks=True)
    try:
        code, other = run_codex(w, record, effort="high", timeout=5400)
    except (StopRequested, LaunchGateClosed):
        with _lock:
            rec["rounds"] -= 1
            rec["stamps"].remove(stamp)
            save_json(RECORDS / f"fix-{fam}" / "rounds.json", rec)
            emit(stage="fix", id=fam, status="stopped")
        return
    susp = suspicious_commands(record, w)
    ok = code == 0 and other == 0 and susp == 0 and complete(fam) and hidden_tests_kept(fam, snapshot, out)
    with _lock:
        if ok:
            st["hashes"] = tree_hashes(out)
            st["fixed_rounds"] = rec["rounds"]
        else:
            shutil.rmtree(GYM / fam / "out")
            (GYM / fam / "out").mkdir()
            shutil.copytree(snapshot, out, symlinks=True)
        save_json(status_path, st)
        emit(stage="fix", id=fam, exit=code, other_events=other, skipped=susp, ok=ok, round=rec["rounds"])


@gateway("fix")
def main():
    args = sys.argv[1:]
    parallel = int(args[args.index("--parallel") + 1]) if "--parallel" in args else 3
    if not 1 <= parallel <= 3:
        raise ValueError("parallel")
    gate_or_exit("fix")
    if not prereqs_ok():
        emit(stage="fix", status="prereqs-not-met")
        sys.exit(2)
    with DriverLock():
        val = load_json(RECORDS / "validate.json")
        if not val:
            raise ValueError("no validation")
        active = set(load_json(RECORDS / "active.json")["active"])
        fams = sorted(f.parent.name for f in GYM.glob("g3-f*/FIX-VERDICTS.txt") if f.parent.name in active)
        emit(stage="fix", families=len(fams))
        with ThreadPoolExecutor(max_workers=parallel) as ex:
            list(ex.map(lambda f: one(f, val["sha256"]), fams))


if __name__ == "__main__":
    main()
