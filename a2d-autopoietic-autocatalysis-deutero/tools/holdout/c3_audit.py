"""Independent-solve audit of the confirmation-3 suite (plan §0.6 step 5; rules frozen in §0.11).

Codex Sol (medium), arm D, k = 3, once per task per round, on the fixed provider: one
`a2d lift run` over a sub-suite of the tasks that need it, with the hidden output recorded
(for adjudication only; never printed). Round 1 covers every suite task without a round-1
result. Round 2 is the one re-audit: tasks repaired by round-1 adjudication, and tasks whose
round-1 tuple was void (no information; a second void is disclosed and the task kept).

Gates: the latest validation and similarity checks are both clean on exactly this suite
(digest match), and nothing is pending in the similarity state. Results go to
records/audit.json as {task: {"r1": {...}, "r2": {...}}}; the run's output stays in
$A2D_HOLDOUT_DIR/runs/c3-audit-<n>/. Prints only counts and opaque task ids.

usage: python3 -I tools/holdout/c3_audit.py [--binary <a2d>]
"""
import json
import shutil
import subprocess
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from c3lib import (HOLDOUT, RECORDS, REPO, SUITE, TASK_RE, DriverLock, emit, gateway, hashes_digest, tree_hashes,  # noqa: E402
                   load_json, prereqs_ok, provider_merged, safe_copy, safe_read_text, save_json, suite_digest)

CONTRACT = ("task.json", "public_tests.rs", "hidden_tests.rs", "reference.rs")
MUTANTS = tuple(f"m{i}.rs" for i in (1, 2, 3, 4))
SOLVER = ("codex/gpt-6.1-sol", "medium")
K = 3
PENDING_SIM = ("reauthor", "reauthor-done", "reauthor-failed")


def clean_suite(suite):
    """Validation and similarity were both clean on exactly this suite, nothing is pending,
    and every suite family is the family's current verified files (a repair after the last
    assembly makes the suite stale)."""
    digest = suite_digest()
    val = load_json(RECORDS / "validate.json", {})
    sim = load_json(RECORDS / "similarity-last.json", {})
    state = load_json(RECORDS / "similarity.json", {})
    admitted = load_json(RECORDS / "assembled-hashes.json", {})
    current = all(admitted.get(f) == hashes_digest(
        load_json(RECORDS / f"author-{f}" / "status.json", {}).get("hashes"))
        for f in {t.rsplit("-l", 1)[0] for t in suite})
    return (current and val.get("suite") == digest and val.get("rejected") == [] and sim.get("suite") == digest
            and sim.get("conflicts") == 0 and not any(v in PENDING_SIM for v in state.values()))


def needs(audit, adj, tid):
    """The round this task is due for, or None."""
    a = audit.get(tid, {})
    if "r1" not in a:
        return "r1"
    if "r2" in a:
        return None
    if a["r1"]["void"]:
        return "r2"
    if not a["r1"]["passed"] and adj.get(tid, {}).get("r1", {}).get("repaired"):
        return "r2"
    return None


def run_lift(binary, tasks, n):
    out = HOLDOUT / "runs" / f"c3-audit-{n}"
    if out.exists():
        raise RuntimeError("audit run exists")
    sub = HOLDOUT / "runs" / f"c3-audit-{n}-suite"
    if sub.exists():
        shutil.rmtree(sub)
    for tid in tasks:
        for f in CONTRACT:
            safe_copy(SUITE / tid / f, sub / tid / f, SUITE)
        for m in MUTANTS:
            safe_copy(SUITE / tid / "mutants" / m, sub / tid / "mutants" / m, SUITE)
    argv = [binary, "lift", "run", "--experiment", f"C3-AUDIT-{n}", "--unregistered", "--suite", str(sub),
            "--model", SOLVER[0], "--effort", SOLVER[1], "--arms", "D", "--k", str(K), "--jobs", "4",
            "--min-families", "1", "--record-hidden-output", "--max-invocations", str(K * len(tasks) + 8),
            "--max-wall-clock-secs", "43200", "--out", str(out)]
    with open(RECORDS / f"audit-{n}.log", "wb") as log:
        code = subprocess.run(argv, stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT).returncode
    passed, void = {}, set()
    for line in safe_read_text(out / "ledger.jsonl", HOLDOUT).splitlines():
        rec = json.loads(line)
        if rec.get("arm") != "D" or rec.get("task") not in tasks:
            continue
        if rec.get("kind") == "score":
            passed[rec["task"]] = bool(rec["result"]["all_passed"])
        elif rec.get("kind") == "submission" and rec.get("void_reason"):
            void.add(rec["task"])
    return code, passed, void


@gateway("audit")
def main():
    args = sys.argv[1:]
    binary = args[args.index("--binary") + 1] if "--binary" in args else str(REPO / "target" / "debug" / "a2d")
    if not prereqs_ok() or not provider_merged():
        emit(stage="audit", status="prereqs-not-met")
        sys.exit(2)
    with DriverLock():
        suite = sorted(p.name for p in SUITE.iterdir() if TASK_RE.match(p.name))
        if not suite or not clean_suite(suite):
            emit(stage="audit", status="suite-not-clean")
            sys.exit(2)
        audit = load_json(RECORDS / "audit.json", {})
        adj = load_json(RECORDS / "adjudication.json", {})
        due = {tid: needs(audit, adj, tid) for tid in suite}
        tasks = sorted(t for t, r in due.items() if r)
        if not tasks:
            emit(stage="audit", tasks=0, status="nothing-due")
            return
        content = {tid: hashes_digest(tree_hashes(SUITE / tid)) for tid in tasks}   # what this round audits
        n = load_json(RECORDS / "audit-runs.json", {"n": 0})["n"] + 1
        save_json(RECORDS / "audit-runs.json", {"n": n, "started_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())})
        code, passed, void = run_lift(binary, tasks, n)
        for tid in tasks:
            if tid not in passed and tid not in void:
                void.add(tid)                       # no score at all: no information, like a void tuple
            audit.setdefault(tid, {})[due[tid]] = {"passed": passed.get(tid, False) and tid not in void,
                                                  "void": tid in void, "run": n, "content": content[tid]}
        save_json(RECORDS / "audit.json", audit)
        failing = sorted(t for t in tasks if not audit[t][due[t]]["passed"] and not audit[t][due[t]]["void"])
        emit(stage="audit", exit=code, tasks=len(tasks), count=len(tasks) - len(failing) - len(void & set(tasks)),
             missing=len(failing), skipped=len(void & set(tasks)), round=n)
        for t in failing:
            emit(stage="audit-fail", id=t, round=int(due[t][1]))


if __name__ == "__main__":
    main()
