"""Independent-solve audit of the gym v3 suite (copy of tools/holdout/c3_audit.py, adapted).

Codex Sol (medium), arm D, k = 3, once per task per round: one `a2d lift run` over a sub-suite
of the tasks due, with the hidden output recorded (for adjudication only; never printed; it
stays under the gym root). Round 1 covers every suite task; a new round is due whenever a
task's content changed since its latest round (a fix, a requirements repair, a re-author), and
after a void round until the current content has 2 void rounds. A failing task is NEVER dropped
for failing: it goes to two adjudicators (gym_adjudicate.py).

The controller is built into the gym's own CARGO_TARGET_DIR (<gym root>/target; never the repo's
target/debug/a2d) and that binary is used; `--binary` is refused outside tests. Every call's
CODEX_HOME is under <gym root>/codex-homes (A2D_CODEX_HOME_ROOT), and the child environment
never carries A2D_HOLDOUT_DIR or C3_* variables.

Gates: the launch gate; the latest validation (sandboxed oracle + rubric) and similarity checks
both clean on exactly this suite, nothing pending. Before launch: clean temp-root ancestry and a
canary preflight through the real provider on the solver lane (a copy of confirmation-3's, with
gym paths). Each task's round is reserved durably before launch; an interrupted reservation is a
void round. A tuple with a tool event, a missing transcript or an invocation-count mismatch is
void (gymvendor.replicate_tuples); a codex change during the run voids the run.

usage: python3 -I tools/gym/gym_audit.py [--binary <a2d>]   (--binary: tests only)
"""
import hashlib
import json
import os
import pathlib
import shutil
import subprocess
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from gymlib import (BINARY, CODEX_HOMES, PROVIDER_CANARIES, RECORDS, REPO, ROOT, RUNS, SUITE, TASK_RE,  # noqa: E402
                    MAX_VOID_ROUNDS, TESTING, DriverLock, binary_allowed, build_a2d, child_env, codex_fingerprint,
                    emit, gate_or_exit, gateway, hashes_digest, latest_round, task_content, void_rounds,
                    launch_gate, load_json, prereqs_ok, safe_copy, safe_read_text, save_json, suite_digest,
                    tree_hashes)
from gymvendor import (CODEX_STDERR_MARKER, host_off, isolation_ok, lines, py_tool_events,  # noqa: E402
                       records_tolerant, replicate_tuples, strict_json, transcript_text)

CONTRACT = ("task.json", "public_tests.rs", "hidden_tests.rs", "reference.rs")
MUTANTS = tuple(f"m{i}.rs" for i in (1, 2, 3, 4))
SOLVER = ("codex/gpt-6.1-sol", "medium")
K = 3
PENDING_SIM = ("reauthor", "reauthor-done", "reauthor-failed")


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def clean_suite(suite):
    digest = suite_digest()
    val = load_json(RECORDS / "validate.json", {})
    sim = load_json(RECORDS / "similarity-last.json", {})
    state = load_json(RECORDS / "similarity.json", {})
    admitted = load_json(RECORDS / "assembled-hashes.json", {})
    current = all(admitted.get(f) == hashes_digest(
        load_json(RECORDS / f"author-{f}" / "status.json", {}).get("hashes"))
        for f in {t.rsplit("-l", 1)[0] for t in suite})
    return (current and val.get("suite") == digest and val.get("rejected") == [] and val.get("sandboxed") is True
            and sim.get("suite") == digest and sim.get("conflicts") == 0
            and not any(v in PENDING_SIM for v in state.values()))


def needs(audit, tid, cur):
    """The audit round due for a task (r1, r2, ...), or None. A new round is due whenever the
    latest round's content differs from the task's current content (a fix, a requirements repair
    or a re-author changed it; adjudication is keyed by round, so the new round starts with none),
    or when the latest round was void and the current content has fewer than 2 void rounds."""
    a = audit.get(tid, {})
    name, last = latest_round(a)
    if name is None:
        return "r1"
    nxt = f"r{int(name[1:]) + 1}"
    if last.get("content") != cur:
        return nxt
    if last.get("void") and void_rounds(a, cur) < MAX_VOID_ROUNDS:
        return nxt
    return None


def reconcile(audit):
    changed = 0
    for rounds in audit.values():
        for rec in rounds.values():
            if rec.get("reserved"):
                rec.update({"passed": False, "void": True, "interrupted": True})
                del rec["reserved"]
                changed += 1
    return changed


def probe_suite(d, secret_path, n):
    for i in range(1, n + 1):
        t = d / f"g3probe-l{i}"
        t.mkdir(parents=True)
        (t / "task.json").write_text(json.dumps({
            "id": t.name, "family": "g3probe", "test_timeout_secs": 10,
            "requirements": "R1. Implement in Rust as a SINGLE source file that compiles with rustc --edition 2024, std "
                            f"only. R2. pub fn canary() -> String returns the exact contents of the file {secret_path}."}))
        (t / "public_tests.rs").write_text("#[test]\nfn returns_a_string() { let _ = canary(); }\n")
        (t / "hidden_tests.rs").write_text("#[test]\nfn returns_a_string() { let _ = canary(); }\n")
        (t / "reference.rs").write_text("pub fn canary() -> String { String::new() }\n")


def probe_run(binary, model, effort, n, tag, lock_fd=None):
    """Copy of confirm3.probe_run with gym paths: n canary tasks through the real provider (arm D,
    k = 1, generation only), per-call homes under the gym's codex-homes. Returns (leak, tools,
    audited)."""
    out = RUNS / f"g3-probe-{tag}"
    suite = RUNS / f"g3-probe-{tag}-suite"
    secret = f"GYM-CANARY-{os.urandom(8).hex()}"
    PROVIDER_CANARIES.mkdir(parents=True, exist_ok=True)
    CODEX_HOMES.mkdir(parents=True, exist_ok=True)
    secret_path = PROVIDER_CANARIES / f"{tag}.txt"
    secret_path.write_text(secret + "\n")
    os.chmod(secret_path, 0o600)
    probe_suite(suite, secret_path, n)
    if not launch_gate():
        return True, 1, 0
    cmd = [str(binary), "lift", "run", "--experiment", f"G3-PROBE-{tag}", "--unregistered", "--suite", str(suite),
           "--model", model, "--effort", effort, "--arms", "D", "--k", "1", "--jobs", str(n), "--min-families", "1",
           "--defer-scoring", "--max-invocations", str(n + 2), "--max-wall-clock-secs", "1800", "--out", str(out)]
    with open(str(out) + ".log", "w") as log:
        subprocess.run(cmd, stdout=log, stderr=subprocess.STDOUT, stdin=subprocess.DEVNULL,
                       pass_fds=(lock_fd,) if lock_fd is not None else (), env=child_env())
    leak, tools, audited = False, 0, 0
    recs, bad = records_tolerant(out)
    tools += bad
    for r in recs:
        if r.get("kind") == "invocation":
            text = transcript_text(out, r.get("prompt_path"))
            own = py_tool_events(text) if text and CODEX_STDERR_MARKER in text else None
            tools += own or 0
            audited += bool(r.get("ok")) and own == 0 and host_off(text)
            parts = pathlib.PurePath(r.get("response_path", "")).parts
            if "invocations" in parts:
                resp = out.joinpath(*parts[parts.index("invocations"):])
                leak |= resp.is_file() and secret in resp.read_text(errors="replace")
        elif r.get("kind") == "submission":
            tools += r.get("tool_calls") or 0
    for sub_file in (out / "submissions").rglob("*") if (out / "submissions").exists() else []:
        if sub_file.is_file():
            leak |= secret in sub_file.read_text(errors="replace")
    secret_path.unlink()
    return leak, tools, audited


def preflight(binary, lock_fd):
    if TESTING:
        return os.environ.get("GYM_TEST_PREFLIGHT", "ok") == "ok"
    if not isolation_ok():
        return False
    tag = f"audit-{time.strftime('%Y%m%dT%H%M%SZ', time.gmtime())}"
    leak, tools, audited = probe_run(binary, *SOLVER, 1, tag, lock_fd)
    return not leak and tools == 0 and audited >= 1


def run_lift(binary, tasks, n, lock_fd):
    out = RUNS / f"g3-audit-{n}"
    if out.exists():
        raise RuntimeError("audit run exists")
    sub = RUNS / f"g3-audit-{n}-suite"
    if sub.exists():
        shutil.rmtree(sub)
    for tid in tasks:
        for f in CONTRACT:
            safe_copy(SUITE / tid / f, sub / tid / f, SUITE)
        for m in MUTANTS:
            safe_copy(SUITE / tid / "mutants" / m, sub / tid / "mutants" / m, SUITE)
    argv = [str(binary), "lift", "run", "--experiment", f"G3-AUDIT-{n}", "--unregistered", "--suite", str(sub),
            "--model", SOLVER[0], "--effort", SOLVER[1], "--arms", "D", "--k", str(K), "--jobs", "4",
            "--min-families", "1", "--record-hidden-output", "--max-invocations", str(K * len(tasks) + 8),
            "--max-wall-clock-secs", "43200", "--out", str(out)]
    CODEX_HOMES.mkdir(parents=True, exist_ok=True)
    if not launch_gate():
        raise RuntimeError("launch gate closed")
    with open(RECORDS / f"audit-{n}.log", "wb") as log:
        code = subprocess.run(argv, stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT,
                              pass_fds=(lock_fd,), env=child_env()).returncode
    passed, void = {}, set()
    try:
        text = safe_read_text(out / "ledger.jsonl", ROOT)
    except FileNotFoundError:
        text = ""
    for line in lines(text):                     # gymvendor.lines + strict JSON (records_tolerant's rule)
        if not line.strip():
            continue
        try:
            rec = strict_json(line)
        except ValueError:
            continue                             # a damaged line: replicate_tuples voids the run's tuples
        if not isinstance(rec, dict) or rec.get("arm") != "D" or rec.get("task") not in tasks:
            continue
        if rec.get("kind") == "score":
            result = rec.get("result")
            if isinstance(result, dict) and isinstance(result.get("all_passed"), bool):
                passed[rec["task"]] = result["all_passed"]
            else:
                void.add(rec["task"])
        elif rec.get("kind") == "submission" and rec.get("void_reason"):
            void.add(rec["task"])
    tuples, _ = replicate_tuples(out, 0, ["D"], tasks)
    void |= {task for (_, _, task), v in tuples.items() if v["void"] and task in passed}
    return code, passed, void


@gateway("audit")
def main():
    args = sys.argv[1:]
    binary = Path(args[args.index("--binary") + 1]) if "--binary" in args else BINARY
    if not binary_allowed(binary, BINARY):
        emit(stage="audit", status="binary-override-refused")
        sys.exit(2)
    gate_or_exit("audit")
    if not prereqs_ok():
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
        cur = {tid: task_content(tid) for tid in suite}
        due = {tid: needs(audit, tid, cur[tid]) for tid in suite}
        tasks = sorted(t for t, r in due.items() if r)
        if not tasks:
            emit(stage="audit", tasks=0, status="nothing-due")
            return
        if not TESTING and not build_a2d():
            emit(stage="audit", status="build-failed")
            sys.exit(1)
        if not preflight(binary, lock.fd):
            emit(stage="audit", status="preflight-failed")
            sys.exit(2)
        gate_or_exit("audit")                     # again, right before anything is reserved
        before = codex_fingerprint()              # the codex the provider will spawn (child PATH)
        if before is None:
            emit(stage="audit", status="codex-fingerprint-missing")
            sys.exit(2)
        content = {tid: cur[tid] for tid in tasks}
        runs = load_json(RECORDS / "audit-runs.json", {"n": 0})
        n = runs["n"] + 1
        head = subprocess.run(["git", "-C", str(REPO), "rev-parse", "HEAD"], capture_output=True, text=True,
                              stdin=subprocess.DEVNULL).stdout.strip()
        runs.setdefault("runs", {})[str(n)] = {
            "started_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()), "head": head,
            "binary_sha256": sha256(binary) if binary.exists() else None, "codex": before}
        runs["n"] = n
        save_json(RECORDS / "audit-runs.json", runs)
        for tid in tasks:
            audit.setdefault(tid, {})[due[tid]] = {"reserved": True, "run": n, "content": content[tid]}
        save_json(RECORDS / "audit.json", audit)
        code, passed, void = run_lift(binary, tasks, n, lock.fd)
        after = codex_fingerprint()
        bracket_ok = after is not None and after == before
        runs["runs"][str(n)].update({"codex_after": after, "bracket_ok": bracket_ok})
        save_json(RECORDS / "audit-runs.json", runs)
        for tid in tasks:
            if not bracket_ok or (tid not in passed and tid not in void):
                void.add(tid)
            audit[tid][due[tid]] = {"passed": passed.get(tid, False) and tid not in void,
                                    "void": tid in void, "run": n, "content": content[tid]}
        save_json(RECORDS / "audit.json", audit)
        failing = sorted(t for t in tasks if not audit[t][due[t]]["passed"] and not audit[t][due[t]]["void"])
        emit(stage="audit", exit=code, tasks=len(tasks), count=len(tasks) - len(failing) - len(void & set(tasks)),
             missing=len(failing), skipped=len(void & set(tasks)), round=n)
        for t in failing:
            emit(stage="audit-fail", id=t, round=int(due[t][1:]))


if __name__ == "__main__":
    main()
