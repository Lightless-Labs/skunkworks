"""Independent-solve audit of the confirmation-3 suite (plan §0.6 step 5; rules frozen in §0.11;
hardened after the early reviews of 2026-10-19).

Codex Sol (medium), arm D, k = 3, once per task per round, on the fixed provider: one
`a2d lift run` over a sub-suite of the tasks that need it, with the hidden output recorded
(for adjudication only; never printed). Round 1 covers every suite task without a round-1
result. Round 2 is the one re-audit: tasks repaired by round-1 adjudication, and tasks whose
round-1 tuple was void (no information; a second void is disclosed and the task kept).

Gates: the latest validation and similarity checks are both clean on exactly this suite
(digest match), and nothing is pending in the similarity state. Before launch (§0.10): no
codex context above the temp root, a canary preflight through the real provider on the
solver lane (the secret in no response, 0 tool events, a --json transcript), and every call's
CODEX_HOME under the holdout dir. Each task's round is reserved durably before launch; a
reservation without a result (an interrupted run) becomes a void round, never a fresh call.
The child inherits the driver lock. A tuple with a tool event, a missing transcript or an
invocation-count mismatch is void; a codex change during the run voids the whole run. Each
run records its git head, binary and codex fingerprint, and each round the content hash of
the task it audited. Results go to records/audit.json; the run's output stays in
$A2D_HOLDOUT_DIR/runs/c3-audit-<n>/. Prints only counts and opaque task ids.

usage: python3 -I tools/holdout/c3_audit.py
"""
import json
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from c3lib import (HOLDOUT, RECORDS, REPO, SUITE, TASK_RE, TESTING, DriverLock, emit, gateway,  # noqa: E402
                   hashes_digest, load_json, prereqs_ok, provider_merged, safe_copy, safe_read_text, save_json,
                   suite_digest, tree_hashes)

sys.path.insert(0, str(REPO / "nano"))
import confirm3  # noqa: E402

CONTRACT = ("task.json", "public_tests.rs", "hidden_tests.rs", "reference.rs")
MUTANTS = tuple(f"m{i}.rs" for i in (1, 2, 3, 4))
SOLVER = ("codex/gpt-6.1-sol", "medium")
K = 3
PENDING_SIM = ("reauthor", "reauthor-done", "reauthor-failed")
BINARY = REPO / "target" / "debug" / "a2d"
CODEX_HOMES = HOLDOUT / "codex-homes"


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


def reconcile(audit):
    """A round reserved by an interrupted run has no result: it is void (no information).
    It is never re-run beyond the registered allowance (void r1 -> one r2; void r2 -> kept)."""
    changed = 0
    for rounds in audit.values():
        for rec in rounds.values():
            if rec.get("reserved"):
                rec.update({"passed": False, "void": True, "interrupted": True})
                del rec["reserved"]
                changed += 1
    return changed


def preflight(binary, lock_fd):
    """§0.10 before audit jobs: clean temp-root ancestry and a canary through the provider on
    the solver lane. True iff the audit may launch."""
    if TESTING:
        return os.environ.get("C3_TEST_PREFLIGHT", "ok") == "ok"
    if not confirm3.isolation_ok():
        return False
    confirm3.A2D = str(binary)
    tag = f"audit-{time.strftime('%Y%m%dT%H%M%SZ', time.gmtime())}"
    secs, leak, tools, audited = confirm3.probe_run(*SOLVER, 1, tag, lock_fd)
    return not leak and tools == 0 and audited >= 1


def run_lift(binary, tasks, n, lock_fd):
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
    argv = [str(binary), "lift", "run", "--experiment", f"C3-AUDIT-{n}", "--unregistered", "--suite", str(sub),
            "--model", SOLVER[0], "--effort", SOLVER[1], "--arms", "D", "--k", str(K), "--jobs", "4",
            "--min-families", "1", "--record-hidden-output", "--max-invocations", str(K * len(tasks) + 8),
            "--max-wall-clock-secs", "43200", "--out", str(out)]
    CODEX_HOMES.mkdir(parents=True, exist_ok=True)
    with open(RECORDS / f"audit-{n}.log", "wb") as log:
        code = subprocess.run(argv, stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT,
                              pass_fds=(lock_fd,), env={**os.environ, "A2D_CODEX_HOME_ROOT": str(CODEX_HOMES)}).returncode
    passed, void = {}, set()
    for line in safe_read_text(out / "ledger.jsonl", HOLDOUT).splitlines():
        rec = json.loads(line)
        if rec.get("arm") != "D" or rec.get("task") not in tasks:
            continue
        if rec.get("kind") == "score":
            passed[rec["task"]] = bool(rec["result"]["all_passed"])
        elif rec.get("kind") == "submission" and rec.get("void_reason"):
            void.add(rec["task"])
    # Per-tuple integrity, as for the confirmation (tool events, transcripts, invocation counts).
    tuples, _ = confirm3.replicate_tuples(out, 0, ["D"], tasks)
    void |= {task for (_, _, task), v in tuples.items() if v["void"] and task in passed}
    return code, passed, void


@gateway("audit")
def main():
    args = sys.argv[1:]
    binary = Path(args[args.index("--binary") + 1]) if "--binary" in args else BINARY
    if binary != BINARY and not TESTING:
        emit(stage="audit", status="binary-override-refused")
        sys.exit(2)
    if not prereqs_ok() or not provider_merged():
        emit(stage="audit", status="prereqs-not-met")
        sys.exit(2)
    with DriverLock() as lock:
        audit = load_json(RECORDS / "audit.json", {})
        if reconcile(audit):
            save_json(RECORDS / "audit.json", audit)
            emit(stage="audit", status="interrupted-rounds-voided")
        suite = sorted(p.name for p in SUITE.iterdir() if TASK_RE.match(p.name))
        if not suite or not clean_suite(suite):
            emit(stage="audit", status="suite-not-clean")
            sys.exit(2)
        adj = load_json(RECORDS / "adjudication.json", {})
        due = {tid: needs(audit, adj, tid) for tid in suite}
        tasks = sorted(t for t, r in due.items() if r)
        if not tasks:
            emit(stage="audit", tasks=0, status="nothing-due")
            return
        if not preflight(binary, lock.fd):
            emit(stage="audit", status="preflight-failed")
            sys.exit(2)
        content = {tid: hashes_digest(tree_hashes(SUITE / tid)) for tid in tasks}   # what this round audits
        before = None if TESTING else confirm3.codex_fingerprint()
        runs = load_json(RECORDS / "audit-runs.json", {"n": 0})
        n = runs["n"] + 1
        head = subprocess.run(["git", "-C", str(REPO), "rev-parse", "HEAD"], capture_output=True, text=True).stdout.strip()
        runs.setdefault("runs", {})[str(n)] = {
            "started_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()), "head": head,
            "binary_sha256": confirm3.sha256(binary) if binary.exists() else None, "codex": before}
        runs["n"] = n
        save_json(RECORDS / "audit-runs.json", runs)
        for tid in tasks:                                    # durable reservation before any call
            audit.setdefault(tid, {})[due[tid]] = {"reserved": True, "run": n, "content": content[tid]}
        save_json(RECORDS / "audit.json", audit)
        code, passed, void = run_lift(binary, tasks, n, lock.fd)
        after = None if TESTING else confirm3.codex_fingerprint()
        bracket_ok = after == before
        runs["runs"][str(n)].update({"codex_after": after, "bracket_ok": bracket_ok})
        save_json(RECORDS / "audit-runs.json", runs)
        for tid in tasks:
            if not bracket_ok or (tid not in passed and tid not in void):
                void.add(tid)                       # no score, or another codex ran: no information
            audit[tid][due[tid]] = {"passed": passed.get(tid, False) and tid not in void,
                                    "void": tid in void, "run": n, "content": content[tid]}
        save_json(RECORDS / "audit.json", audit)
        failing = sorted(t for t in tasks if not audit[t][due[t]]["passed"] and not audit[t][due[t]]["void"])
        emit(stage="audit", exit=code, tasks=len(tasks), count=len(tasks) - len(failing) - len(void & set(tasks)),
             missing=len(failing), skipped=len(void & set(tasks)), round=n)
        for t in failing:
            emit(stage="audit-fail", id=t, round=int(due[t][1]))


if __name__ == "__main__":
    main()
