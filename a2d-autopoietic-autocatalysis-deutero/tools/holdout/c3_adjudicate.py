"""Blind adjudication of audit failures (plan §0.11; instructions in authoring/ADJUDICATE.md).

One sandboxed Sol (high) job per family and mode, with only that family's failing tasks: the
task files from the assembled suite, the audit solver's submission and its hidden-test output.
It classifies every failing hidden test as DETERMINED (the requirements imply it: the task is
kept unchanged) or UNDERDETERMINED.

- mode "repair" (a task's round-1 audit failed): UNDERDETERMINED tasks must come back
  repaired under out/<id>/ (task.json with only `requirements` changed, and/or
  hidden_tests.rs). The repair is applied to the family's verified author files the way
  c3_fix.py applies a repair (hashes must match, snapshot, verify, restore on failure). The
  task is then re-assembled, re-validated, re-checked for similarity and re-audited once
  (c3_audit.py round 2). A repair that is missing or malformed excludes the task.
- mode "classify" (the task failed again at its re-audit, or its first informative audit was
  the re-audit): classification only. Still UNDERDETERMINED -> the task is excluded
  (records/excluded.json), and the assembly replaces or drops its family.

Caps: 2 jobs per family and mode, reserved before launch; both failing excludes the tasks
(fail closed). The verdict file is parsed strictly against the failing tests the driver read
from the hidden output (or `*` when the submission did not compile). Prints only counts and
opaque ids.

usage: python3 -I tools/holdout/c3_adjudicate.py [--parallel N]
"""
import json
import re
import shutil
import sys
import threading
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from c3lib import (AUTHORING_FILES, C3, HOLDOUT, RECORDS, SUITE, TASK_RE, DriverLock, StopRequested,  # noqa: E402
                   emit, gateway, isolation_check, load_json, prereqs_ok, run_codex, safe_copy, safe_read_text,
                   safe_write_text, save_json, stop_requested, suspicious_commands, tree_hashes)

MAX_JOBS = 2
CONTRACT = ("task.json", "public_tests.rs", "hidden_tests.rs", "reference.rs")
MUTANTS = tuple(f"m{i}.rs" for i in (1, 2, 3, 4))
FAILED = re.compile(r"^test (\S+) \.\.\. FAILED$")
TEST_NAME = re.compile(r"^(\*|[A-Za-z_][A-Za-z0-9_:]*)$")
MODE_TEXT = {
    "repair": (
        "## Repair (only tasks with at least one UNDERDETERMINED line)\n"
        "For each such task, write the repaired files under `out/<id>/`: `task.json` (only its `requirements` "
        "text may change) and/or `hidden_tests.rs`. Prefer stating in the requirements the behaviour the test "
        "checks. Change a hidden test only when it checks behaviour the task should not prescribe, and then "
        "remove or loosen only that test. Never change a test to match the submission's output, and keep the "
        "task as difficult as it was. Verify with `./check.sh tasks/<id>/reference.rs <hidden tests>` that the "
        "reference passes every hidden test, and with each `tasks/<id>/mutants/mN.rs` that every mutant still "
        "fails at least one. Write nothing under `out/` for tasks whose lines are all DETERMINED."),
    "classify": (
        "## Classification only\n"
        "These tasks were already repaired once, or this is their only informative audit. Do not write `out/` "
        "and do not change any file except `VERDICTS.tsv`."),
}
_lock = threading.Lock()


def failing_tests(hidden_output):
    names = sorted({m.group(1) for m in map(FAILED.match, hidden_output.splitlines()) if m})
    return names or ["*"]


def due(audit, adj, suite):
    """{(family, mode): [task, ...]} of failing tasks awaiting adjudication."""
    out = {}
    for tid in suite:
        a, j = audit.get(tid, {}), adj.get(tid, {})
        if "r1" in a and not a["r1"]["passed"] and not a["r1"]["void"] and "r1" not in j:
            mode, rnd = "repair", "r1"
        elif "r2" in a and not a["r2"]["passed"] and not a["r2"]["void"] and "r2" not in j:
            mode, rnd = "classify", "r2"
        else:
            continue
        out.setdefault((tid.rsplit("-l", 1)[0], mode), []).append((tid, rnd, a[rnd]["run"]))
    return out


def prepare(w, tasks, mode):
    if w.exists():
        shutil.rmtree(w)
    w.mkdir(parents=True)
    expected = {}
    for tid, _, run in tasks:
        for f in CONTRACT:
            safe_copy(SUITE / tid / f, w / "tasks" / tid / f, SUITE, w)
        for m in MUTANTS:
            safe_copy(SUITE / tid / "mutants" / m, w / "tasks" / tid / "mutants" / m, SUITE, w)
        sub = HOLDOUT / "runs" / f"c3-audit-{run}" / "submissions" / "D" / tid
        safe_copy(sub / "r0.rs", w / "solver" / tid / "submission.rs", HOLDOUT, w)
        safe_copy(sub / "r0.hidden.txt", w / "solver" / tid / "hidden_output.txt", HOLDOUT, w)
        expected[tid] = failing_tests(safe_read_text(sub / "r0.hidden.txt", HOLDOUT))
    safe_copy(AUTHORING_FILES / "check.sh", w / "check.sh", AUTHORING_FILES, w, mode=0o755)
    text = (AUTHORING_FILES / "ADJUDICATE.md").read_text().replace("{MODE}", MODE_TEXT[mode])
    safe_write_text(w / "INSTRUCTIONS.md", text, w)
    return expected


def parse_verdicts(w, expected):
    """{task: {test: verdict}} covering exactly the expected failing tests, else None."""
    got = {}
    try:
        for line in safe_read_text(w / "VERDICTS.tsv", w).splitlines():
            if not line.strip():
                continue
            parts = line.strip().split("\t")
            if len(parts) != 3 or parts[0] not in expected or not TEST_NAME.match(parts[1]) \
                    or parts[2] not in ("DETERMINED", "UNDERDETERMINED") or parts[1] in got.get(parts[0], {}):
                return None
            got.setdefault(parts[0], {})[parts[1]] = parts[2]
    except (OSError, PermissionError):
        return None
    return got if {t: sorted(v) for t, v in got.items()} == {t: sorted(v) for t, v in expected.items()} else None


def apply_repair(fid, tid, w):
    """c3_fix pattern: verified author files, snapshot, copy the repair in, verify, else restore."""
    from c3_author import complete
    status_path = RECORDS / f"author-{fid}" / "status.json"
    st = load_json(status_path, {})
    out = C3 / fid / "out" / fid
    if not st.get("done") or st.get("hashes") != tree_hashes(out):
        return False
    src = w / "out" / tid
    try:
        names = sorted(p.name for p in src.iterdir()) if src.is_dir() and not src.is_symlink() else []
        if not names or not set(names) <= {"task.json", "hidden_tests.rs"}:
            return False
        if "task.json" in names:
            old = json.loads(safe_read_text(out / tid / "task.json", C3))
            new = json.loads(safe_read_text(src / "task.json", w))
            if not isinstance(new, dict) or not isinstance(new.get("requirements"), str) \
                    or not new["requirements"].strip() \
                    or {k: v for k, v in new.items() if k != "requirements"} != \
                    {k: v for k, v in old.items() if k != "requirements"}:
                return False
    except (OSError, PermissionError, ValueError):
        return False
    snapshot = RECORDS / f"adj-{fid}" / "snapshot"
    if snapshot.exists():
        shutil.rmtree(snapshot)
    shutil.copytree(out, snapshot, symlinks=True)
    try:
        for n in names:
            safe_copy(src / n, out / tid / n, w, C3)
        ok = complete(fid)
    except (OSError, PermissionError, ValueError):
        ok = False
    if ok:
        st["hashes"] = tree_hashes(out)
        save_json(status_path, st)
    else:
        shutil.rmtree(C3 / fid / "out")
        (C3 / fid / "out").mkdir()
        shutil.copytree(snapshot, out, symlinks=True)
    return ok


def one(key, tasks):
    fid, mode = key
    jobs_key = f"{fid}-{mode}"
    with _lock:
        if stop_requested():
            emit(stage="adjudicate", id=fid, status="stopped")
            return
        jobs = load_json(RECORDS / "adj-jobs.json", {})
        if jobs.get(jobs_key, 0) >= MAX_JOBS:
            emit(stage="adjudicate", id=fid, status="cap-exhausted")
            return
    w = C3 / "_adj" / jobs_key
    expected = prepare(w, tasks, mode)
    if not isolation_check(w):
        with _lock:
            emit(stage="adjudicate-isolation", id=fid, ok=False)
        return
    with _lock:
        jobs = load_json(RECORDS / "adj-jobs.json", {})
        jobs[jobs_key] = jobs.get(jobs_key, 0) + 1          # reserved before launch
        save_json(RECORDS / "adj-jobs.json", jobs)
        attempt = jobs[jobs_key]
    record = f"adj-{jobs_key}-a{attempt}"
    try:
        code, other = run_codex(w, record, effort="high", timeout=5400)
    except StopRequested:
        with _lock:
            jobs = load_json(RECORDS / "adj-jobs.json", {})
            jobs[jobs_key] -= 1
            save_json(RECORDS / "adj-jobs.json", jobs)
            emit(stage="adjudicate", id=fid, status="stopped")
        return
    susp = suspicious_commands(record, w)
    verdicts = parse_verdicts(w, expected) if code == 0 and other == 0 and susp == 0 else None
    with _lock:
        adj = load_json(RECORDS / "adjudication.json", {})
        excluded = set(load_json(RECORDS / "excluded.json", {"tasks": []})["tasks"])
        if verdicts is None:
            if attempt >= MAX_JOBS:                           # fail closed: the tasks cannot be certified
                for tid, rnd, _ in tasks:
                    adj.setdefault(tid, {})[rnd] = {"verdicts": None, "repaired": False, "excluded": True}
                    excluded.add(tid)
            emit(stage="adjudicate", id=fid, exit=code, other_events=other, skipped=susp, ok=False, round=attempt)
        else:
            under = 0
            for tid, rnd, _ in tasks:
                v = verdicts[tid]
                undetermined = "UNDERDETERMINED" in v.values()
                under += undetermined
                rec = {"verdicts": v, "repaired": False, "excluded": False}
                if undetermined and mode == "repair":
                    rec["repaired"] = apply_repair(fid, tid, w)
                if undetermined and not rec["repaired"]:
                    rec["excluded"] = True
                    excluded.add(tid)
                adj.setdefault(tid, {})[rnd] = rec
            emit(stage="adjudicate", id=fid, exit=code, ok=True, tasks=len(tasks), conflicts=under, round=attempt)
        save_json(RECORDS / "adjudication.json", adj)
        save_json(RECORDS / "excluded.json", {"tasks": sorted(excluded)})


@gateway("adjudicate")
def main():
    args = sys.argv[1:]
    parallel = int(args[args.index("--parallel") + 1]) if "--parallel" in args else 3
    if not 1 <= parallel <= 3:
        raise ValueError("parallel")
    if not prereqs_ok():
        emit(stage="adjudicate", status="prereqs-not-met")
        sys.exit(2)
    with DriverLock():
        audit = load_json(RECORDS / "audit.json", {})
        adj = load_json(RECORDS / "adjudication.json", {})
        suite = sorted(p.name for p in SUITE.iterdir() if TASK_RE.match(p.name))
        work = due(audit, adj, suite)
        emit(stage="adjudicate", families=len({f for f, _ in work}), tasks=sum(len(v) for v in work.values()))
        with ThreadPoolExecutor(max_workers=parallel) as ex:
            list(ex.map(lambda kv: one(*kv), sorted(work.items())))
        adj = load_json(RECORDS / "adjudication.json", {})
        recs = [r for t in adj.values() for r in t.values()]
        emit(stage="adjudicate", done=len(recs), count=sum(r["repaired"] for r in recs),
             skipped=sum(r["excluded"] for r in recs))


if __name__ == "__main__":
    main()
