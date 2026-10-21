"""Two-adjudicator review of gym v3 audit outcomes (rubric M3, M4; authoring/ADJUDICATE.md,
authoring/REPAIR.md). Adapted from tools/holdout/c3_adjudicate.py.

Per family and mode, for every task with an informative audit round not yet adjudicated:
- failing hidden tests (or `*` when the submission did not compile) go to TWO independent Sol
  adjudicators, A and B, each a fresh sandboxed job with its own workdir and CODEX_HOME; neither
  sees the other's verdicts;
- M4: on a task's first informative round, adjudicator B also reviews 2 seeded hidden tests the
  audit PASSED (gym_rubric.sample_passing; seed bound to the task and its audited content), so
  every audited task gets a spec review, including tasks the solver fully passed.
- M3: a failing test is DETERMINED only if both A and B say DETERMINED with a valid verbatim quote
  (>= 8 words, a substring of the requirements) and a named excluded reading; a sampled test only
  if B does. Anything else needs a requirements-only repair.
- mode "repair" (round r1): a third job (REPAIR.md) writes out/<id>/task.json with only the
  requirements changed for the tasks that need it; applied the c3_fix way (verified files,
  snapshot, verify, restore). The task is then re-assembled, re-validated, re-checked and
  re-audited once (gym_audit.py round 2). A missing or malformed repair excludes the task.
- mode "classify" (round r2): no repair; a test still not determined excludes the task.
- A determined solver failure is KEPT: tasks are never dropped for failing the solver
  (gym_rubric.EXCLUSION_REASONS).
Caps: 2 jobs per family, mode and role, reserved before launch; a role failing twice excludes
its tasks (fail closed). Verdict files are parsed strictly against the listed tests. Prints
only counts and opaque ids.

usage: python3 -I tools/gym/gym_adjudicate.py [--parallel N]
"""
import hashlib
import json
import os
import re
import shutil
import sys
import threading
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import gym_rubric  # noqa: E402
from gymlib import (AUTHORING_FILES, GYM, MAX_VOID_ROUNDS, RECORDS, ROOT, RUNS, SUITE, TASK_RE, DriverLock,  # noqa: E402
                    latest_round, task_content, void_rounds,
                    LaunchGateClosed, StopRequested, emit, gate_or_exit, gateway, isolation_check, load_json,
                    parse_task_json, safe_read_bytes, strict_json_doc,
                    prereqs_ok, run_codex, safe_copy, safe_read_text, safe_write_text, save_json, stop_requested,
                    suspicious_commands, tree_hashes)
from gymvendor import lines, strict_json, test_names  # noqa: E402

MAX_JOBS = 2
FILES = ("task.json", "public_tests.rs", "hidden_tests.rs", "reference.rs")
MUTANTS = tuple(f"m{i}.rs" for i in (1, 2, 3, 4))
FAILED = re.compile(r"^test (\S+) \.\.\. FAILED$")
TEST_LINE = re.compile(r"^test (\S+) \.\.\. (ok|FAILED|ignored)$")
M4_SEED = "gym-v3-m4"
_lock = threading.Lock()


def _bare(name):
    return name.rsplit("::", 1)[-1]


def test_statuses(hidden_output):
    """{bare test name: ok | FAILED | ignored} from libtest lines of a hidden-test run."""
    out = {}
    for m in map(TEST_LINE.match, lines(hidden_output)):
        if m:
            out[_bare(m.group(1))] = m.group(2)
    return out


def due(audit, adj, suite, cur):
    """{(family, mode): [task entry]} of rounds awaiting adjudication, each on the task's CURRENT
    content: the latest informative round (mode "repair" until the task has had a requirements
    repair, then "classify"), or, when the current content has 2 void rounds, the latest void
    round (kind "void": the M4 spec review without solver evidence, mode "classify")."""
    out = {}
    for tid in suite:
        a, j = audit.get(tid, {}), adj.get(tid, {})
        name, last = latest_round(a)
        if name is None or last.get("reserved") or last.get("content") != cur[tid] or name in j:
            continue
        if last["void"]:
            if void_rounds(a, cur[tid]) < MAX_VOID_ROUNDS:
                continue
            kind, mode = "void", "classify"
        else:
            kind = "audit"
            mode = "classify" if any((r or {}).get("repaired") for r in j.values()) else "repair"
        out.setdefault((tid.rsplit("-l", 1)[0], mode), []).append(
            {"task": tid, "round": name, "run": last["run"], "passed": bool(last.get("passed")),
             "content": last["content"], "kind": kind})
    return out


def solver_dir(entry):
    return RUNS / f"g3-audit-{entry['run']}" / "submissions" / "D" / entry["task"]


def audit_score(entry):
    """The audit ledger's score record for the task (arm D), or None; read with lines() + strict
    JSON from the sealed run dir."""
    try:
        text = safe_read_text(RUNS / f"g3-audit-{entry['run']}" / "ledger.jsonl", ROOT)
    except OSError:
        return None
    for line in lines(text):
        if not line.strip():
            continue
        try:
            r = strict_json(line)
        except ValueError:
            continue
        if isinstance(r, dict) and r.get("kind") == "score" and r.get("arm") == "D" and r.get("task") == entry["task"] \
                and isinstance(r.get("result"), dict):
            return r["result"]
    return None


def plan_tests(entry):
    """Fill in the failing tests (M3) and the 2 seeded spec-review tests (M4) for one task entry,
    from positive, task-bound evidence only (the round is on the current content, due()):
    - passed: the audit's score record shows every hidden test passed; M4 samples any 2 hidden tests;
    - failed and compiled: the recorded hidden output (read only for failed tasks) has test
      lines; every hidden test it does not show passing (FAILED, or never reported, e.g. after a
      timeout) goes to M3, and M4 samples 2 of the passing ones;
    - did not compile (score `compiled` false): `*` goes to M3 and M4 samples any 2 hidden tests;
    - void twice: M4 samples any 2 hidden tests (no solver evidence).
    Without that evidence `evidence` is False and the task stays pending (never adjudicated OK)."""
    tid = entry["task"]
    names = test_names((SUITE / tid / "hidden_tests.rs").read_text())
    entry.update({"failing": [], "sampled": [], "evidence": False})
    if entry["kind"] == "void":
        entry.update(sampled=gym_rubric.sample_passing(tid, entry["content"], names, [], M4_SEED), evidence=True)
        return entry
    score = audit_score(entry)
    if not isinstance(score, dict):
        return entry
    if entry["passed"]:
        if score.get("all_passed") is True and isinstance(score.get("expected"), int) and score["expected"] > 0 \
                and score.get("passed") == score["expected"]:
            entry.update(sampled=gym_rubric.sample_passing(tid, entry["content"], names, [], M4_SEED), evidence=True)
        return entry
    if score.get("compiled") is False:
        entry.update(failing=["*"], sampled=gym_rubric.sample_passing(tid, entry["content"], names, [], M4_SEED),
                     evidence=True)
        return entry
    try:
        statuses = test_statuses(safe_read_text(solver_dir(entry) / "r0.hidden.txt", ROOT))
    except OSError:
        return entry
    if not statuses or not set(statuses) <= names:
        return entry                                   # missing, empty or foreign evidence
    passing = sorted(n for n in names if statuses.get(n) == "ok")
    failing = sorted(n for n in names if statuses.get(n) != "ok")
    if not failing:
        return entry                                   # inconsistent with the failed score
    entry.update(failing=failing, sampled=gym_rubric.sample_passing(tid, entry["content"], passing, [], M4_SEED),
                 evidence=True)
    return entry


def prepare(w, tasks, role, mode):
    if w.exists():
        shutil.rmtree(w)
    w.mkdir(parents=True)
    expected, rows = {}, []
    for e in tasks:
        tid = e["task"]
        for f in FILES:
            safe_copy(SUITE / tid / f, w / "tasks" / tid / f, SUITE, w)
        for m in MUTANTS:
            safe_copy(SUITE / tid / "mutants" / m, w / "tasks" / tid / "mutants" / m, SUITE, w)
        if role != "R":
            (w / "solver" / tid).mkdir(parents=True, exist_ok=True)
            if e["kind"] == "void":
                note = "(no solver evidence: both audit attempts on this content were void)\n"
            elif e["passed"]:
                note = "(the submission passed every hidden test)\n"
            else:
                note = None
                safe_copy(solver_dir(e) / "r0.hidden.txt", w / "solver" / tid / "hidden_output.txt", ROOT, w)
            if note:
                safe_write_text(w / "solver" / tid / "hidden_output.txt", note, w)
            if e["kind"] != "void" and os.path.lexists(solver_dir(e) / "r0.rs"):
                safe_copy(solver_dir(e) / "r0.rs", w / "solver" / tid / "submission.rs", ROOT, w)
        if role == "A":
            listed = [(t, "failing") for t in e["failing"]]
        elif role == "B":
            listed = [(t, "failing") for t in e["failing"]] + [(t, "sampled") for t in e["sampled"]]
        else:
            listed = [(t, "repair") for t in e["repair"]]
        expected[tid] = [t for t, _ in listed]
        rows += [f"{tid}\t{t}\t{k}" if role != "R" else f"{tid}\t{t}" for t, k in listed]
    safe_write_text(w / "LIST.tsv", "\n".join(rows) + "\n", w)
    if role == "R":
        safe_copy(AUTHORING_FILES / "SPEC.md", w / "SPEC.md", AUTHORING_FILES, w)
        safe_write_text(w / "INSTRUCTIONS.md", (AUTHORING_FILES / "REPAIR.md").read_text(), w)
    else:
        safe_copy(AUTHORING_FILES / "check.sh", w / "check.sh", AUTHORING_FILES, w, mode=0o755)
        safe_write_text(w / "INSTRUCTIONS.md", (AUTHORING_FILES / "ADJUDICATE.md").read_text(), w)
    return expected


def unit_id(fid, mode, tasks):
    """A unit of adjudication: one family, mode and set of (task, round, content). A later round
    (new content) is a new unit, with fresh verdicts and its own job caps."""
    sig = json.dumps(sorted((e["task"], e["round"], e["content"]) for e in tasks))
    return f"{fid}-{mode}-{hashlib.sha256(sig.encode()).hexdigest()[:8]}"


def run_role(fid, mode, role, tasks, unit):
    """One adjudicator (A, B) or repair (R) job. Returns (status, payload): ("ok", verdicts or
    workdir), ("failed", None), ("capped", None) or ("stopped", None)."""
    key = f"{unit}-{role}"
    with _lock:
        if stop_requested():
            return "stopped", None
        jobs = load_json(RECORDS / "adj-jobs.json", {})
        if jobs.get(key, 0) >= MAX_JOBS:
            return "capped", None
    w = GYM / "_adj" / key
    expected = prepare(w, tasks, role, mode)
    if not isolation_check(w):
        with _lock:
            emit(stage="adjudicate-isolation", id=fid, ok=False)
        return "stopped", None
    with _lock:
        jobs = load_json(RECORDS / "adj-jobs.json", {})
        jobs[key] = jobs.get(key, 0) + 1
        save_json(RECORDS / "adj-jobs.json", jobs)
        attempt = jobs[key]
    record = f"adj-{key}-a{attempt}"
    try:
        code, other = run_codex(w, record, effort="high", timeout=5400)
    except (StopRequested, LaunchGateClosed):
        with _lock:
            jobs = load_json(RECORDS / "adj-jobs.json", {})
            jobs[key] -= 1
            save_json(RECORDS / "adj-jobs.json", jobs)
        return "stopped", None
    susp = suspicious_commands(record, w)
    clean = code == 0 and other == 0 and susp == 0
    with _lock:
        emit(stage=f"adjudicate-{role.lower()}", id=fid, exit=code, other_events=other, skipped=susp, ok=clean,
             round=attempt)
    if not clean:
        return "failed", None
    if role == "R":
        return "ok", w
    try:
        verdicts = gym_rubric.parse_verdicts(safe_read_text(w / "VERDICTS.jsonl", w), expected)
    except (OSError, PermissionError):
        verdicts = None
    return ("ok", verdicts) if verdicts is not None else ("failed", None)


def repair_keeps_rubric(fid, tid, old_req, new_req):
    """A requirements-only repair rewords or extends existing clauses only: the clause ids stay
    exactly R1..Rn, and the unchanged hidden tests, public tests, reference and rubric.json still
    pass every mechanical rubric check (M1 coverage, M2 quotes, M5, clause floor, tag evidence)
    against the new text. Otherwise the repair is refused: the task is excluded as
    underdetermined, without spending the family's FIX rounds (hidden tests are never edited)."""
    d = SUITE / tid
    try:
        if sorted(gym_rubric.clauses(new_req)) != sorted(gym_rubric.clauses(old_req)):
            return False
        cls = load_json(RECORDS / "active.json", {}).get("classes", {}).get(fid)
        rubric = strict_json_doc((d / "rubric.json").read_bytes())
        r = gym_rubric.check_task(new_req, (d / "hidden_tests.rs").read_text(), (d / "public_tests.rs").read_text(),
                                  (d / "reference.rs").read_text(), rubric, int(tid[-1]), cls)
    except (OSError, ValueError):
        return False
    return r["ok"]


def apply_repair(fid, tid, w):
    """c3_fix pattern: verified author files, snapshot, copy the repair in, verify, else restore.
    Only task.json, and in it only `requirements`, may change; the new text must keep the rubric
    (repair_keeps_rubric)."""
    from gym_author import complete
    status_path = RECORDS / f"author-{fid}" / "status.json"
    st = load_json(status_path, {})
    out = GYM / fid / "out" / fid
    if not st.get("done") or st.get("hashes") != tree_hashes(out):
        return False
    src = w / "out" / tid
    try:
        names = sorted(p.name for p in src.iterdir()) if src.is_dir() and not src.is_symlink() else []
        if names != ["task.json"]:
            return False
        old = parse_task_json(safe_read_bytes(out / tid / "task.json", GYM))
        new = parse_task_json(safe_read_bytes(src / "task.json", w))
        if not new["requirements"].strip() or new["requirements"] == old["requirements"] \
                or {k: v for k, v in new.items() if k != "requirements"} != \
                {k: v for k, v in old.items() if k != "requirements"}:
            return False
        if not repair_keeps_rubric(fid, tid, old["requirements"], new["requirements"]):
            return False
    except (OSError, PermissionError, ValueError):
        return False
    snapshot = RECORDS / f"adj-{fid}" / "snapshot"
    if snapshot.exists():
        shutil.rmtree(snapshot)
    shutil.copytree(out, snapshot, symlinks=True)
    try:
        safe_copy(src / "task.json", out / tid / "task.json", w, GYM)
        ok = complete(fid)
    except (OSError, PermissionError, ValueError):
        ok = False
    if ok:
        st["hashes"] = tree_hashes(out)
        save_json(status_path, st)
    else:
        shutil.rmtree(GYM / fid / "out")
        (GYM / fid / "out").mkdir()
        shutil.copytree(snapshot, out, symlinks=True)
    return ok


def combine_task(e, a, b):
    """{test: {"kind", "combined"}} for one task, and whether it needs a repair. A quote must be
    a literal substring of a clause the test's own trace cites (gym_rubric.quotable)."""
    req = parse_task_json((SUITE / e["task"] / "task.json").read_bytes())["requirements"]
    hidden = (SUITE / e["task"] / "hidden_tests.rs").read_text()
    a, b = (a or {}).get(e["task"], {}), (b or {}).get(e["task"], {})
    tests = {}
    for t in e["failing"]:
        tests[t] = {"kind": "failing",
                    "combined": gym_rubric.combine(a.get(t), b.get(t), gym_rubric.quotable(req, hidden, t))}
    for t in e["sampled"]:
        ok = gym_rubric.valid_determined(b.get(t), gym_rubric.quotable(req, hidden, t))
        tests[t] = {"kind": "sampled", "combined": "DETERMINED" if ok else "REPAIR"}
    return tests, any(v["combined"] == "REPAIR" for v in tests.values())


def _exclude(tids, reason):
    excluded = load_json(RECORDS / "excluded.json", {"tasks": [], "reasons": {}})
    excluded.setdefault("reasons", {})
    excluded["tasks"] = sorted(set(excluded["tasks"]) | set(tids))
    for t in tids:
        excluded["reasons"].setdefault(t.rsplit("-l", 1)[0], reason)
    save_json(RECORDS / "excluded.json", excluded)


def one(key, tasks):
    fid, mode = key
    tasks = [plan_tests(e) for e in tasks]
    missing = [e["task"] for e in tasks if not e["evidence"]]
    if missing:
        with _lock:
            emit(stage="adjudicate", id=fid, status="evidence-missing", pending=len(missing))
    tasks = [e for e in tasks if e["evidence"]]                # the rest stay pending
    if not tasks:
        return
    uid = unit_id(fid, mode, tasks)
    unit = RECORDS / "adj-units" / f"{uid}.json"
    res = load_json(unit, {})
    roles = ["A", "B"] if any(e["failing"] for e in tasks) else ["B"]
    if not any(e["failing"] or e["sampled"] for e in tasks):
        roles = []
    for role in roles:
        if res.get(role) is not None:
            continue
        status, verdicts = run_role(fid, mode, role, tasks, uid)
        if status == "stopped":
            return
        res[role] = verdicts
        res.setdefault("capped", {})[role] = status in ("capped", "failed") and \
            load_json(RECORDS / "adj-jobs.json", {}).get(f"{uid}-{role}", 0) >= MAX_JOBS
        save_json(unit, res)
    failed_roles = [r for r in roles if res.get(r) is None]
    if failed_roles:
        if all(res.get("capped", {}).get(r) for r in failed_roles):
            with _lock:
                adj = load_json(RECORDS / "adjudication.json", {})
                for e in tasks:
                    adj.setdefault(e["task"], {})[e["round"]] = {"tests": None, "repaired": False, "excluded": True}
                save_json(RECORDS / "adjudication.json", adj)
                _exclude([e["task"] for e in tasks], "adjudication-failed")
                emit(stage="adjudicate", id=fid, status="excluded-unadjudicated", tasks=len(tasks))
        return
    recs, need = {}, []
    for e in tasks:
        tests, needs_repair = combine_task(e, res.get("A"), res.get("B"))
        recs[e["task"]] = {"tests": tests, "repaired": False, "excluded": False, "m4": e["sampled"]}
        if needs_repair:
            e["repair"] = sorted(t for t, v in tests.items() if v["combined"] == "REPAIR")
            need.append(e)
    if need and mode == "repair":
        status, w = run_role(fid, mode, "R", need, uid)
        if status == "stopped":
            return
        if status == "failed" and load_json(RECORDS / "adj-jobs.json", {}).get(f"{uid}-R", 0) < MAX_JOBS:
            return                                                # one more repair attempt next run
        for e in need:
            recs[e["task"]]["repaired"] = status == "ok" and apply_repair(fid, e["task"], w)
    excluded = [e["task"] for e in need if not recs[e["task"]]["repaired"]]
    for t in excluded:
        recs[t]["excluded"] = True
    with _lock:
        adj = load_json(RECORDS / "adjudication.json", {})
        for e in tasks:
            adj.setdefault(e["task"], {})[e["round"]] = recs[e["task"]]
        save_json(RECORDS / "adjudication.json", adj)
        if excluded:
            _exclude(excluded, "adjudication-underdetermined")
        emit(stage="adjudicate", id=fid, ok=True, tasks=len(tasks), conflicts=len(need),
             count=sum(r["repaired"] for r in recs.values()), skipped=len(excluded))


@gateway("adjudicate")
def main():
    args = sys.argv[1:]
    parallel = int(args[args.index("--parallel") + 1]) if "--parallel" in args else 3
    if not 1 <= parallel <= 3:
        raise ValueError("parallel")
    gate_or_exit("adjudicate")
    if not prereqs_ok():
        emit(stage="adjudicate", status="prereqs-not-met")
        sys.exit(2)
    with DriverLock():
        audit = load_json(RECORDS / "audit.json", {})
        adj = load_json(RECORDS / "adjudication.json", {})
        suite = sorted(p.name for p in SUITE.iterdir() if TASK_RE.match(p.name))
        work = due(audit, adj, suite, {t: task_content(t) for t in suite})
        emit(stage="adjudicate", families=len({f for f, _ in work}), tasks=sum(len(v) for v in work.values()))
        with ThreadPoolExecutor(max_workers=parallel) as ex:
            list(ex.map(lambda kv: one(*kv), sorted(work.items())))
        adj = load_json(RECORDS / "adjudication.json", {})
        recs = [r for t in adj.values() for r in t.values() if r]
        emit(stage="adjudicate", done=len(recs), count=sum(r["repaired"] for r in recs),
             skipped=sum(r["excluded"] for r in recs))


if __name__ == "__main__":
    main()
